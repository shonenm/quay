pub mod docker;
pub mod local;
pub mod ssh;

use std::collections::HashMap;
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
    pub is_open: bool,
}

impl PortEntry {
    pub fn remote_display(&self) -> String {
        match (&self.remote_host, self.remote_port) {
            (Some(host), Some(port)) => format!("{}:{}", host, port),
            (Some(host), None) => host.clone(),
            _ => String::new(),
        }
    }

    pub fn process_display(&self) -> String {
        match self.source {
            PortSource::Docker => {
                let name = self.container_name.as_deref().unwrap_or("unknown");
                let id = self.container_id.as_deref().map(|s| &s[..8.min(s.len())]).unwrap_or("");
                format!("{} ({})", name, id)
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

async fn collect_entries() -> anyhow::Result<Vec<PortEntry>> {
    let mut entries = Vec::new();

    if let Ok(local) = local::collect().await {
        entries.extend(local);
    }

    if let Ok(docker) = docker::collect().await {
        entries.extend(docker);
    }

    if let Ok(ssh) = ssh::collect().await {
        entries.extend(ssh);
    }

    Ok(entries)
}

async fn probe_open_ports(entries: &mut [PortEntry]) {
    let unique_ports: Vec<u16> = {
        let mut seen = HashMap::new();
        for e in entries.iter() {
            seen.entry(e.local_port).or_insert(());
        }
        seen.into_keys().collect()
    };

    let mut handles = Vec::new();
    for port in unique_ports {
        handles.push(tokio::spawn(async move {
            let addr = format!("127.0.0.1:{}", port);
            let result = tokio::time::timeout(
                Duration::from_millis(200),
                TcpStream::connect(&addr),
            )
            .await;
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

pub async fn collect_all() -> anyhow::Result<Vec<PortEntry>> {
    let mut entries = collect_entries().await?;
    probe_open_ports(&mut entries).await;
    entries.sort_by_key(|e| (!e.is_open, e.local_port));
    Ok(entries)
}

pub fn kill_by_pid(pid: u32) -> anyhow::Result<()> {
    use std::process::Command;
    let status = Command::new("kill").arg(pid.to_string()).status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Failed to kill process {}", pid)
    }
}

pub async fn kill_by_port(port: u16) -> anyhow::Result<()> {
    let entries = collect_entries().await?;
    let entry = entries
        .iter()
        .find(|e| e.local_port == port)
        .ok_or_else(|| anyhow::anyhow!("No process found on port {}", port))?;

    match entry.source {
        PortSource::Local | PortSource::Ssh => {
            if let Some(pid) = entry.pid {
                kill_by_pid(pid)
            } else {
                anyhow::bail!("No PID found for port {}", port)
            }
        }
        PortSource::Docker => {
            if let Some(ref container_id) = entry.container_id {
                use std::process::Command;
                let status = Command::new("docker")
                    .args(["stop", container_id])
                    .status()?;
                if status.success() {
                    Ok(())
                } else {
                    anyhow::bail!("Failed to stop container {}", container_id)
                }
            } else {
                anyhow::bail!("No container ID found for port {}", port)
            }
        }
    }
}
