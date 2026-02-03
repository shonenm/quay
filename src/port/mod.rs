pub mod docker;
pub mod local;
pub mod ssh;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Duration;
use tokio::net::TcpStream;

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
                tokio::time::timeout(Duration::from_millis(200), TcpStream::connect(&addr)).await;
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
) -> anyhow::Result<Vec<PortEntry>> {
    let mut entries = if let Some(container) = docker_target {
        // Docker target mode: only collect from inside the specified container
        docker::collect_from_container(container, remote_host).await?
    } else {
        let mut e = collect_entries(remote_host).await?;
        probe_open_ports(&mut e, remote_host.is_some()).await;
        e
    };
    entries.sort_by_key(|e| (!e.is_open, e.local_port));
    Ok(entries)
}

pub fn kill_by_pid(pid: u32, remote_host: Option<&str>) -> anyhow::Result<()> {
    use std::process::Command;
    let status = match remote_host {
        Some(host) => Command::new("ssh")
            .arg(host)
            .arg(format!("kill {pid}"))
            .status()?,
        None => Command::new("kill").arg(pid.to_string()).status()?,
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
                kill_by_pid(pid, None)
            } else {
                anyhow::bail!("No PID found for port {port}")
            }
        }
        PortSource::Local => {
            if let Some(pid) = entry.pid {
                kill_by_pid(pid, remote_host)
            } else {
                anyhow::bail!("No PID found for port {port}")
            }
        }
        PortSource::Docker => {
            if let Some(ref container_id) = entry.container_id {
                use std::process::Command;
                let status = match remote_host {
                    Some(host) => Command::new("ssh")
                        .arg(host)
                        .arg(format!("docker stop {container_id}"))
                        .status()?,
                    None => Command::new("docker")
                        .args(["stop", container_id])
                        .status()?,
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
}
