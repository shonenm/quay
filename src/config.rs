use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub auto_refresh: bool,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: u32,
    #[serde(default = "default_filter")]
    pub default_filter: String,
    #[serde(default)]
    pub remote_host: Option<String>,
    #[serde(default)]
    pub docker_target: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub mouse_enabled: bool,
}

fn default_refresh_interval() -> u32 {
    5
}

fn default_filter() -> String {
    "all".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_refresh: false,
            refresh_interval: default_refresh_interval(),
            default_filter: default_filter(),
            remote_host: None,
            docker_target: None,
        }
    }
}

impl Config {
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("quay"))
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|p| p.join("config.toml"))
    }

    pub fn load() -> Self {
        Self::config_path()
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
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.general.auto_refresh);
        assert_eq!(config.general.refresh_interval, 5);
        assert_eq!(config.general.default_filter, "all");
        assert!(config.general.remote_host.is_none());
        assert!(config.general.docker_target.is_none());
        assert!(!config.ui.mouse_enabled);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[general]
auto_refresh = true
refresh_interval = 10
default_filter = "local"

[ui]
mouse_enabled = true
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.general.auto_refresh);
        assert_eq!(config.general.refresh_interval, 10);
        assert_eq!(config.general.default_filter, "local");
        assert!(config.ui.mouse_enabled);
    }

    #[test]
    fn test_parse_partial_config() {
        let toml = r"
[general]
auto_refresh = true
";
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.general.auto_refresh);
        assert_eq!(config.general.refresh_interval, 5);
        assert!(config.general.remote_host.is_none());
        assert!(!config.ui.mouse_enabled);
    }

    #[test]
    fn test_parse_config_with_remote_host() {
        let toml = r#"
[general]
remote_host = "user@server"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.general.remote_host, Some("user@server".to_string()));
    }

    #[test]
    fn test_parse_config_with_docker_target() {
        let toml = r#"
[general]
remote_host = "ailab"
docker_target = "syntopic-dev"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.general.remote_host, Some("ailab".to_string()));
        assert_eq!(
            config.general.docker_target,
            Some("syntopic-dev".to_string())
        );
    }
}
