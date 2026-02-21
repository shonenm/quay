use super::{PortEntry, PortSource};
use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::Duration;

#[allow(clippy::unused_async)]
pub async fn collect(remote_host: Option<&str>) -> Result<Vec<PortEntry>> {
    let output = match remote_host {
        Some(host) => {
            match Command::new("ssh")
                .arg(host)
                .arg(r"docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'")
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
    let port_re = Regex::new(r"(?:[\d.:]+:)?(\d+)(?:-(\d+))?->(\d+)(?:-(\d+))?/tcp")?;

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
                                is_loopback: false,
                                forwarded_port: None,
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
                            is_loopback: false,
                            forwarded_port: None,
                        });
                    }
                }
            }
        }
    }

    Ok(entries)
}

/// Collect LISTEN ports from inside a Docker container via `ss -tln`.
/// When `remote_host` is Some, the command is run via SSH on the remote host.
#[allow(clippy::unused_async)]
pub async fn collect_from_container(
    container: &str,
    remote_host: Option<&str>,
) -> Result<Vec<PortEntry>> {
    let docker_cmd = format!("docker exec {container} ss -tln");
    let output = match remote_host {
        Some(host) => match Command::new("ssh").arg(host).arg(&docker_cmd).output() {
            Ok(o) => o,
            Err(e) => anyhow::bail!("Failed to run ss in container via SSH: {e}"),
        },
        None => {
            match Command::new("docker")
                .args(["exec", container, "ss", "-tln"])
                .output()
            {
                Ok(o) => o,
                Err(e) => anyhow::bail!("Failed to run ss in container: {e}"),
            }
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "ss command failed in container '{}': {}",
            container,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ss_output(&stdout, container))
}

/// Parse `ss -tln` output from inside a container.
///
/// Example output:
/// ```text
/// State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
/// LISTEN 0      511           *:3000              *:*
/// LISTEN 0      511     0.0.0.0:5173        0.0.0.0:*
/// LISTEN 0      128     127.0.0.1:5432      0.0.0.0:*
/// LISTEN 0      511        [::]:3000           [::]:*
/// ```
fn parse_ss_output(output: &str, container_name: &str) -> Vec<PortEntry> {
    let mut entries = Vec::new();
    let mut seen_ports = HashSet::new();

    for line in output.lines() {
        let trimmed = line.trim();
        // Skip header and empty lines
        if trimmed.is_empty() || trimmed.starts_with("State") {
            continue;
        }
        if !trimmed.starts_with("LISTEN") {
            continue;
        }

        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }

        let local_addr = fields[3];
        // Extract port: last segment after ':'
        let port = match local_addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse::<u16>().ok())
        {
            Some(p) if p > 0 => p,
            _ => continue,
        };

        // Determine bind address for forwardability
        let bind_addr = &local_addr[..local_addr.rfind(':').unwrap_or(0)];
        let is_loopback = bind_addr == "127.0.0.1" || bind_addr == "[::1]";

        // Deduplicate IPv4/IPv6 entries for the same port
        if !seen_ports.insert(port) {
            continue;
        }

        // Extract process name from Process column if available
        let process_name = if fields.len() > 5 {
            // Process column may contain "users:(("name",...))"
            let proc_field = fields[5..].join(" ");
            if let Some(start) = proc_field.find("((\"") {
                if let Some(end) = proc_field[start + 3..].find('"') {
                    proc_field[start + 3..start + 3 + end].to_string()
                } else {
                    container_name.to_string()
                }
            } else {
                container_name.to_string()
            }
        } else {
            container_name.to_string()
        };

        entries.push(PortEntry {
            source: PortSource::Docker,
            local_port: port,
            remote_host: Some(container_name.to_string()),
            remote_port: Some(port),
            process_name,
            pid: None,
            container_id: None,
            container_name: Some(container_name.to_string()),
            ssh_host: None,
            is_open: true,
            is_loopback,
            forwarded_port: None,
        });
    }

    entries
}

/// Get the IP address of a Docker container.
/// Uses `docker inspect` to retrieve the container's IP from its network settings.
pub fn get_container_ip(container: &str, remote_host: Option<&str>) -> Result<String> {
    let inspect_fmt = "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}";
    let output = match remote_host {
        Some(host) => {
            let cmd = format!("docker inspect -f '{inspect_fmt}' {container}");
            Command::new("ssh").arg(host).arg(&cmd).output()?
        }
        None => Command::new("docker")
            .args(["inspect", "-f", inspect_fmt, container])
            .output()?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Failed to get container IP for '{}': {}",
            container,
            stderr.trim()
        );
    }

    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ip.is_empty() {
        anyhow::bail!("Container '{container}' has no IP address");
    }
    Ok(ip)
}

/// Parse `ss -tn state established dst IP` output to count connections per peer port.
///
/// The peer port corresponds to the container port that received the connection.
/// Uses counts instead of a set to handle ports with existing connections.
fn parse_ss_peer_port_counts(output: &str) -> HashMap<u16, usize> {
    let mut counts = HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("State")
            || trimmed.starts_with("Recv-Q")
            || trimmed.starts_with("LISTEN")
        {
            continue;
        }

        // Fields: [Recv-Q, Send-Q, Local Address:Port, Peer Address:Port, ...]
        // or with State column: [State, Recv-Q, Send-Q, Local Address:Port, Peer Address:Port, ...]
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        let peer_addr = if fields.len() >= 4 {
            if fields[0].parse::<u32>().is_ok() {
                // Starts with number → no State column → peer is field 3
                fields.get(3)
            } else {
                // Starts with text → has State column → peer is field 4
                fields.get(4)
            }
        } else {
            None
        };

        if let Some(addr) = peer_addr {
            if let Some(port_str) = addr.rsplit(':').next() {
                if let Ok(port) = port_str.parse::<u16>() {
                    if port > 0 {
                        *counts.entry(port).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    counts
}

/// Run `ss -tn state established dst <container_ip>` on the remote host
/// and return peer (container) port counts.
///
/// Checks from the host side because SSH tunnel connections may not be
/// visible inside the container's network namespace.
fn get_host_to_container_port_counts(
    container_ip: &str,
    remote_host: &str,
) -> Result<HashMap<u16, usize>> {
    let ss_cmd = format!("ss -tn state established dst {container_ip}");
    let output = Command::new("ssh")
        .arg(remote_host)
        .arg(&ss_cmd)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ss_peer_port_counts(&stdout))
}

/// Detect SSH forward mappings by probing each SSH-owned local port.
///
/// For each SSH port, connects to `127.0.0.1:{ssh_port}`, then checks
/// which container port's connection count increased on the remote host via
/// `ss -tn state established dst <container_ip>`. Uses counts (not sets)
/// to handle ports that already have existing connections.
///
/// Returns a map of `container_port → local_port`.
pub async fn detect_forward_mappings(
    container: &str,
    remote_host: &str,
    ssh_ports: &[u16],
    container_ports: &HashSet<u16>,
) -> Result<HashMap<u16, u16>> {
    let container_ip = get_container_ip(container, Some(remote_host))?;
    let mut result: HashMap<u16, u16> = HashMap::new();
    let mut detected_container_ports: HashSet<u16> = HashSet::new();

    // Get baseline established connection counts (host → container)
    let baseline = get_host_to_container_port_counts(&container_ip, remote_host)?;

    for &ssh_port in ssh_ports {
        // Connect to the SSH tunnel local port
        let addr = format!("127.0.0.1:{ssh_port}");
        let connect_result = tokio::time::timeout(
            Duration::from_millis(500),
            tokio::net::TcpStream::connect(&addr),
        )
        .await;

        let Ok(Ok(stream)) = connect_result else {
            continue; // Port not connectable (e.g. SOCKS proxy or dead tunnel)
        };

        // Wait for the connection to propagate through SSH tunnel
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Get current established connection counts (host → container)
        let Ok(current) = get_host_to_container_port_counts(&container_ip, remote_host) else {
            continue;
        };

        // Find container ports where connection count increased
        let new_ports: Vec<u16> = current
            .iter()
            .filter(|(&port, &count)| {
                count > baseline.get(&port).copied().unwrap_or(0)
                    && container_ports.contains(&port)
                    && !detected_container_ports.contains(&port)
            })
            .map(|(&port, _)| port)
            .collect();

        if new_ports.len() == 1 {
            let container_port = new_ports[0];
            result.insert(container_port, ssh_port);
            detected_container_ports.insert(container_port);
        }

        // Drop the connection and wait for cleanup
        drop(stream);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(result)
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
        let output = "abc123\tpostgres\t0.0.0.0:5432->5432/tcp, :::5432->5432/tcp";
        let entries = parse_docker_ps(output, false).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 5432);
    }

    #[test]
    fn test_parse_docker_ps_empty() {
        let entries = parse_docker_ps("", false).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_ss_output() {
        let output = "\
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      511           *:3000              *:*
LISTEN 0      511     0.0.0.0:5173        0.0.0.0:*
";
        let entries = parse_ss_output(output, "mycontainer");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 3000);
        assert_eq!(entries[0].source, PortSource::Docker);
        assert!(entries[0].is_open);
        assert!(!entries[0].is_loopback);
        assert_eq!(entries[0].container_name, Some("mycontainer".to_string()));
        assert_eq!(entries[0].process_name, "mycontainer");
        assert_eq!(entries[1].local_port, 5173);
        assert!(!entries[1].is_loopback);
    }

    #[test]
    fn test_parse_ss_output_ipv6_dedup() {
        let output = "\
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      511           *:3000              *:*
LISTEN 0      511        [::]:3000           [::]:*
LISTEN 0      511     0.0.0.0:5173        0.0.0.0:*
LISTEN 0      511        [::]:5173           [::]:*
";
        let entries = parse_ss_output(output, "test");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 3000);
        assert_eq!(entries[1].local_port, 5173);
    }

    #[test]
    fn test_parse_ss_output_loopback() {
        let output = "\
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      128     127.0.0.1:5432      0.0.0.0:*
LISTEN 0      511     0.0.0.0:3000        0.0.0.0:*
";
        let entries = parse_ss_output(output, "test");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 5432);
        assert!(entries[0].is_loopback);
        assert_eq!(entries[1].local_port, 3000);
        assert!(!entries[1].is_loopback);
    }

    #[test]
    fn test_parse_ss_output_with_process() {
        let output = "\
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      511     0.0.0.0:3000        0.0.0.0:*     users:((\"node\",pid=123,fd=4))
";
        let entries = parse_ss_output(output, "mycontainer");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].process_name, "node");
    }

    #[test]
    fn test_collect_from_container_entries_have_is_open_reset_pattern() {
        // Entries from parse_ss_output default to is_open=true.
        // In collect_all(), Docker Target mode resets is_open=false before
        // probing localhost, so only forwarded ports show as open.
        let output = "\
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      511           *:3000              *:*
LISTEN 0      511     0.0.0.0:5173        0.0.0.0:*
";
        let mut entries = parse_ss_output(output, "mycontainer");
        // parse_ss_output sets is_open=true for all entries
        assert!(entries.iter().all(|e| e.is_open));

        // Simulate the reset that collect_all() performs
        for entry in entries.iter_mut() {
            entry.is_open = false;
        }
        assert!(entries.iter().all(|e| !e.is_open));
    }

    #[test]
    fn test_parse_ss_output_empty() {
        let output = "State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process\n";
        let entries = parse_ss_output(output, "test");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_ss_peer_port_counts() {
        // ss -tn state established dst 172.28.0.2 on host:
        // Local is host side, Peer is container side
        let output = "\
Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
0      0       172.28.0.1:32870     172.28.0.2:1234
0      0       172.28.0.1:45678     172.28.0.2:3000
";
        let counts = parse_ss_peer_port_counts(output);
        assert_eq!(counts.get(&1234), Some(&1));
        assert_eq!(counts.get(&3000), Some(&1));
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn test_parse_ss_peer_port_counts_with_state_column() {
        let output = "\
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
ESTAB  0      0       172.28.0.1:32870     172.28.0.2:1234
";
        let counts = parse_ss_peer_port_counts(output);
        assert_eq!(counts.get(&1234), Some(&1));
        assert_eq!(counts.len(), 1);
    }

    #[test]
    fn test_parse_ss_peer_port_counts_empty() {
        let output = "Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process\n";
        let counts = parse_ss_peer_port_counts(output);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_parse_ss_peer_port_counts_multiple_connections() {
        // Two connections to same container port (1234), one to port 3000
        let output = "\
Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
0      0       172.28.0.1:32870     172.28.0.2:1234
0      0       172.28.0.1:32871     172.28.0.2:1234
0      0       172.28.0.1:45678     172.28.0.2:3000
";
        let counts = parse_ss_peer_port_counts(output);
        assert_eq!(counts.get(&1234), Some(&2));
        assert_eq!(counts.get(&3000), Some(&1));
    }
}
