use super::{PortEntry, PortSource};
use anyhow::Result;
use std::process::Command;

pub async fn collect() -> Result<Vec<PortEntry>> {
    // Use -F for machine-readable output
    // Fields: c=command, p=pid, n=name (includes address)
    let output = Command::new("lsof")
        .args(["-i", "-P", "-n", "-sTCP:LISTEN", "-Fcpn"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_lsof_fields(&stdout)
}

fn parse_lsof_fields(output: &str) -> Result<Vec<PortEntry>> {
    let mut entries = Vec::new();
    let mut current_pid: Option<u32> = None;
    let mut current_command: Option<String> = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let field_type = line.chars().next().unwrap_or(' ');
        let value = &line[1..];

        match field_type {
            'p' => {
                current_pid = value.parse().ok();
            }
            'c' => {
                current_command = Some(value.to_string());
            }
            'n' => {
                // Parse address like "*:3000" or "127.0.0.1:8080" or "[::1]:8080"
                if let Some(port) = extract_port(value) {
                    entries.push(PortEntry {
                        source: PortSource::Local,
                        local_port: port,
                        remote_host: None,
                        remote_port: None,
                        process_name: current_command.clone().unwrap_or_default(),
                        pid: current_pid,
                        container_id: None,
                        container_name: None,
                    });
                }
            }
            _ => {}
        }
    }

    // Remove duplicates by port, keeping first occurrence
    entries.sort_by_key(|e| e.local_port);
    entries.dedup_by_key(|e| e.local_port);

    Ok(entries)
}

fn extract_port(addr: &str) -> Option<u16> {
    // Handle IPv6 like "[::1]:8080" or "*:8080" or "127.0.0.1:8080"
    addr.rsplit(':').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lsof_fields() {
        let output = "p12345\ncnode\nn*:3000\np5678\ncpython\nn127.0.0.1:8080\n";
        let entries = parse_lsof_fields(output).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].local_port, 3000);
        assert_eq!(entries[0].process_name, "node");
        assert_eq!(entries[0].pid, Some(12345));
        assert_eq!(entries[1].local_port, 8080);
        assert_eq!(entries[1].process_name, "python");
    }

    #[test]
    fn test_parse_lsof_ipv6() {
        let output = "p1234\ncnginx\nn[::1]:80\n";
        let entries = parse_lsof_fields(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].local_port, 80);
    }

    #[test]
    fn test_extract_port() {
        assert_eq!(extract_port("*:3000"), Some(3000));
        assert_eq!(extract_port("127.0.0.1:8080"), Some(8080));
        assert_eq!(extract_port("[::1]:80"), Some(80));
        assert_eq!(extract_port("invalid"), None);
    }
}
