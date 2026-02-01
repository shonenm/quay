use crate::port::{PortEntry, PortSource};
use anyhow::Result;

pub fn generate_mock_entries() -> Vec<PortEntry> {
    let mut entries = vec![
        // Local x 3
        PortEntry {
            source: PortSource::Local,
            local_port: 3000,
            remote_host: None,
            remote_port: None,
            process_name: "node".to_string(),
            pid: Some(1234),
            container_id: None,
            container_name: None,
            is_open: true,
        },
        PortEntry {
            source: PortSource::Local,
            local_port: 8080,
            remote_host: None,
            remote_port: None,
            process_name: "python".to_string(),
            pid: Some(2345),
            container_id: None,
            container_name: None,
            is_open: true,
        },
        PortEntry {
            source: PortSource::Local,
            local_port: 4200,
            remote_host: None,
            remote_port: None,
            process_name: "ng".to_string(),
            pid: Some(3456),
            container_id: None,
            container_name: None,
            is_open: false,
        },
        // SSH x 2
        PortEntry {
            source: PortSource::Ssh,
            local_port: 9000,
            remote_host: Some("db.internal".to_string()),
            remote_port: Some(5432),
            process_name: "ssh".to_string(),
            pid: Some(4567),
            container_id: None,
            container_name: None,
            is_open: true,
        },
        PortEntry {
            source: PortSource::Ssh,
            local_port: 9090,
            remote_host: Some("(R) localhost:9090".to_string()),
            remote_port: Some(9090),
            process_name: "ssh -R".to_string(),
            pid: Some(5678),
            container_id: None,
            container_name: None,
            is_open: false,
        },
        // Docker x 3
        PortEntry {
            source: PortSource::Docker,
            local_port: 5432,
            remote_host: None,
            remote_port: Some(5432),
            process_name: "postgres:15".to_string(),
            pid: None,
            container_id: Some("abc123def456".to_string()),
            container_name: Some("postgres".to_string()),
            is_open: true,
        },
        PortEntry {
            source: PortSource::Docker,
            local_port: 6379,
            remote_host: None,
            remote_port: Some(6379),
            process_name: "redis:7".to_string(),
            pid: None,
            container_id: Some("def456abc789".to_string()),
            container_name: Some("redis".to_string()),
            is_open: true,
        },
        PortEntry {
            source: PortSource::Docker,
            local_port: 27017,
            remote_host: None,
            remote_port: Some(27017),
            process_name: "mongo:6".to_string(),
            pid: None,
            container_id: Some("789abc123def".to_string()),
            container_name: Some("mongo".to_string()),
            is_open: false,
        },
    ];

    // Sort: open first, then by port number (same as collect_all)
    entries.sort_by_key(|e| (!e.is_open, e.local_port));
    entries
}

pub async fn run() -> Result<()> {
    let entries = generate_mock_entries();
    crate::run_tui_with_entries(Some(entries)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_mock_entries_not_empty() {
        let entries = generate_mock_entries();
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_mock_entries_have_all_sources() {
        let entries = generate_mock_entries();
        let sources: HashSet<_> = entries.iter().map(|e| &e.source).collect();
        assert!(sources.contains(&PortSource::Local));
        assert!(sources.contains(&PortSource::Ssh));
        assert!(sources.contains(&PortSource::Docker));
    }

    #[test]
    fn test_mock_entries_have_mixed_open_status() {
        let entries = generate_mock_entries();
        let has_open = entries.iter().any(|e| e.is_open);
        let has_closed = entries.iter().any(|e| !e.is_open);
        assert!(has_open);
        assert!(has_closed);
    }

    #[test]
    fn test_mock_entries_have_unique_ports() {
        let entries = generate_mock_entries();
        let ports: HashSet<u16> = entries.iter().map(|e| e.local_port).collect();
        assert_eq!(ports.len(), entries.len());
    }

    #[test]
    fn test_mock_docker_entries_have_container_fields() {
        let entries = generate_mock_entries();
        let docker_entries: Vec<_> = entries.iter().filter(|e| e.source == PortSource::Docker).collect();
        assert!(!docker_entries.is_empty());
        for entry in docker_entries {
            assert!(entry.container_id.is_some());
            assert!(entry.container_name.is_some());
        }
    }

    #[test]
    fn test_mock_local_entries_have_pid() {
        let entries = generate_mock_entries();
        let local_entries: Vec<_> = entries.iter().filter(|e| e.source == PortSource::Local).collect();
        assert!(!local_entries.is_empty());
        for entry in local_entries {
            assert!(entry.pid.is_some());
        }
    }
}
