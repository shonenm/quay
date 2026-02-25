mod app;
mod config;
mod connection;
mod dev;
mod event;
mod forward;
mod port;
mod preset;
mod theme;
mod ui;

use anyhow::Result;
use app::{App, ConnectionPopupMode, Filter, ForwardInput, InputMode, Popup};
use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use event::{
    Action, AppEvent, handle_connection_input_key, handle_connection_key, handle_forward_key,
    handle_key, handle_mouse, handle_popup_key, handle_preset_key, handle_search_key,
};
use futures::StreamExt;
use port::PortEntry;
use ratatui::prelude::*;
use std::collections::HashMap;
use std::io::{self, stdout};
use std::time::Duration;

fn save_forwards(app: &mut app::App) {
    let persisted = forward::Forwards::from_runtime(&app.ssh_forwards, &app.connections);
    if let Err(e) = persisted.save() {
        app.set_status(&format!("Forward save failed: {e}"));
    }
}

async fn refresh_and_save(app: &mut App) {
    match port::collect_all(
        app.remote_host.as_deref(),
        app.docker_target.as_deref(),
        app.known_forwards(),
    )
    .await
    {
        Ok(entries) => {
            if app.set_entries(entries) {
                save_forwards(app);
            }
        }
        Err(e) => app.set_status(&format!("Refresh failed: {e}")),
    }
}

async fn resolve_container_info(app: &mut App) {
    if let Some(ref target) = app.docker_target {
        match port::docker::get_container_info(target, app.remote_host.as_deref()).await {
            Ok(info) => {
                app.container_ip = Some(info.ip);
                app.docker_port_mappings = info.port_mappings;
            }
            Err(e) => app.set_status(&format!("Container info lookup failed: {e}")),
        }
    }
}

fn resolve_docker_forward(
    container_port: u16,
    docker_port_mappings: &HashMap<u16, u16>,
    container_ip: Option<&str>,
) -> Option<(String, u16)> {
    if let Some(&host_port) = docker_port_mappings.get(&container_port) {
        return Some(("localhost".to_string(), host_port));
    }
    container_ip.map(|ip| (ip.to_string(), container_port))
}

#[allow(clippy::unused_async)]
async fn restore_forwards(app: &mut App) {
    let Some(host) = app.remote_host.clone() else {
        return;
    };
    let Some(forwards) = app.ssh_forwards.get(&app.active_connection).cloned() else {
        return;
    };
    if forwards.is_empty() {
        return;
    }

    let mut restored = 0u32;
    let mut failed = 0u32;

    for (&container_port, &local_port) in &forwards {
        if forward::is_port_listening(local_port) {
            continue;
        }
        let (remote_target, remote_port) = if app.is_docker_target() {
            match resolve_docker_forward(
                container_port,
                &app.docker_port_mappings,
                app.container_ip.as_deref(),
            ) {
                Some(pair) => pair,
                None => continue,
            }
        } else {
            ("localhost".to_string(), container_port)
        };
        let spec = format!("{local_port}:{remote_target}:{remote_port}");
        match port::ssh::create_forward(&spec, &host, false) {
            Ok(_) => restored += 1,
            Err(_) => failed += 1,
        }
    }

    if restored > 0 && failed > 0 {
        app.set_status(&format!("Restored {restored} forward(s), {failed} failed"));
    } else if restored > 0 {
        app.set_status(&format!("Restored {restored} forward(s)"));
    }
}

fn activate_connection_ui(app: &mut App) {
    app.apply_connection();
    app.entries.clear();
    app.apply_filter();
    app.selected = 0;
    app.loading = true;
    let name = app
        .active_connection()
        .map_or("Unknown", |c| c.name.as_str())
        .to_string();
    app.set_status(&format!("Switched to: {name}"));
}

struct ActivationInput {
    remote_host: Option<String>,
    docker_target: Option<String>,
    is_docker_target: bool,
    ssh_forwards_for_conn: Option<HashMap<u16, u16>>,
    known_forwards: HashMap<u16, u16>,
    active_connection: usize,
}

struct ActivationResult {
    active_connection: usize,
    container_ip: Option<String>,
    docker_port_mappings: HashMap<u16, u16>,
    restore_status: Option<String>,
    entries: anyhow::Result<Vec<PortEntry>>,
}

struct RefreshResult {
    active_connection: usize,
    entries: anyhow::Result<Vec<PortEntry>>,
}

fn extract_activation_input(app: &App) -> ActivationInput {
    ActivationInput {
        remote_host: app.remote_host.clone(),
        docker_target: app.docker_target.clone(),
        is_docker_target: app.is_docker_target(),
        ssh_forwards_for_conn: app.ssh_forwards.get(&app.active_connection).cloned(),
        known_forwards: app.known_forwards().clone(),
        active_connection: app.active_connection,
    }
}

fn restore_forwards_standalone(
    host: &str,
    forwards: &HashMap<u16, u16>,
    is_docker_target: bool,
    container_ip: Option<&str>,
    docker_port_mappings: &HashMap<u16, u16>,
) -> Option<String> {
    if forwards.is_empty() {
        return None;
    }

    let mut restored = 0u32;
    let mut failed = 0u32;

    for (&container_port, &local_port) in forwards {
        if forward::is_port_listening(local_port) {
            continue;
        }
        let (remote_target, remote_port) = if is_docker_target {
            match resolve_docker_forward(container_port, docker_port_mappings, container_ip) {
                Some(pair) => pair,
                None => continue,
            }
        } else {
            ("localhost".to_string(), container_port)
        };
        let spec = format!("{local_port}:{remote_target}:{remote_port}");
        match port::ssh::create_forward(&spec, host, false) {
            Ok(_) => restored += 1,
            Err(_) => failed += 1,
        }
    }

    if restored > 0 && failed > 0 {
        Some(format!("Restored {restored} forward(s), {failed} failed"))
    } else if restored > 0 {
        Some(format!("Restored {restored} forward(s)"))
    } else {
        None
    }
}

async fn run_activation(input: ActivationInput) -> ActivationResult {
    // 1. Resolve container info (IP + port mappings)
    let (container_ip, docker_port_mappings) = if let Some(ref target) = input.docker_target {
        match port::docker::get_container_info(target, input.remote_host.as_deref()).await {
            Ok(info) => (Some(info.ip), info.port_mappings),
            Err(_) => (None, HashMap::new()),
        }
    } else {
        (None, HashMap::new())
    };

    // 2. Restore forwards (sync, fast)
    let restore_status = if let (Some(ref host), Some(ref forwards)) =
        (&input.remote_host, &input.ssh_forwards_for_conn)
    {
        restore_forwards_standalone(
            host,
            forwards,
            input.is_docker_target,
            container_ip.as_deref(),
            &docker_port_mappings,
        )
    } else {
        None
    };

    // 3. Collect all ports (heavy I/O)
    let entries = port::collect_all(
        input.remote_host.as_deref(),
        input.docker_target.as_deref(),
        &input.known_forwards,
    )
    .await;

    ActivationResult {
        active_connection: input.active_connection,
        container_ip,
        docker_port_mappings,
        restore_status,
        entries,
    }
}

fn apply_activation_result(app: &mut App, result: ActivationResult) {
    if app.active_connection != result.active_connection {
        return; // stale result, discard
    }
    app.loading = false;
    app.container_ip = result.container_ip.or(app.container_ip.take());
    if !result.docker_port_mappings.is_empty() {
        app.docker_port_mappings = result.docker_port_mappings;
    }
    if let Some(status) = result.restore_status {
        app.set_status(&status);
    }
    match result.entries {
        Ok(entries) => {
            if app.set_entries(entries) {
                save_forwards(app);
            }
        }
        Err(e) => app.set_status(&format!("Refresh failed: {e}")),
    }
}

fn apply_refresh_result(app: &mut App, result: RefreshResult) {
    if app.active_connection != result.active_connection {
        return;
    }
    app.loading = false;
    match result.entries {
        Ok(entries) => {
            if app.set_entries(entries) {
                save_forwards(app);
            }
        }
        Err(e) => app.set_status(&format!("Refresh failed: {e}")),
    }
}

fn spawn_activation(
    app: &App,
    handle: &mut Option<tokio::task::JoinHandle<()>>,
    refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx: &tokio::sync::mpsc::Sender<ActivationResult>,
) {
    if let Some(h) = handle.take() {
        h.abort();
    }
    if let Some(h) = refresh_handle.take() {
        h.abort();
    }
    let input = extract_activation_input(app);
    let tx = tx.clone();
    *handle = Some(tokio::spawn(async move {
        let result = run_activation(input).await;
        let _ = tx.send(result).await;
    }));
}

fn spawn_refresh(
    app: &App,
    refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    activation_handle: Option<&tokio::task::JoinHandle<()>>,
    tx: &tokio::sync::mpsc::Sender<RefreshResult>,
) {
    // activation 実行中なら refresh は不要 (activation が collect_all を含む)
    if activation_handle.is_some_and(|h| !h.is_finished()) {
        return;
    }
    if let Some(h) = refresh_handle.take() {
        h.abort();
    }
    let remote_host = app.remote_host.clone();
    let docker_target = app.docker_target.clone();
    let known_forwards = app.known_forwards().clone();
    let active_connection = app.active_connection;
    let tx = tx.clone();
    *refresh_handle = Some(tokio::spawn(async move {
        let entries = port::collect_all(
            remote_host.as_deref(),
            docker_target.as_deref(),
            &known_forwards,
        )
        .await;
        let _ = tx
            .send(RefreshResult {
                active_connection,
                entries,
            })
            .await;
    }));
}

fn handle_submit_forward(app: &mut App, mock_mode: bool) -> bool {
    let mut needs_refresh = false;
    if mock_mode {
        if app.forward_input.to_spec().is_some() {
            let local_port: u16 = app.forward_input.local_port.parse().unwrap_or(0);
            let mock_entry = PortEntry {
                source: port::PortSource::Ssh,
                local_port,
                remote_host: Some(app.forward_input.remote_host.clone()),
                remote_port: app.forward_input.remote_port.parse().ok(),
                process_name: "ssh".to_string(),
                pid: Some(99999),
                container_id: None,
                container_name: None,
                ssh_host: Some(app.forward_input.ssh_host.clone()),
                is_open: true,
                is_loopback: false,
                forwarded_port: None,
            };
            let mut entries = app.entries.clone();
            entries.push(mock_entry);
            entries.sort_by_key(|e| (!e.is_open, e.local_port));
            app.set_entries(entries);
            app.set_status("[mock] Forward created");
        } else {
            app.set_status("Invalid forward specification");
        }
    } else if let Some((spec, host)) = app.forward_input.to_spec() {
        let local_port: Option<u16> = app.forward_input.local_port.parse().ok();
        let already_listening = local_port.is_some_and(forward::is_port_listening);

        if already_listening {
            if app.is_remote() {
                if let (Ok(rp), Ok(lp)) = (
                    app.forward_input.remote_port.parse::<u16>(),
                    app.forward_input.local_port.parse::<u16>(),
                ) {
                    app.ssh_forwards
                        .entry(app.active_connection)
                        .or_default()
                        .insert(rp, lp);
                    save_forwards(app);
                }
            }
            app.set_status("Forward already active, registered mapping");
            needs_refresh = true;
        } else {
            match port::ssh::create_forward(&spec, &host, false) {
                Ok(pid) => {
                    if app.is_remote() {
                        if let (Ok(rp), Ok(lp)) = (
                            app.forward_input.remote_port.parse::<u16>(),
                            app.forward_input.local_port.parse::<u16>(),
                        ) {
                            app.ssh_forwards
                                .entry(app.active_connection)
                                .or_default()
                                .insert(rp, lp);
                            save_forwards(app);
                        }
                    }
                    app.set_status(&format!("Forward created (PID: {pid})"));
                    needs_refresh = true;
                }
                Err(e) => {
                    app.set_status(&format!("Forward failed: {e}"));
                }
            }
        }
    } else {
        app.set_status("Invalid forward specification");
    }
    app.popup = Popup::None;
    app.reset_forward_input();
    needs_refresh
}

fn handle_kill_action(
    app: &mut App,
    mock_mode: bool,
    tx: &tokio::sync::mpsc::Sender<RefreshResult>,
) {
    let Some(entry) = app.selected_entry() else {
        return;
    };
    let port = entry.local_port;
    let pid = entry.pid;
    let is_ssh = entry.source == port::PortSource::Ssh;

    if mock_mode {
        let entries: Vec<_> = app
            .entries
            .iter()
            .filter(|e| e.local_port != port)
            .cloned()
            .collect();
        app.set_entries(entries);
        app.set_status(&format!("[mock] Removed port {port}"));
        return;
    }

    // Pre-remove from ssh_forwards (if kill fails, the forward is already broken)
    if is_ssh {
        if let Some(map) = app.ssh_forwards.get_mut(&app.active_connection) {
            map.retain(|_, &mut lp| lp != port);
            save_forwards(app);
        }
    } else if app.is_docker_target() {
        if let Some(map) = app.ssh_forwards.get_mut(&app.active_connection) {
            map.retain(|_, &mut lp| lp != port);
            save_forwards(app);
        }
    }

    let is_docker = app.is_docker_target();
    let remote_host = app.remote_host.clone();
    let docker_target = app.docker_target.clone();
    let known_forwards = app.known_forwards().clone();
    let active_connection = app.active_connection;
    let tx = tx.clone();

    app.set_status(&format!("Killing port {port}..."));

    tokio::spawn(async move {
        let killed = if is_docker {
            if let Some(pid) = pid {
                if let Some(ref target) = docker_target {
                    let pid_str = pid.to_string();
                    let result = match remote_host.as_deref() {
                        Some(host) => {
                            port::ssh_cmd_tokio(host, &["docker", "exec", target, "kill", &pid_str])
                                .status()
                                .await
                        }
                        None => {
                            tokio::process::Command::new("docker")
                                .args(["exec", target, "kill", &pid_str])
                                .status()
                                .await
                        }
                    };
                    matches!(result, Ok(status) if status.success())
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            let kill_host = if is_ssh { None } else { remote_host.as_deref() };
            port::kill_by_port(port, kill_host).await.is_ok()
        };

        if killed {
            let entries = port::collect_all(
                remote_host.as_deref(),
                docker_target.as_deref(),
                &known_forwards,
            )
            .await;
            let _ = tx
                .send(RefreshResult {
                    active_connection,
                    entries,
                })
                .await;
        }
    });
}

fn handle_quick_forward(app: &mut App, mock_mode: bool) -> bool {
    let Some(entry) = app.selected_entry() else {
        return false;
    };
    let port = entry.local_port;

    let Some(host) = app.remote_host.clone() else {
        if app.is_docker_target() {
            app.set_status("Quick Forward for local Docker not yet supported");
        } else {
            app.set_status("Quick Forward requires --remote mode");
        }
        return false;
    };

    let (forward_target, remote_port) = if app.is_docker_target() {
        match resolve_docker_forward(port, &app.docker_port_mappings, app.container_ip.as_deref()) {
            Some(pair) => pair,
            None => {
                app.set_status("Container IP not available");
                return false;
            }
        }
    } else {
        ("localhost".to_string(), port)
    };
    let spec = format!("{port}:{forward_target}:{remote_port}");

    if mock_mode {
        let mock_entry = PortEntry {
            source: port::PortSource::Ssh,
            local_port: port,
            remote_host: Some(forward_target.clone()),
            remote_port: Some(port),
            process_name: "ssh".to_string(),
            pid: Some(99999),
            container_id: None,
            container_name: None,
            ssh_host: Some(host.clone()),
            is_open: true,
            is_loopback: false,
            forwarded_port: None,
        };
        let mut entries = app.entries.clone();
        entries.push(mock_entry);
        entries.sort_by_key(|e| (!e.is_open, e.local_port));
        app.set_entries(entries);
        app.set_status(&format!("[mock] Forward :{port} -> {host}:{port}"));
        false
    } else if forward::is_port_listening(port) {
        app.ssh_forwards
            .entry(app.active_connection)
            .or_default()
            .insert(port, port);
        save_forwards(app);
        app.set_status("Forward already active, registered mapping");
        true
    } else {
        match port::ssh::create_forward(&spec, &host, false) {
            Ok(pid) => {
                app.ssh_forwards
                    .entry(app.active_connection)
                    .or_default()
                    .insert(port, port);
                save_forwards(app);
                app.set_status(&format!("Forward :{port} -> {host}:{port} (PID: {pid})"));
                true
            }
            Err(e) => {
                app.set_status(&format!("Forward failed: {e}"));
                false
            }
        }
    }
}

fn handle_connection_switch(app: &mut App, direction: i32, mock_mode: bool) -> bool {
    if !app.has_multiple_connections() {
        return false;
    }
    if direction > 0 {
        app.next_connection();
    } else {
        app.prev_connection();
    }
    activate_connection_ui(app);
    !mock_mode
}

#[derive(Parser)]
#[command(name = "quay")]
#[command(about = "A TUI port manager for local processes, SSH forwards, and Docker containers")]
#[command(version)]
struct Cli {
    /// Remote host (e.g., user@server) to scan ports via SSH
    #[arg(short, long)]
    remote: Option<String>,

    /// Docker container to scan ports inside (e.g., syntopic-dev)
    #[arg(short = 'd', long)]
    docker: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all ports (non-interactive)
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Show only local ports
        #[arg(long)]
        local: bool,
        /// Show only SSH forwards
        #[arg(long)]
        ssh: bool,
        /// Show only Docker ports
        #[arg(long)]
        docker: bool,
    },
    /// Create an SSH port forward
    Forward {
        /// Port specification (e.g., 8080:localhost:80)
        spec: String,
        /// Remote host
        host: String,
        /// Remote forward (-R instead of -L)
        #[arg(short = 'R', long)]
        remote: bool,
    },
    /// Kill process on a port
    Kill {
        /// Port number
        port: u16,
        /// Kill by PID instead of port
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Developer tools for testing and debugging
    Dev {
        #[command(subcommand)]
        command: dev::DevCommands,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Resolve remote_host and docker_target: CLI flags take precedence over config
    let config = config::Config::load();
    let remote_host = cli.remote.or(config.general.remote_host);
    let docker_target = cli.docker.or(config.general.docker_target);

    match cli.command {
        Some(Commands::List {
            json,
            local,
            ssh,
            docker,
        }) => {
            run_list(
                json,
                local,
                ssh,
                docker,
                remote_host.as_deref(),
                docker_target.as_deref(),
            )
            .await
        }
        Some(Commands::Forward { spec, host, remote }) => run_forward(&spec, &host, remote).await,
        Some(Commands::Kill { port, pid }) => run_kill(port, pid, remote_host.as_deref()).await,
        Some(Commands::Dev { command }) => dev::run_dev(command).await,
        None => run_tui(remote_host, docker_target).await,
    }
}

#[allow(clippy::fn_params_excessive_bools)]
async fn run_list(
    json: bool,
    local: bool,
    ssh: bool,
    docker: bool,
    remote_host: Option<&str>,
    docker_target: Option<&str>,
) -> Result<()> {
    let entries = port::collect_all(remote_host, docker_target, &HashMap::new()).await?;

    let filtered: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            if local {
                e.source == port::PortSource::Local
            } else if ssh {
                e.source == port::PortSource::Ssh
            } else if docker {
                e.source == port::PortSource::Docker
            } else {
                true
            }
        })
        .collect();

    if json {
        let json_entries: Vec<_> = filtered
            .iter()
            .map(|e| {
                serde_json::json!({
                    "source": format!("{:?}", e.source),
                    "local_port": e.local_port,
                    "is_open": e.is_open,
                    "remote_host": e.remote_host,
                    "remote_port": e.remote_port,
                    "process_name": e.process_name,
                    "pid": e.pid,
                    "container_id": e.container_id,
                    "container_name": e.container_name,
                    "ssh_host": e.ssh_host,
                    "is_loopback": e.is_loopback,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_entries)?);
    } else {
        println!(
            "{:<8} {:<6} {:<8} {:<20} PROCESS",
            "TYPE", "OPEN", "LOCAL", "REMOTE"
        );
        println!("{}", "-".repeat(66));
        for entry in filtered {
            let open_indicator = if entry.is_open { "●" } else { "○" };
            let local_display = if let Some(fwd) = entry.forwarded_port {
                format!(":{}→:{}", entry.local_port, fwd)
            } else {
                format!(":{}", entry.local_port)
            };
            println!(
                "{:<8} {:<6} {:<14} {:<20} {}",
                entry.source,
                open_indicator,
                local_display,
                entry.remote_display(),
                entry.process_display()
            );
        }
    }

    Ok(())
}

#[allow(clippy::unused_async)]
async fn run_forward(spec: &str, host: &str, remote: bool) -> Result<()> {
    let flag = if remote { "-R" } else { "-L" };
    println!("Creating SSH forward: ssh -f -N {flag} {spec} {host}");

    match port::ssh::create_forward(spec, host, remote) {
        Ok(pid) => {
            println!("Started with PID: {pid}");
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to create forward: {e}");
            Err(e)
        }
    }
}

async fn run_kill(port: u16, pid: Option<u32>, remote_host: Option<&str>) -> Result<()> {
    if let Some(pid) = pid {
        println!("Killing process with PID: {pid}...");
        port::kill_by_pid(pid, remote_host).await?;
        println!("Done.");
    } else {
        println!("Killing process on port: {port}...");
        port::kill_by_port(port, remote_host).await?;
        println!("Done.");
    }
    Ok(())
}

async fn run_tui(remote_host: Option<String>, docker_target: Option<String>) -> Result<()> {
    run_tui_with_entries(None, remote_host, docker_target).await
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run_tui_with_entries(
    initial: Option<Vec<PortEntry>>,
    remote_host: Option<String>,
    docker_target: Option<String>,
) -> Result<()> {
    let mock_mode = initial.is_some();

    // Load config first (needed for terminal setup)
    let config = config::Config::load();
    let mouse_enabled = config.ui.mouse_enabled;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    if mouse_enabled {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    } else {
        execute!(stdout, EnterAlternateScreen)?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();
    app.remote_host = remote_host;
    app.docker_target = docker_target;

    // Resolve container info (IP + port mappings) for docker target mode
    resolve_container_info(&mut app).await;

    // Apply config settings
    if !mock_mode {
        app.auto_refresh = config.general.auto_refresh;
    }
    app.refresh_ticks = config.general.refresh_interval.saturating_mul(4).max(1);
    match config.general.default_filter.as_str() {
        "local" => app.filter = Filter::Local,
        "ssh" => app.filter = Filter::Ssh,
        "docker" => app.filter = Filter::Docker,
        _ => app.filter = Filter::All,
    }

    // Load presets
    let presets = preset::Presets::load();
    app.presets = presets.preset;

    // Load connections
    let mut stored_connections = connection::Connections::load();
    let all_connections = stored_connections.all_with_local();
    app.connections = all_connections;

    // In mock mode, add sample connections for testing h/l switching
    if mock_mode && app.connections.len() <= 1 {
        app.connections.push(connection::Connection {
            name: "Production".to_string(),
            remote_host: Some("user@prod-server".to_string()),
            docker_target: None,
        });
        app.connections.push(connection::Connection {
            name: "AI Lab".to_string(),
            remote_host: Some("ailab".to_string()),
            docker_target: Some("syntopic-dev".to_string()),
        });
    }

    // CLI args: find matching connection or keep Local with overrides
    if app.remote_host.is_some() || app.docker_target.is_some() {
        let mut found = false;
        for (i, conn) in app.connections.iter().enumerate() {
            if conn.remote_host == app.remote_host && conn.docker_target == app.docker_target {
                app.active_connection = i;
                found = true;
                break;
            }
        }
        if !found {
            // Keep Local (index 0) but CLI values already override remote_host/docker_target
        }
    }

    // Load persisted forward mappings
    if !mock_mode {
        let mut stored_forwards = forward::Forwards::load();
        if stored_forwards.remove_stale() {
            let _ = stored_forwards.save();
        }
        app.ssh_forwards = stored_forwards.to_runtime(&app.connections);
    }

    // Load initial data
    if let Some(entries) = initial {
        app.set_entries(entries);
        app.loading = false;
        app.set_status("[mock] Loaded mock data");
    } else {
        restore_forwards(&mut app).await;
        refresh_and_save(&mut app).await;
        app.loading = false;
    }

    // Main loop
    let (activation_tx, mut activation_rx) = tokio::sync::mpsc::channel::<ActivationResult>(1);
    let mut activation_handle: Option<tokio::task::JoinHandle<()>> = None;
    let (refresh_tx, mut refresh_rx) = tokio::sync::mpsc::channel::<RefreshResult>(1);
    let mut refresh_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut reader = EventStream::new();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(250));
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        let event = tokio::select! {
            event = reader.next() => match event {
                Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                    AppEvent::Key(key)
                }
                Some(Ok(Event::Mouse(mouse))) => AppEvent::Mouse(mouse),
                Some(Ok(_) | Err(_)) => continue,
                None => break,
            },
            result = activation_rx.recv() => {
                if let Some(result) = result {
                    apply_activation_result(&mut app, result);
                }
                continue;
            },
            result = refresh_rx.recv() => {
                if let Some(result) = result {
                    apply_refresh_result(&mut app, result);
                }
                continue;
            },
            _ = tick_interval.tick() => AppEvent::Tick,
        };

        match event {
            AppEvent::Key(key) => {
                // Handle Forward popup specially (needs input handling)
                if app.popup == Popup::Forward {
                    let remote_mode = app.is_remote();
                    let docker_mode = app.is_docker_target();
                    if let Some(action) =
                        handle_forward_key(key, &mut app.forward_input, remote_mode, docker_mode)
                    {
                        match action {
                            Action::ClosePopup => {
                                app.popup = Popup::None;
                                app.reset_forward_input();
                            }
                            Action::SubmitForward => {
                                let needs_refresh = handle_submit_forward(&mut app, mock_mode);
                                if needs_refresh {
                                    spawn_refresh(
                                        &app,
                                        &mut refresh_handle,
                                        activation_handle.as_ref(),
                                        &refresh_tx,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                // Handle Presets popup
                if app.popup == Popup::Presets {
                    if let Some(action) = handle_preset_key(key) {
                        match action {
                            Action::ClosePopup => {
                                app.popup = Popup::None;
                            }
                            Action::Up => app.preset_previous(),
                            Action::Down => app.preset_next(),
                            Action::LaunchPreset => {
                                if mock_mode {
                                    app.set_status("[mock] Forward created");
                                } else if let Some(preset) = app.selected_preset() {
                                    let spec = format!(
                                        "{}:{}:{}",
                                        preset.local_port, preset.remote_host, preset.remote_port
                                    );
                                    let host = preset.ssh_host.clone();
                                    match port::ssh::create_forward(&spec, &host, false) {
                                        Ok(pid) => {
                                            app.set_status(&format!(
                                                "Forward created (PID: {pid})"
                                            ));
                                            spawn_refresh(
                                                &app,
                                                &mut refresh_handle,
                                                activation_handle.as_ref(),
                                                &refresh_tx,
                                            );
                                        }
                                        Err(e) => {
                                            app.set_status(&format!("Forward failed: {e}"));
                                        }
                                    }
                                }
                                app.popup = Popup::None;
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                // Handle Connections popup
                if app.popup == Popup::Connections {
                    if app.connection_popup_mode == ConnectionPopupMode::AddNew {
                        if let Some(action) =
                            handle_connection_input_key(key, &mut app.connection_input)
                        {
                            match action {
                                Action::ClosePopup => {
                                    // Go back to List mode
                                    app.connection_popup_mode = ConnectionPopupMode::List;
                                    app.reset_connection_input();
                                }
                                Action::SubmitConnection => {
                                    if let Some(conn) = app.connection_input.to_connection() {
                                        let name = conn.name.clone();
                                        stored_connections.add(conn);
                                        if let Err(e) = stored_connections.save() {
                                            app.set_status(&format!("Save failed: {e}"));
                                        } else {
                                            app.connections = stored_connections.all_with_local();
                                            app.set_status(&format!("Added connection: {name}"));
                                        }
                                        app.connection_popup_mode = ConnectionPopupMode::List;
                                        app.reset_connection_input();
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(action) = handle_connection_key(key) {
                        match action {
                            Action::ClosePopup => {
                                app.popup = Popup::None;
                            }
                            Action::Up => app.connection_previous(),
                            Action::Down => app.connection_next(),
                            Action::ActivateConnection => {
                                app.active_connection = app.connection_selected;
                                activate_connection_ui(&mut app);
                                if !mock_mode {
                                    spawn_activation(
                                        &app,
                                        &mut activation_handle,
                                        &mut refresh_handle,
                                        &activation_tx,
                                    );
                                }
                                app.popup = Popup::None;
                            }
                            Action::AddConnection => {
                                app.connection_popup_mode = ConnectionPopupMode::AddNew;
                                app.reset_connection_input();
                            }
                            Action::DeleteConnection => {
                                if app.connection_selected == 0 {
                                    app.set_status("Cannot delete Local connection");
                                } else {
                                    let user_index = app.connection_selected - 1;
                                    let name = stored_connections
                                        .connection
                                        .get(user_index)
                                        .map_or("Unknown".to_string(), |c| c.name.clone());
                                    if stored_connections.remove(user_index) {
                                        if let Err(e) = stored_connections.save() {
                                            app.set_status(&format!("Save failed: {e}"));
                                        } else {
                                            app.connections = stored_connections.all_with_local();
                                            // Adjust active_connection if needed
                                            if app.active_connection >= app.connections.len() {
                                                app.active_connection =
                                                    app.connections.len().saturating_sub(1);
                                                app.apply_connection();
                                            } else if app.active_connection
                                                == app.connection_selected
                                            {
                                                // Deleted the active connection, switch to Local
                                                app.active_connection = 0;
                                                app.apply_connection();
                                            }
                                            // Adjust selection cursor
                                            if app.connection_selected >= app.connections.len() {
                                                app.connection_selected =
                                                    app.connections.len().saturating_sub(1);
                                            }
                                            app.set_status(&format!("Deleted connection: {name}"));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                // Handle other popups
                if app.popup != Popup::None {
                    if let Some(Action::ClosePopup) = handle_popup_key(key) {
                        app.popup = Popup::None;
                    }
                    continue;
                }

                let action = match app.input_mode {
                    InputMode::Search => handle_search_key(key, &mut app.search_query),
                    InputMode::Normal => handle_key(key),
                };

                if let Some(action) = action {
                    match action {
                        Action::Quit => {
                            app.should_quit = true;
                        }
                        Action::Up => app.previous(),
                        Action::Down => app.next(),
                        Action::First => app.first(),
                        Action::Last => app.last(),
                        Action::EnterSearch => {
                            app.input_mode = InputMode::Search;
                        }
                        Action::ExitSearch => {
                            app.input_mode = InputMode::Normal;
                        }
                        Action::UpdateSearch => {
                            app.apply_filter();
                        }
                        Action::FilterAll => app.set_filter(Filter::All),
                        Action::FilterLocal => app.set_filter(Filter::Local),
                        Action::FilterSsh => app.set_filter(Filter::Ssh),
                        Action::FilterDocker => app.set_filter(Filter::Docker),
                        Action::Refresh => {
                            if !mock_mode {
                                app.loading = true;
                                spawn_refresh(
                                    &app,
                                    &mut refresh_handle,
                                    activation_handle.as_ref(),
                                    &refresh_tx,
                                );
                                app.set_status("Refreshing...");
                            }
                        }
                        Action::ToggleAutoRefresh => {
                            if !mock_mode {
                                app.auto_refresh = !app.auto_refresh;
                                if app.auto_refresh {
                                    app.set_status("Auto-refresh ON");
                                } else {
                                    app.set_status("Auto-refresh OFF");
                                }
                            }
                        }
                        Action::Kill => {
                            handle_kill_action(&mut app, mock_mode, &refresh_tx);
                        }
                        Action::Select => {
                            app.popup = Popup::Details;
                        }
                        Action::ShowHelp => {
                            app.popup = Popup::Help;
                        }
                        Action::StartForward => {
                            app.forward_input = match (
                                app.selected_entry(),
                                app.remote_host.as_deref(),
                            ) {
                                (Some(entry), Some(host)) if app.is_docker_target() => {
                                    let mut input = ForwardInput::for_remote_entry(entry, host);
                                    if let Some((target, rport)) = resolve_docker_forward(
                                        entry.local_port,
                                        &app.docker_port_mappings,
                                        app.container_ip.as_deref(),
                                    ) {
                                        input.remote_host = target;
                                        input.remote_port = rport.to_string();
                                    }
                                    input
                                }
                                (Some(entry), Some(host)) => {
                                    ForwardInput::for_remote_entry(entry, host)
                                }
                                (Some(entry), None) => ForwardInput::from_entry(entry),
                                _ => ForwardInput::new(),
                            };
                            app.popup = Popup::Forward;
                        }
                        Action::ShowPresets => {
                            app.preset_selected = 0;
                            app.popup = Popup::Presets;
                        }
                        Action::ClosePopup => {
                            app.popup = Popup::None;
                        }
                        Action::QuickForward => {
                            let needs_refresh = handle_quick_forward(&mut app, mock_mode);
                            if needs_refresh {
                                spawn_refresh(
                                    &app,
                                    &mut refresh_handle,
                                    activation_handle.as_ref(),
                                    &refresh_tx,
                                );
                            }
                        }
                        Action::PrevConnection => {
                            if handle_connection_switch(&mut app, -1, mock_mode) {
                                spawn_activation(
                                    &app,
                                    &mut activation_handle,
                                    &mut refresh_handle,
                                    &activation_tx,
                                );
                            }
                        }
                        Action::NextConnection => {
                            if handle_connection_switch(&mut app, 1, mock_mode) {
                                spawn_activation(
                                    &app,
                                    &mut activation_handle,
                                    &mut refresh_handle,
                                    &activation_tx,
                                );
                            }
                        }
                        Action::ShowConnections => {
                            app.connection_selected = app.active_connection;
                            app.connection_popup_mode = ConnectionPopupMode::List;
                            app.popup = Popup::Connections;
                        }
                        Action::ClearSearch => {
                            app.search_query.clear();
                            app.apply_filter();
                        }
                        Action::SubmitForward
                        | Action::LaunchPreset
                        | Action::SelectRow(_)
                        | Action::ActivateConnection
                        | Action::AddConnection
                        | Action::DeleteConnection
                        | Action::SubmitConnection => {
                            // Handled elsewhere (popup handlers or mouse handler)
                        }
                    }
                }
            }
            AppEvent::Mouse(mouse) => {
                // Only handle mouse if enabled and in normal mode without popup
                if mouse_enabled && app.popup == Popup::None && app.input_mode == InputMode::Normal
                {
                    // Calculate table area: header(3) + filter(3) = 6 rows before table
                    let table_top = 6_u16;
                    let term_height = terminal.size()?.height;
                    let table_height = term_height.saturating_sub(8); // minus header, filter, footer

                    if let Some(action) = handle_mouse(mouse, table_top, table_height) {
                        match action {
                            Action::Up => app.previous(),
                            Action::Down => app.next(),
                            Action::SelectRow(row) => {
                                if row < app.filtered_entries.len() {
                                    app.selected = row;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            AppEvent::Tick => {
                app.tick();
                if !mock_mode && app.should_refresh() {
                    spawn_refresh(
                        &app,
                        &mut refresh_handle,
                        activation_handle.as_ref(),
                        &refresh_tx,
                    );
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    if mouse_enabled {
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    } else {
        execute!(io::stdout(), LeaveAlternateScreen)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_default() {
        let cli = Cli::try_parse_from(["quay"]).unwrap();
        assert!(cli.command.is_none());
        assert!(cli.remote.is_none());
        assert!(cli.docker.is_none());
    }

    #[test]
    fn test_cli_parse_remote() {
        let cli = Cli::try_parse_from(["quay", "--remote", "user@server"]).unwrap();
        assert_eq!(cli.remote, Some("user@server".to_string()));
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parse_remote_with_list() {
        let cli = Cli::try_parse_from(["quay", "--remote", "server", "list"]).unwrap();
        assert_eq!(cli.remote, Some("server".to_string()));
        assert!(matches!(cli.command, Some(Commands::List { .. })));
    }

    #[test]
    fn test_cli_parse_list() {
        let cli = Cli::try_parse_from(["quay", "list", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::List { json: true, .. })
        ));
    }

    #[test]
    fn test_cli_parse_forward() {
        let cli =
            Cli::try_parse_from(["quay", "forward", "8080:localhost:80", "remote-host"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Forward { .. })));
    }

    #[test]
    fn test_cli_parse_kill() {
        let cli = Cli::try_parse_from(["quay", "kill", "3000"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Kill {
                port: 3000,
                pid: None
            })
        ));
    }

    #[test]
    fn test_cli_parse_dev_listen() {
        let cli = Cli::try_parse_from(["quay", "dev", "listen", "3000", "8080"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Dev { .. })));
    }

    #[test]
    fn test_cli_parse_dev_listen_http() {
        let cli = Cli::try_parse_from(["quay", "dev", "listen", "3000", "--http"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Dev { .. })));
    }

    #[test]
    fn test_cli_parse_dev_scenario() {
        let cli = Cli::try_parse_from(["quay", "dev", "scenario", "web"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Dev { .. })));
    }

    #[test]
    fn test_cli_parse_dev_scenario_list() {
        let cli = Cli::try_parse_from(["quay", "dev", "scenario", "--list"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Dev { .. })));
    }

    #[test]
    fn test_cli_parse_dev_check() {
        let cli = Cli::try_parse_from(["quay", "dev", "check", "3000"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Dev { .. })));
    }

    #[test]
    fn test_cli_parse_dev_mock() {
        let cli = Cli::try_parse_from(["quay", "dev", "mock"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Dev { .. })));
    }

    #[test]
    fn test_cli_parse_docker() {
        let cli = Cli::try_parse_from(["quay", "--docker", "my-container"]).unwrap();
        assert_eq!(cli.docker, Some("my-container".to_string()));
        assert!(cli.remote.is_none());
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parse_remote_docker() {
        let cli =
            Cli::try_parse_from(["quay", "--remote", "ailab", "--docker", "syntopic-dev"]).unwrap();
        assert_eq!(cli.remote, Some("ailab".to_string()));
        assert_eq!(cli.docker, Some("syntopic-dev".to_string()));
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parse_docker_short_flag() {
        let cli =
            Cli::try_parse_from(["quay", "-r", "ailab", "-d", "syntopic-dev", "list"]).unwrap();
        assert_eq!(cli.remote, Some("ailab".to_string()));
        assert_eq!(cli.docker, Some("syntopic-dev".to_string()));
        assert!(matches!(cli.command, Some(Commands::List { .. })));
    }
}
