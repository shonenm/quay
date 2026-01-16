use super::{PortEntry, PortSource};
use anyhow::Result;
use regex::Regex;
use std::process::Command;

/// Create an SSH port forward
/// spec format: "local_port:remote_host:remote_port"
pub fn create_forward(spec: &str, host: &str, remote: bool) -> Result<u32> {
    let flag = if remote { "-R" } else { "-L" };

    let child = Command::new("ssh")
        .args(["-f", "-N", flag, spec, host])
        .spawn()?;

    Ok(child.id())
}

pub async fn collect() -> Result<Vec<PortEntry>> {
    let output = Command::new("ps").args(["aux"]).output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ssh_forwards(&stdout)
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
                    remote_host: Some(format!("(R) {}:{}", local_host, remote_port)),
                    remote_port: Some(remote_port),
                    process_name: "ssh -R".to_string(),
                    pid,
                    container_id: None,
                    container_name: None,
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
    }

    #[test]
    fn test_parse_ssh_remote_forward() {
        let output =
            "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -R 8080:localhost:3000 remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 3000);
        assert_eq!(entries[0].process_name, "ssh -R");
    }

    #[test]
    fn test_parse_ssh_multiple_forwards() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh -L 9000:localhost:80 -L 9001:localhost:443 remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 9000);
        assert_eq!(entries[1].local_port, 9001);
    }

    #[test]
    fn test_parse_ssh_no_forwards() {
        let output = "user  12345  0.0  0.1 123456 7890 ?  Ss  10:00  0:00 ssh remote";
        let entries = parse_ssh_forwards(output).unwrap();
        assert!(entries.is_empty());
    }
}
