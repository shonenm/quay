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
    // Match single port: 0.0.0.0:5432->5432/tcp or :::5432->5432/tcp
    // Match port range:  0.0.0.0:3000-3001->3000-3001/tcp or :::3000-3001->3000-3001/tcp
    let port_re =
        Regex::new(r"(?:[\d.:]+:)?(\d+)(?:-(\d+))?->(\d+)(?:-(\d+))?/tcp")?;

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
        let mut seen_ports = std::collections::HashSet::new();

        for cap in port_re.captures_iter(ports_str) {
            let local_start = cap[1].parse::<u16>().unwrap_or(0);
            let local_end = cap.get(2).and_then(|m| m.as_str().parse::<u16>().ok());
            let remote_start = cap[3].parse::<u16>().unwrap_or(0);
            let remote_end = cap.get(4).and_then(|m| m.as_str().parse::<u16>().ok());

            match (local_end, remote_end) {
                // Range: 3000-3001->3000-3001/tcp → expand to individual ports
                (Some(le), Some(re)) if le >= local_start && re >= remote_start => {
                    let count = (le - local_start + 1).min(re - remote_start + 1);
                    for i in 0..count {
                        let lp = local_start + i;
                        let rp = remote_start + i;
                        if lp > 0 && seen_ports.insert(lp) {
                            entries.push(PortEntry {
                                source: PortSource::Docker,
                                local_port: lp,
                                remote_host: Some(container_name.clone()),
                                remote_port: Some(rp),
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
                // Single port: 5432->5432/tcp
                _ => {
                    if local_start > 0 && seen_ports.insert(local_start) {
                        entries.push(PortEntry {
                            source: PortSource::Docker,
                            local_port: local_start,
                            remote_host: Some(container_name.clone()),
                            remote_port: Some(remote_start),
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
    fn test_parse_docker_ps_port_range() {
        let output =
            "abc123\tsyntopic-dev\t0.0.0.0:3000-3001->3000-3001/tcp, :::3000-3001->3000-3001/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 3000);
        assert_eq!(entries[0].remote_port, Some(3000));
        assert_eq!(entries[1].local_port, 3001);
        assert_eq!(entries[1].remote_port, Some(3001));
    }

    #[test]
    fn test_parse_docker_ps_mixed_range_and_single() {
        let output = "abc123\tapp\t0.0.0.0:5173-5174->5173-5174/tcp, 0.0.0.0:5432->5432/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].local_port, 5173);
        assert_eq!(entries[1].local_port, 5174);
        assert_eq!(entries[2].local_port, 5432);
    }

    #[test]
    fn test_parse_docker_ps_ipv4_ipv6_dedup() {
        let output =
            "abc123\tpostgres\t0.0.0.0:5432->5432/tcp, :::5432->5432/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 5432);
    }

    #[test]
    fn test_parse_docker_ps_empty() {
        let entries = parse_docker_ps("", false).unwrap();
        assert!(entries.is_empty());
    }
}
