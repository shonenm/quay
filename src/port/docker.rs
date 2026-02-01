use super::{PortEntry, PortSource};
use anyhow::Result;
use regex::Regex;
use std::process::Command;

pub async fn collect(remote_host: Option<&str>) -> Result<Vec<PortEntry>> {
    let output = match remote_host {
        Some(host) => {
            match Command::new("ssh")
                .arg(host)
                .arg(r#"docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'"#)
                .output()
            {
                Ok(o) => o,
                Err(_) => return Ok(Vec::new()),
            }
        }
        None => {
            match Command::new("docker")
                .args(["ps", "--format", "{{.ID}}\t{{.Names}}\t{{.Ports}}"])
                .output()
            {
                Ok(o) => o,
                Err(_) => return Ok(Vec::new()), // Docker not installed
            }
        }
    };

    if !output.status.success() {
        return Ok(Vec::new()); // Docker daemon not running
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_docker_ps(&stdout, remote_host.is_some())
}

fn parse_docker_ps(output: &str, remote_mode: bool) -> Result<Vec<PortEntry>> {
    let mut entries = Vec::new();
    // Match: 0.0.0.0:5432->5432/tcp or :::5432->5432/tcp (IPv6)
    let port_re = Regex::new(r"(?:[\d.:]+:)?(\d+)->(\d+)/tcp")?;

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }

        let container_id = parts[0].to_string();
        let container_name = parts[1].to_string();
        let ports_str = parts[2];

        for cap in port_re.captures_iter(ports_str) {
            let local_port = cap[1].parse::<u16>().unwrap_or(0);
            let remote_port = cap[2].parse::<u16>().ok();

            if local_port > 0 {
                entries.push(PortEntry {
                    source: PortSource::Docker,
                    local_port,
                    remote_host: Some(container_name.clone()),
                    remote_port,
                    process_name: container_name.clone(),
                    pid: None,
                    container_id: Some(container_id.clone()),
                    container_name: Some(container_name.clone()),
                    ssh_host: None,
                    is_open: remote_mode,
                });
            }
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_docker_ps() {
        let output = "abc123def456\tpostgres\t0.0.0.0:5432->5432/tcp\n\
                      def456abc123\tredis\t0.0.0.0:6379->6379/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 5432);
        assert_eq!(entries[0].container_name, Some("postgres".to_string()));
        assert_eq!(entries[1].local_port, 6379);
    }

    #[test]
    fn test_parse_docker_ps_multiple_ports() {
        let output = "abc123\tweb\t0.0.0.0:80->80/tcp, 0.0.0.0:443->443/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 80);
        assert_eq!(entries[1].local_port, 443);
    }

    #[test]
    fn test_parse_docker_ps_ipv6() {
        let output = "abc123\tnginx\t:::8080->80/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 8080);
        assert_eq!(entries[0].remote_port, Some(80));
    }

    #[test]
    fn test_parse_docker_ps_empty() {
        let entries = parse_docker_ps("", false).unwrap();
        assert!(entries.is_empty());
    }
}
