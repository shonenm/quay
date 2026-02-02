use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    #[serde(default)]
    pub remote_host: Option<String>,
    #[serde(default)]
    pub docker_target: Option<String>,
}

impl Connection {
    pub fn local() -> Self {
        Self {
            name: "Local".to_string(),
            remote_host: None,
            docker_target: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Connections {
    #[serde(default)]
    pub connection: Vec<Connection>,
}

impl Connections {
    pub fn connections_path() -> Option<PathBuf> {
        Config::config_dir().map(|p| p.join("connections.toml"))
    }

    pub fn load() -> Self {
        Self::connections_path()
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
        let Some(path) = Self::connections_path() else {
            anyhow::bail!("Could not determine config directory");
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Returns all connections with Local auto-inserted at index 0.
    pub fn all_with_local(&self) -> Vec<Connection> {
        let mut result = vec![Connection::local()];
        result.extend(self.connection.clone());
        result
    }

    pub fn add(&mut self, conn: Connection) {
        self.connection.push(conn);
    }

    /// Remove a connection by index in the user-defined list (not including Local).
    /// Returns true if the connection was removed.
    pub fn remove(&mut self, index: usize) -> bool {
        if index < self.connection.len() {
            self.connection.remove(index);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_connection() {
        let local = Connection::local();
        assert_eq!(local.name, "Local");
        assert!(local.remote_host.is_none());
        assert!(local.docker_target.is_none());
    }

    #[test]
    fn test_default_connections() {
        let conns = Connections::default();
        assert!(conns.connection.is_empty());
    }

    #[test]
    fn test_all_with_local() {
        let conns = Connections {
            connection: vec![Connection {
                name: "Production".to_string(),
                remote_host: Some("user@prod".to_string()),
                docker_target: None,
            }],
        };
        let all = conns.all_with_local();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "Local");
        assert_eq!(all[1].name, "Production");
    }

    #[test]
    fn test_all_with_local_empty() {
        let conns = Connections::default();
        let all = conns.all_with_local();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Local");
    }

    #[test]
    fn test_add_connection() {
        let mut conns = Connections::default();
        conns.add(Connection {
            name: "Test".to_string(),
            remote_host: Some("test@host".to_string()),
            docker_target: None,
        });
        assert_eq!(conns.connection.len(), 1);
        assert_eq!(conns.connection[0].name, "Test");
    }

    #[test]
    fn test_remove_connection() {
        let mut conns = Connections {
            connection: vec![
                Connection {
                    name: "A".to_string(),
                    remote_host: None,
                    docker_target: None,
                },
                Connection {
                    name: "B".to_string(),
                    remote_host: None,
                    docker_target: None,
                },
            ],
        };
        assert!(conns.remove(0));
        assert_eq!(conns.connection.len(), 1);
        assert_eq!(conns.connection[0].name, "B");
    }

    #[test]
    fn test_remove_out_of_bounds() {
        let mut conns = Connections::default();
        assert!(!conns.remove(0));
    }

    #[test]
    fn test_parse_connections_toml() {
        let toml = r#"
[[connection]]
name = "Production"
remote_host = "user@prod-server"

[[connection]]
name = "AI Lab + Docker"
remote_host = "ailab"
docker_target = "syntopic-dev"
"#;
        let conns: Connections = toml::from_str(toml).unwrap();
        assert_eq!(conns.connection.len(), 2);
        assert_eq!(conns.connection[0].name, "Production");
        assert_eq!(
            conns.connection[0].remote_host,
            Some("user@prod-server".to_string())
        );
        assert!(conns.connection[0].docker_target.is_none());
        assert_eq!(conns.connection[1].name, "AI Lab + Docker");
        assert_eq!(
            conns.connection[1].remote_host,
            Some("ailab".to_string())
        );
        assert_eq!(
            conns.connection[1].docker_target,
            Some("syntopic-dev".to_string())
        );
    }

    #[test]
    fn test_serialize_connections() {
        let conns = Connections {
            connection: vec![Connection {
                name: "Test".to_string(),
                remote_host: Some("host".to_string()),
                docker_target: None,
            }],
        };
        let serialized = toml::to_string_pretty(&conns).unwrap();
        assert!(serialized.contains("[[connection]]"));
        assert!(serialized.contains("name = \"Test\""));
        assert!(serialized.contains("remote_host = \"host\""));
    }
}
