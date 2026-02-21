use crate::config::Config;
use crate::connection::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardMapping {
    pub connection: String,
    pub container_port: u16,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Forwards {
    #[serde(default)]
    pub forward: Vec<ForwardMapping>,
}

impl Forwards {
    pub fn forwards_path() -> Option<PathBuf> {
        Config::config_dir().map(|p| p.join("forwards.toml"))
    }

    pub fn load() -> Self {
        Self::forwards_path()
            .and_then(|path| {
                if path.exists() {
                    fs::read_to_string(&path).ok()
                } else {
                    None
                }
            })
            .and_then(|content| toml::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = Self::forwards_path() else {
            anyhow::bail!("Could not determine config directory");
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn to_runtime(&self, connections: &[Connection]) -> HashMap<usize, HashMap<u16, u16>> {
        let mut result: HashMap<usize, HashMap<u16, u16>> = HashMap::new();
        for fwd in &self.forward {
            if let Some(idx) = connections.iter().position(|c| c.name == fwd.connection) {
                result
                    .entry(idx)
                    .or_default()
                    .insert(fwd.container_port, fwd.local_port);
            }
        }
        result
    }

    pub fn from_runtime(
        ssh_forwards: &HashMap<usize, HashMap<u16, u16>>,
        connections: &[Connection],
    ) -> Self {
        let mut forward = Vec::new();
        for (&conn_idx, port_map) in ssh_forwards {
            if let Some(conn) = connections.get(conn_idx) {
                for (&container_port, &local_port) in port_map {
                    forward.push(ForwardMapping {
                        connection: conn.name.clone(),
                        container_port,
                        local_port,
                    });
                }
            }
        }
        forward.sort_by(|a, b| {
            a.connection
                .cmp(&b.connection)
                .then(a.container_port.cmp(&b.container_port))
        });
        Self { forward }
    }

    pub fn remove_stale(&mut self) -> bool {
        let original_len = self.forward.len();
        self.forward.retain(|fwd| is_port_listening(fwd.local_port));
        self.forward.len() != original_len
    }
}

pub fn is_port_listening(port: u16) -> bool {
    let addr = format!("127.0.0.1:{port}");
    TcpStream::connect_timeout(
        &addr.parse().expect("valid socket addr"),
        Duration::from_millis(200),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_forwards() {
        let fwds = Forwards::default();
        assert!(fwds.forward.is_empty());
    }

    #[test]
    fn test_parse_forwards_toml() {
        let toml_str = r#"
[[forward]]
connection = "AI Lab"
container_port = 3000
local_port = 3000

[[forward]]
connection = "AI Lab"
container_port = 8080
local_port = 18080
"#;
        let fwds: Forwards = toml::from_str(toml_str).unwrap();
        assert_eq!(fwds.forward.len(), 2);
        assert_eq!(fwds.forward[0].connection, "AI Lab");
        assert_eq!(fwds.forward[0].container_port, 3000);
        assert_eq!(fwds.forward[0].local_port, 3000);
        assert_eq!(fwds.forward[1].container_port, 8080);
        assert_eq!(fwds.forward[1].local_port, 18080);
    }

    #[test]
    fn test_serialize_forwards() {
        let fwds = Forwards {
            forward: vec![ForwardMapping {
                connection: "Test".to_string(),
                container_port: 5432,
                local_port: 15432,
            }],
        };
        let serialized = toml::to_string_pretty(&fwds).unwrap();
        assert!(serialized.contains("[[forward]]"));
        assert!(serialized.contains("connection = \"Test\""));
        assert!(serialized.contains("container_port = 5432"));
        assert!(serialized.contains("local_port = 15432"));
    }

    #[test]
    fn test_to_runtime() {
        let fwds = Forwards {
            forward: vec![
                ForwardMapping {
                    connection: "Remote".to_string(),
                    container_port: 3000,
                    local_port: 3000,
                },
                ForwardMapping {
                    connection: "Remote".to_string(),
                    container_port: 8080,
                    local_port: 18080,
                },
            ],
        };
        let connections = vec![
            Connection::local(),
            Connection {
                name: "Remote".to_string(),
                remote_host: Some("ailab".to_string()),
                docker_target: Some("dev".to_string()),
            },
        ];
        let runtime = fwds.to_runtime(&connections);
        assert_eq!(runtime.len(), 1);
        let map = runtime.get(&1).unwrap();
        assert_eq!(map.get(&3000), Some(&3000));
        assert_eq!(map.get(&8080), Some(&18080));
    }

    #[test]
    fn test_to_runtime_skips_unknown_connection() {
        let fwds = Forwards {
            forward: vec![ForwardMapping {
                connection: "Deleted".to_string(),
                container_port: 3000,
                local_port: 3000,
            }],
        };
        let connections = vec![Connection::local()];
        let runtime = fwds.to_runtime(&connections);
        assert!(runtime.is_empty());
    }

    #[test]
    fn test_from_runtime() {
        let connections = vec![
            Connection::local(),
            Connection {
                name: "MyServer".to_string(),
                remote_host: Some("host".to_string()),
                docker_target: None,
            },
        ];
        let mut ssh_forwards = HashMap::new();
        let mut map = HashMap::new();
        map.insert(3000u16, 3000u16);
        map.insert(8080u16, 18080u16);
        ssh_forwards.insert(1usize, map);

        let fwds = Forwards::from_runtime(&ssh_forwards, &connections);
        assert_eq!(fwds.forward.len(), 2);
        assert_eq!(fwds.forward[0].container_port, 3000);
        assert_eq!(fwds.forward[1].container_port, 8080);
        assert_eq!(fwds.forward[1].local_port, 18080);
        assert!(fwds.forward.iter().all(|f| f.connection == "MyServer"));
    }

    #[test]
    fn test_roundtrip() {
        let connections = vec![
            Connection::local(),
            Connection {
                name: "Remote".to_string(),
                remote_host: Some("host".to_string()),
                docker_target: Some("container".to_string()),
            },
        ];
        let mut ssh_forwards = HashMap::new();
        let mut map = HashMap::new();
        map.insert(5432u16, 15432u16);
        ssh_forwards.insert(1usize, map);

        let fwds = Forwards::from_runtime(&ssh_forwards, &connections);
        let toml_str = toml::to_string_pretty(&fwds).unwrap();
        let loaded: Forwards = toml::from_str(&toml_str).unwrap();
        let runtime = loaded.to_runtime(&connections);

        assert_eq!(runtime.len(), 1);
        assert_eq!(runtime.get(&1).unwrap().get(&5432), Some(&15432));
    }
}
