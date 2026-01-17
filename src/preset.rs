use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub key: Option<String>,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub ssh_host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Presets {
    #[serde(default)]
    pub preset: Vec<Preset>,
}

impl Presets {
    pub fn presets_path() -> Option<PathBuf> {
        Config::config_dir().map(|p| p.join("presets.toml"))
    }

    pub fn load() -> Self {
        Self::presets_path()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_presets() {
        let presets = Presets::default();
        assert!(presets.preset.is_empty());
    }

    #[test]
    fn test_parse_presets() {
        let toml = r#"
[[preset]]
name = "Production DB"
key = "1"
local_port = 5432
remote_host = "localhost"
remote_port = 5432
ssh_host = "prod-bastion"

[[preset]]
name = "Staging Redis"
local_port = 6379
remote_host = "localhost"
remote_port = 6379
ssh_host = "staging-bastion"
"#;
        let presets: Presets = toml::from_str(toml).unwrap();
        assert_eq!(presets.preset.len(), 2);
        assert_eq!(presets.preset[0].name, "Production DB");
        assert_eq!(presets.preset[0].key, Some("1".to_string()));
        assert_eq!(presets.preset[0].local_port, 5432);
        assert_eq!(presets.preset[1].name, "Staging Redis");
        assert_eq!(presets.preset[1].key, None);
    }
}
