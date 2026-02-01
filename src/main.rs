mod app;
mod config;
mod dev;
mod event;
mod port;
mod preset;
mod ui;

use anyhow::Result;
use app::{App, Filter, ForwardInput, InputMode, Popup};
use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use event::{
    Action, AppEvent, EventHandler, handle_forward_key, handle_key, handle_mouse, handle_popup_key,
    handle_preset_key, handle_search_key,
};
use port::PortEntry;
use ratatui::prelude::*;
use std::io::{self, stdout};
use std::time::Duration;

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
    let entries = port::collect_all(remote_host, docker_target).await?;

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
            println!(
                "{:<8} {:<6} :{:<7} {:<20} {}",
                entry.source,
                open_indicator,
                entry.local_port,
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
        port::kill_by_pid(pid, remote_host)?;
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

    // Resolve container IP for docker target mode
    if let Some(ref target) = app.docker_target {
        match port::docker::get_container_ip(target, app.remote_host.as_deref()) {
            Ok(ip) => app.container_ip = Some(ip),
            Err(e) => app.set_status(&format!("Container IP lookup failed: {e}")),
        }
    }

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

    // Load initial data
    if let Some(entries) = initial {
        app.set_entries(entries);
        app.set_status("[mock] Loaded mock data");
    } else {
        match port::collect_all(app.remote_host.as_deref(), app.docker_target.as_deref()).await {
            Ok(entries) => app.set_entries(entries),
            Err(e) => app.set_status(&format!("Load failed: {e}")),
        }
    }

    // Event handler
    let event_handler = EventHandler::new(Duration::from_millis(250));

    // Main loop
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        match event_handler.next()? {
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
                                if mock_mode {
                                    // In mock mode, add a fake SSH forward entry
                                    if let Some((_, _)) = app.forward_input.to_spec() {
                                        let local_port: u16 =
                                            app.forward_input.local_port.parse().unwrap_or(0);
                                        let mock_entry = PortEntry {
                                            source: port::PortSource::Ssh,
                                            local_port,
                                            remote_host: Some(
                                                app.forward_input.remote_host.clone(),
                                            ),
                                            remote_port: app.forward_input.remote_port.parse().ok(),
                                            process_name: "ssh".to_string(),
                                            pid: Some(99999),
                                            container_id: None,
                                            container_name: None,
                                            ssh_host: Some(app.forward_input.ssh_host.clone()),
                                            is_open: true,
                                            is_loopback: false,
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
                                    match port::ssh::create_forward(&spec, &host, false) {
                                        Ok(pid) => {
                                            app.set_status(&format!(
                                                "Forward created (PID: {pid})"
                                            ));
                                            if let Ok(entries) = port::collect_all(
                                                app.remote_host.as_deref(),
                                                app.docker_target.as_deref(),
                                            )
                                            .await
                                            {
                                                app.set_entries(entries);
                                            }
                                        }
                                        Err(e) => {
                                            app.set_status(&format!("Forward failed: {e}"));
                                        }
                                    }
                                } else {
                                    app.set_status("Invalid forward specification");
                                }
                                app.popup = Popup::None;
                                app.reset_forward_input();
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
                                            if let Ok(entries) = port::collect_all(
                                                app.remote_host.as_deref(),
                                                app.docker_target.as_deref(),
                                            )
                                            .await
                                            {
                                                app.set_entries(entries);
                                            }
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
                            if mock_mode {
                                // no-op in mock mode
                            } else {
                                match port::collect_all(
                                    app.remote_host.as_deref(),
                                    app.docker_target.as_deref(),
                                )
                                .await
                                {
                                    Ok(entries) => {
                                        app.set_entries(entries);
                                        app.set_status("Refreshed");
                                    }
                                    Err(e) => app.set_status(&format!("Refresh failed: {e}")),
                                }
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
                            if let Some(entry) = app.selected_entry() {
                                let port = entry.local_port;
                                let pid = entry.pid;
                                let is_ssh = entry.source == port::PortSource::Ssh;
                                if mock_mode {
                                    // Remove entry from list in mock mode
                                    let entries: Vec<_> = app
                                        .entries
                                        .iter()
                                        .filter(|e| e.local_port != port)
                                        .cloned()
                                        .collect();
                                    app.set_entries(entries);
                                    app.set_status(&format!("[mock] Removed port {port}"));
                                } else if app.is_docker_target() {
                                    // Docker target mode: kill process inside container
                                    if let Some(pid) = pid {
                                        if let Some(ref target) = app.docker_target {
                                            let kill_cmd =
                                                format!("docker exec {target} kill {pid}");
                                            let result = match app.remote_host.as_deref() {
                                                Some(host) => std::process::Command::new("ssh")
                                                    .arg(host)
                                                    .arg(&kill_cmd)
                                                    .status(),
                                                None => std::process::Command::new("docker")
                                                    .args([
                                                        "exec",
                                                        target,
                                                        "kill",
                                                        &pid.to_string(),
                                                    ])
                                                    .status(),
                                            };
                                            match result {
                                                Ok(status) if status.success() => {
                                                    app.set_status(&format!(
                                                        "Killed PID {pid} in container"
                                                    ));
                                                    if let Ok(entries) = port::collect_all(
                                                        app.remote_host.as_deref(),
                                                        app.docker_target.as_deref(),
                                                    )
                                                    .await
                                                    {
                                                        app.set_entries(entries);
                                                    }
                                                }
                                                Ok(_) => app.set_status(&format!(
                                                    "Kill failed for PID {pid} in container"
                                                )),
                                                Err(e) => {
                                                    app.set_status(&format!("Kill failed: {e}"));
                                                }
                                            }
                                        }
                                    } else {
                                        app.set_status("No PID available for this port (container ss doesn't report PIDs)");
                                    }
                                } else {
                                    // SSH tunnels are always killed locally
                                    let kill_host = if is_ssh {
                                        None
                                    } else {
                                        app.remote_host.as_deref()
                                    };
                                    match port::kill_by_port(port, kill_host).await {
                                        Ok(()) => {
                                            app.set_status(&format!(
                                                "Killed process on port {port}"
                                            ));
                                            if let Ok(entries) = port::collect_all(
                                                app.remote_host.as_deref(),
                                                app.docker_target.as_deref(),
                                            )
                                            .await
                                            {
                                                app.set_entries(entries);
                                            }
                                        }
                                        Err(e) => {
                                            app.set_status(&format!("Kill failed: {e}"));
                                        }
                                    }
                                }
                            }
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
                                app.container_ip.as_deref(),
                            ) {
                                // Docker target + remote: pre-fill container_ip as remote_host, lock ssh_host to remote_host
                                (Some(entry), Some(host), Some(ip)) => {
                                    let mut input = ForwardInput::for_remote_entry(entry, host);
                                    input.remote_host = ip.to_string();
                                    input
                                }
                                (Some(entry), Some(host), None) => {
                                    ForwardInput::for_remote_entry(entry, host)
                                }
                                (Some(entry), None, _) => ForwardInput::from_entry(entry),
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
                            if let Some(entry) = app.selected_entry() {
                                let port = entry.local_port;
                                if let Some(host) = app.remote_host.clone() {
                                    // In docker target mode, forward to container_ip
                                    // In regular remote mode, forward to localhost on remote
                                    let forward_target = if app.is_docker_target() {
                                        if let Some(ip) = app.container_ip.as_deref() {
                                            ip.to_string()
                                        } else {
                                            app.set_status("Container IP not available");
                                            continue;
                                        }
                                    } else {
                                        "localhost".to_string()
                                    };
                                    let spec = format!("{port}:{forward_target}:{port}");
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
                                        };
                                        let mut entries = app.entries.clone();
                                        entries.push(mock_entry);
                                        entries.sort_by_key(|e| (!e.is_open, e.local_port));
                                        app.set_entries(entries);
                                        app.set_status(&format!(
                                            "[mock] Forward :{port} -> {host}:{port}"
                                        ));
                                    } else {
                                        match port::ssh::create_forward(&spec, &host, false) {
                                            Ok(pid) => {
                                                app.set_status(&format!(
                                                    "Forward :{port} -> {host}:{port} (PID: {pid})"
                                                ));
                                                if let Ok(entries) = port::collect_all(
                                                    app.remote_host.as_deref(),
                                                    app.docker_target.as_deref(),
                                                )
                                                .await
                                                {
                                                    app.set_entries(entries);
                                                }
                                            }
                                            Err(e) => {
                                                app.set_status(&format!("Forward failed: {e}"));
                                            }
                                        }
                                    }
                                } else if app.is_docker_target() {
                                    app.set_status(
                                        "Quick Forward for local Docker not yet supported",
                                    );
                                } else {
                                    app.set_status("Quick Forward requires --remote mode");
                                }
                            }
                        }
                        Action::SubmitForward | Action::LaunchPreset | Action::SelectRow(_) => {
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
                    match port::collect_all(
                        app.remote_host.as_deref(),
                        app.docker_target.as_deref(),
                    )
                    .await
                    {
                        Ok(entries) => app.set_entries(entries),
                        Err(e) => app.set_status(&format!("Auto-refresh failed: {e}")),
                    }
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
