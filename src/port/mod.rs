pub mod docker;
pub mod local;
pub mod ssh;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Duration;
use tokio::net::TcpStream;

const PROBE_TIMEOUT: Duration = Duration::from_millis(200);

fn escape_ssh_args(args: &[&str]) -> String {
    let escaped: Vec<String> = args
        .iter()
        .map(|a| shell_escape::escape(Cow::Borrowed(a)).to_string())
        .collect();
    escaped.join(" ")
}

/// Build a `tokio::process::Command` for SSH that safely escapes each argument.
pub fn ssh_cmd_tokio(host: &str, args: &[&str]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.arg(host).arg(escape_ssh_args(args));
    cmd
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PortSource {
    Local,
    Ssh,
    Docker,
}

impl fmt::Display for PortSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortSource::Local => write!(f, "LOCAL"),
            PortSource::Ssh => write!(f, "SSH"),
            PortSource::Docker => write!(f, "DOCKER"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortEntry {
    pub source: PortSource,
    pub local_port: u16,
    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
    pub process_name: String,
    pub pid: Option<u32>,
    pub container_id: Option<String>,
    pub container_name: Option<String>,
    pub ssh_host: Option<String>,
    pub is_open: bool,
    pub is_loopback: bool,
    pub forwarded_port: Option<u16>,
}

impl PortEntry {
    pub fn remote_display(&self) -> String {
        match (&self.remote_host, self.remote_port) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.clone(),
            _ => String::new(),
        }
    }

    pub fn process_display(&self) -> String {
        match self.source {
            PortSource::Docker => {
                let name = self.container_name.as_deref().unwrap_or("unknown");
                let id = self
                    .container_id
                    .as_deref()
                    .map_or("", |s| &s[..8.min(s.len())]);
                format!("{name} ({id})")
            }
            _ => {
                if let Some(pid) = self.pid {
                    format!("{} (pid:{})", self.process_name, pid)
                } else {
                    self.process_name.clone()
                }
            }
        }
    }
}

async fn collect_entries(remote_host: Option<&str>) -> anyhow::Result<Vec<PortEntry>> {
    let mut entries = Vec::new();

    if let Ok(local) = local::collect(remote_host).await {
        entries.extend(local);
    }

    if let Ok(docker) = docker::collect(remote_host).await {
        entries.extend(docker);
    }

    // SSH tunnels are always local processes
    if let Ok(ssh) = ssh::collect().await {
        entries.extend(ssh);
    }

    dedup_entries(&mut entries);

    Ok(entries)
}

/// Remove LOCAL entries whose port overlaps with SSH or Docker entries.
/// SSH/Docker processes listen locally (visible via lsof), so the LOCAL
/// duplicate is redundant and would cause double-counting in the TUI.
pub fn dedup_entries(entries: &mut Vec<PortEntry>) {
    let non_local_ports: HashSet<u16> = entries
        .iter()
        .filter(|e| e.source != PortSource::Local)
        .map(|e| e.local_port)
        .collect();
    entries.retain(|e| e.source != PortSource::Local || !non_local_ports.contains(&e.local_port));
}

async fn probe_open_ports(entries: &mut [PortEntry], remote_mode: bool) {
    // In remote mode, only probe SSH tunnel entries (which are local).
    // Remote Local/Docker entries already have is_open set from lsof/docker output.
    let probe_ports: Vec<u16> = {
        let mut seen = HashSet::new();
        for e in entries.iter() {
            if remote_mode && e.source != PortSource::Ssh {
                continue;
            }
            seen.insert(e.local_port);
        }
        seen.into_iter().collect()
    };

    let mut handles = Vec::new();
    for port in probe_ports {
        handles.push(tokio::spawn(async move {
            let addr = format!("127.0.0.1:{port}");
            let result =
                tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(&addr)).await;
            (port, result.is_ok() && result.unwrap().is_ok())
        }));
    }

    let mut results = HashMap::new();
    for handle in handles {
        if let Ok((port, is_open)) = handle.await {
            results.insert(port, is_open);
        }
    }

    for entry in entries.iter_mut() {
        if let Some(&open) = results.get(&entry.local_port) {
            entry.is_open = open;
        }
    }
}


pub async fn collect_all(
    remote_host: Option<&str>,
    docker_target: Option<&str>,
    known_forwards: &HashMap<u16, u16>,
) -> anyhow::Result<Vec<PortEntry>> {
    let mut entries = if let Some(container) = docker_target {
        // Docker target mode: only collect from inside the specified container
        let mut e = docker::collect_from_container(container, remote_host).await?;
        for entry in &mut e {
            entry.is_open = false;
        }
        if let Some(host) = remote_host {
            // Remote: SSH tunnel detection only (probe would false-positive)
            if let Ok(ssh_entries) = ssh::collect().await {
                let ssh_port_map: HashMap<u16, u16> = ssh_entries
                    .iter()
                    .filter_map(|se| se.remote_port.map(|rp| (rp, se.local_port)))
                    .collect();
                for entry in &mut e {
                    if let Some(&tunnel_local) = ssh_port_map.get(&entry.local_port) {
                        entry.is_open = true;
                        entry.forwarded_port = Some(tunnel_local);
                    }
                }
            }

            // Fallback: detect ControlMaster-managed tunnels via lsof + probe
            let mut already_forwarded: HashSet<u16> = e
                .iter()
                .filter(|entry| entry.forwarded_port.is_some())
                .filter_map(|entry| entry.forwarded_port)
                .collect();
            // Skip probing for already-known (persisted) forwards
            for &local_port in known_forwards.values() {
                already_forwarded.insert(local_port);
            }
            let container_ports: HashSet<u16> = e.iter().map(|entry| entry.local_port).collect();
            let ssh_ports: Vec<u16> = ssh::get_ssh_master_listening_ports(host).await
                .into_iter()
                .filter(|p| !already_forwarded.contains(p))
                .collect();
            if !ssh_ports.is_empty() {
                if let Ok(mappings) =
                    docker::detect_forward_mappings(container, host, &ssh_ports, &container_ports)
                        .await
                {
                    for entry in &mut e {
                        if !entry.is_open {
                            if let Some(&local_port) = mappings.get(&entry.local_port) {
                                entry.is_open = true;
                                entry.forwarded_port = Some(local_port);
                            }
                        }
                    }
                }
            }

            // Apply known forwards as fallback for entries not detected above
            for entry in &mut e {
                if !entry.is_open {
                    if let Some(&local_port) = known_forwards.get(&entry.local_port) {
                        entry.is_open = true;
                        entry.forwarded_port = Some(local_port);
                    }
                }
            }
        } else {
            // Local: probe localhost (Docker port mappings)
            probe_open_ports(&mut e, false).await;
        }
        e
    } else {
        let mut e = collect_entries(remote_host).await?;
        probe_open_ports(&mut e, remote_host.is_some()).await;
        e
    };
    entries.sort_by_key(|e| (!e.is_open, e.local_port));
    Ok(entries)
}

pub async fn kill_by_pid(pid: u32, remote_host: Option<&str>) -> anyhow::Result<()> {
    let pid_str = pid.to_string();
    let status = match remote_host {
        Some(host) => ssh_cmd_tokio(host, &["kill", &pid_str]).status().await?,
        None => tokio::process::Command::new("kill")
            .arg(&pid_str)
            .status()
            .await?,
    };
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Failed to kill process {pid}")
    }
}

pub async fn kill_by_port(port: u16, remote_host: Option<&str>) -> anyhow::Result<()> {
    let entries = collect_entries(remote_host).await?;
    let entry = entries
        .iter()
        .find(|e| e.local_port == port)
        .ok_or_else(|| anyhow::anyhow!("No process found on port {port}"))?;

    match entry.source {
        PortSource::Ssh => {
            // SSH tunnel processes are always local
            if let Some(pid) = entry.pid {
                kill_by_pid(pid, None).await
            } else {
                anyhow::bail!("No PID found for port {port}")
            }
        }
        PortSource::Local => {
            if let Some(pid) = entry.pid {
                kill_by_pid(pid, remote_host).await
            } else {
                anyhow::bail!("No PID found for port {port}")
            }
        }
        PortSource::Docker => {
            if let Some(ref container_id) = entry.container_id {
                let status = match remote_host {
                    Some(host) => {
                        ssh_cmd_tokio(host, &["docker", "stop", container_id])
                            .status()
                            .await?
                    }
                    None => tokio::process::Command::new("docker")
                        .args(["stop", container_id])
                        .status()
                        .await?,
                };
                if status.success() {
                    Ok(())
                } else {
                    anyhow::bail!("Failed to stop container {container_id}")
                }
            } else {
                anyhow::bail!("No container ID found for port {port}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(source: PortSource, local_port: u16) -> PortEntry {
        PortEntry {
            source,
            local_port,
            remote_host: None,
            remote_port: None,
            process_name: String::new(),
            pid: None,
            container_id: None,
            container_name: None,
            ssh_host: None,
            is_open: false,
            is_loopback: false,
            forwarded_port: None,
        }
    }

    #[test]
    fn test_dedup_ssh_overrides_local() {
        let mut entries = vec![
            make_entry(PortSource::Local, 9000),
            make_entry(PortSource::Ssh, 9000),
        ];

        dedup_entries(&mut entries);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, PortSource::Ssh);
        assert_eq!(entries[0].local_port, 9000);
    }

    #[test]
    fn test_dedup_docker_overrides_local() {
        let mut entries = vec![
            make_entry(PortSource::Local, 8080),
            make_entry(PortSource::Docker, 8080),
        ];

        dedup_entries(&mut entries);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, PortSource::Docker);
        assert_eq!(entries[0].local_port, 8080);
    }

    #[test]
    fn test_dedup_no_overlap() {
        let mut entries = vec![
            make_entry(PortSource::Local, 3000),
            make_entry(PortSource::Ssh, 9000),
            make_entry(PortSource::Docker, 8080),
        ];

        dedup_entries(&mut entries);

        assert_eq!(entries.len(), 3);
    }

    /// Simulates the SSH tunnel merge logic used in Docker Target remote mode:
    /// In remote mode, probe is skipped (it would false-positive on SSH tunnel
    /// local_ports), so accessibility is determined solely by SSH tunnel
    /// remote_port matching the container's listening port.
    /// e.g. `ssh -L 3000:container_ip:8080` → remote_port=8080 matches Docker port 8080.
    #[test]
    fn test_ssh_tunnel_merge_marks_matching_ports_open() {
        // Docker entries from remote container (all start with is_open=false, no probe)
        let mut docker_entries = vec![
            make_entry(PortSource::Docker, 8080), // SSH tunnel targets this port
            make_entry(PortSource::Docker, 3000), // SSH tunnel local_port=3000, but remote_port=8080
            make_entry(PortSource::Docker, 5432), // no SSH tunnel
        ];

        // SSH tunnel: local_port=3000, remote_port=8080 (forwards to container port 8080)
        // Without the fix, probing 127.0.0.1:3000 would succeed (tunnel listens there)
        // and Docker port 3000 would be marked open — a false positive.
        let ssh_entries = vec![
            {
                let mut e = make_entry(PortSource::Ssh, 3000);
                e.remote_port = Some(8080);
                e
            },
        ];

        // Apply the same merge logic as collect_all() remote mode: match on remote_port only
        let ssh_port_map: HashMap<u16, u16> = ssh_entries
            .iter()
            .filter_map(|se| se.remote_port.map(|rp| (rp, se.local_port)))
            .collect();
        for entry in &mut docker_entries {
            if let Some(&tunnel_local) = ssh_port_map.get(&entry.local_port) {
                entry.is_open = true;
                entry.forwarded_port = Some(tunnel_local);
            }
        }

        // Port 8080: SSH tunnel remote_port=8080, local_port=3000 → open, forwarded to :3000
        assert!(docker_entries[0].is_open);
        assert_eq!(docker_entries[0].forwarded_port, Some(3000));
        // Port 3000: no SSH tunnel with remote_port=3000 → stays closed
        // (Without the fix, probe would false-positive here because tunnel listens on 3000)
        assert!(!docker_entries[1].is_open);
        assert_eq!(docker_entries[1].forwarded_port, None);
        // Port 5432: no SSH tunnel with remote_port=5432 → stays closed
        assert!(!docker_entries[2].is_open);
        assert_eq!(docker_entries[2].forwarded_port, None);
    }
}
