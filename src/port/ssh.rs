use super::{PortEntry, PortSource};
use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
use std::process::Command;

/// Create an SSH port forward
/// spec format: "`local_port:remote_host:remote_port`"
pub fn create_forward(spec: &str, host: &str, remote: bool) -> Result<u32> {
    let flag = if remote { "-R" } else { "-L" };

    let child = Command::new("ssh")
        .args(["-f", "-N", flag, spec, host])
        .spawn()?;

    Ok(child.id())
}

/// Get the PID of the SSH `ControlMaster` for a given remote host.
///
/// Runs `ssh -O check host` and parses "Master running (pid=NNNNN)" from stderr.
fn get_control_master_pid(host: &str) -> Option<u32> {
    let output = Command::new("ssh")
        .args(["-O", "check", host])
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let re = Regex::new(r"pid=(\d+)").ok()?;
    re.captures(&stderr)?[1].parse().ok()
}

/// Get TCP LISTEN ports for a specific PID via `lsof`.
fn get_listening_ports_for_pid(pid: u32) -> Vec<u16> {
    let pid_str = pid.to_string();
    let output = match Command::new("lsof")
        .args(["-a", "-P", "-n", "-iTCP", "-sTCP:LISTEN", "-p", &pid_str, "-Fn"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_lsof_listen_ports(&stdout)
}

/// Detect LISTEN ports owned by the SSH `ControlMaster` for a specific host.
///
/// Uses `ssh -O check` to find the `ControlMaster` PID, then queries
/// only that PID's TCP LISTEN sockets via `lsof -a -p PID`.
pub fn get_ssh_master_listening_ports(remote_host: &str) -> Vec<u16> {
    let Some(pid) = get_control_master_pid(remote_host) else {
        return Vec::new();
    };
    get_listening_ports_for_pid(pid)
}

fn parse_lsof_listen_ports(output: &str) -> Vec<u16> {
    let mut ports = HashSet::new();

    for line in output.lines() {
        // lsof -F format: lines starting with 'n' contain network addresses
        // e.g. "n*:1235" or "n127.0.0.1:1235" or "n[::1]:1235"
        if let Some(addr) = line.strip_prefix('n') {
            if let Some(port_str) = addr.rsplit(':').next() {
                if let Ok(port) = port_str.parse::<u16>() {
                    if port > 0 {
                        ports.insert(port);
                    }
                }
            }
        }
    }

    let mut result: Vec<u16> = ports.into_iter().collect();
    result.sort_unstable();
    result
}

#[allow(clippy::unused_async)]
pub async fn collect() -> Result<Vec<PortEntry>> {
    let output = Command::new("ps").args(["aux"]).output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ssh_forwards(&stdout)
}

/// Extract the SSH host from the command tokens (everything after `ssh`).
/// The SSH host is the last token that doesn't start with `-` and doesn't contain `:`.
fn extract_ssh_host(line: &str) -> Option<String> {
    // Find the `ssh` command token and take everything after it
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let ssh_pos = tokens.iter().position(|t| {
        let base = t.rsplit('/').next().unwrap_or(t);
        base == "ssh"
    })?;
    let args = &tokens[ssh_pos + 1..];
    // Last token that doesn't start with `-` and doesn't contain `:`
    let last = args.last()?;
    if !last.starts_with('-') && !last.contains(':') {
        Some(last.to_string())
    } else {
        None
    }
}

fn parse_ssh_forwards(output: &str) -> Result<Vec<PortEntry>> {
    let mut entries = Vec::new();
    // -L local_port:remote_host:remote_port
    let local_forward_re = Regex::new(r"-L\s*(\d+):([^:\s]+):(\d+)")?;
    // -R remote_port:local_host:local_port (reverse)
    let remote_forward_re = Regex::new(r"-R\s*(\d+):([^:\s]+):(\d+)")?;

    for line in output.lines() {
        if !line.contains("ssh") {
            continue;
        }
        if !line.contains("-L") && !line.contains("-R") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let pid = parts[1].parse::<u32>().ok();
        let ssh_host = extract_ssh_host(line);

        // Local forwards (-L)
        for cap in local_forward_re.captures_iter(line) {
            let local_port = cap[1].parse::<u16>().unwrap_or(0);
            let remote_host = cap[2].to_string();
            let remote_port = cap[3].parse::<u16>().ok();

            if local_port > 0 {
                entries.push(PortEntry {
                    source: PortSource::Ssh,
                    local_port,
                    remote_host: Some(remote_host),
                    remote_port,
                    process_name: "ssh".to_string(),
                    pid,
                    container_id: None,
                    container_name: None,
                    ssh_host: ssh_host.clone(),
                    is_open: false,
                    is_loopback: false,
                    forwarded_port: None,
                });
            }
        }

        // Remote forwards (-R) - show local side
        for cap in remote_forward_re.captures_iter(line) {
            let remote_port = cap[1].parse::<u16>().unwrap_or(0);
            let local_host = cap[2].to_string();
            let local_port = cap[3].parse::<u16>().unwrap_or(0);

            if local_port > 0 {
                entries.push(PortEntry {
                    source: PortSource::Ssh,
                    local_port,
                    remote_host: Some(format!("(R) {local_host}:{remote_port}")),
                    remote_port: Some(remote_port),
                    process_name: "ssh -R".to_string(),
                    pid,
                    container_id: None,
                    container_name: None,
                    ssh_host: ssh_host.clone(),
                    is_open: false,
                    is_loopback: false,
                    forwarded_port: None,
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
    fn test_parse_ssh_local_forward() {
        let output =
            "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80 remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 9000);
        assert_eq!(entries[0].remote_host, Some("localhost".to_string()));
        assert_eq!(entries[0].remote_port, Some(80));
        assert_eq!(entries[0].process_name, "ssh");
        assert_eq!(entries[0].ssh_host, Some("remote".to_string()));
    }

    #[test]
    fn test_parse_ssh_remote_forward() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -R 8080:localhost:3000 remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 3000);
        assert_eq!(entries[0].process_name, "ssh -R");
        assert_eq!(entries[0].ssh_host, Some("remote".to_string()));
    }

    #[test]
    fn test_parse_ssh_multiple_forwards() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80 -L 9001:localhost:443 remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 9000);
        assert_eq!(entries[1].local_port, 9001);
        assert_eq!(entries[0].ssh_host, Some("remote".to_string()));
        assert_eq!(entries[1].ssh_host, Some("remote".to_string()));
    }

    #[test]
    fn test_parse_ssh_no_forwards() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_ssh_host_with_user_at() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80 user@example.com";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ssh_host, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_ssh_host_with_flags() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -f -N -L 9000:localhost:80 myserver";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ssh_host, Some("myserver".to_string()));
    }

    #[test]
    fn test_extract_ssh_host_basic() {
        let line =
            "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80 bastion";
        assert_eq!(extract_ssh_host(line), Some("bastion".to_string()));
    }

    #[test]
    fn test_extract_ssh_host_none_when_last_is_port_spec() {
        // Last token contains `:` — not a host
        let line = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80";
        assert_eq!(extract_ssh_host(line), None);
    }

    #[test]
    fn test_extract_ssh_host_none_when_last_is_flag() {
        let line =
            "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80 -N";
        assert_eq!(extract_ssh_host(line), None);
    }

    #[test]
    fn test_parse_lsof_listen_ports() {
        let output = "n*:1235\nn*:3108\nn[::1]:1235\nn127.0.0.1:4201\n";
        let ports = parse_lsof_listen_ports(output);
        assert_eq!(ports, vec![1235, 3108, 4201]);
    }

    #[test]
    fn test_parse_lsof_listen_ports_empty() {
        let ports = parse_lsof_listen_ports("");
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_lsof_listen_ports_no_network_lines() {
        let output = "fINET\n";
        let ports = parse_lsof_listen_ports(output);
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_lsof_skips_non_port_entries() {
        // Simulates lsof output with unix sockets, device files, etc.
        let output = "n/dev/ttys021\nn/Users/test/.ssh/sockets/user@host\nn*:1235\nn->0xabcdef\nn127.0.0.1:3108\n";
        let ports = parse_lsof_listen_ports(output);
        assert_eq!(ports, vec![1235, 3108]);
    }
}
