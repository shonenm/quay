pub mod check;
pub mod listen;
pub mod mock;

use crate::port::{PortEntry, PortSource};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum DevCommands {
    /// Start TCP listeners on specified ports
    Listen {
        /// Ports to listen on
        ports: Vec<u16>,
        /// Respond with HTTP 200 to connections
        #[arg(long)]
        http: bool,
    },
    /// Run a predefined scenario (set of listeners)
    Scenario {
        /// Scenario name (web, micro, full)
        name: Option<String>,
        /// List available scenarios
        #[arg(long)]
        list: bool,
    },
    /// Check if ports are open or closed
    Check {
        /// Ports to check
        ports: Vec<u16>,
    },
    /// Launch TUI with mock data (no real port scanning)
    Mock,
}

pub struct ScenarioEntry {
    pub port: u16,
    pub label: &'static str,
    pub should_listen: bool,
}

pub struct Scenario {
    pub name: &'static str,
    pub description: &'static str,
    pub entries: &'static [ScenarioEntry],
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "web",
        description: "Web app + DB + Cache",
        entries: &[
            ScenarioEntry {
                port: 3000,
                label: "web-app",
                should_listen: true,
            },
            ScenarioEntry {
                port: 5432,
                label: "postgres",
                should_listen: true,
            },
            ScenarioEntry {
                port: 6379,
                label: "redis",
                should_listen: true,
            },
        ],
    },
    Scenario {
        name: "micro",
        description: "5 microservices",
        entries: &[
            ScenarioEntry {
                port: 3001,
                label: "svc-auth",
                should_listen: true,
            },
            ScenarioEntry {
                port: 3002,
                label: "svc-users",
                should_listen: true,
            },
            ScenarioEntry {
                port: 3003,
                label: "svc-orders",
                should_listen: true,
            },
            ScenarioEntry {
                port: 3004,
                label: "svc-payments",
                should_listen: true,
            },
            ScenarioEntry {
                port: 3005,
                label: "svc-notifications",
                should_listen: true,
            },
        ],
    },
    Scenario {
        name: "full",
        description: "Mixed open/closed ports",
        entries: &[
            ScenarioEntry {
                port: 3000,
                label: "web-app",
                should_listen: true,
            },
            ScenarioEntry {
                port: 5432,
                label: "postgres",
                should_listen: true,
            },
            ScenarioEntry {
                port: 6379,
                label: "redis",
                should_listen: true,
            },
            ScenarioEntry {
                port: 8080,
                label: "proxy (inactive)",
                should_listen: false,
            },
            ScenarioEntry {
                port: 9090,
                label: "metrics (inactive)",
                should_listen: false,
            },
        ],
    },
];

pub fn find_scenario(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|s| s.name == name)
}

pub async fn run_dev(cmd: DevCommands) -> Result<()> {
    match cmd {
        DevCommands::Listen { ports, http } => listen::run(ports, http).await,
        DevCommands::Scenario { name, list } => run_scenario(name, list).await,
        DevCommands::Check { ports } => check::run(ports).await,
        DevCommands::Mock => mock::run().await,
    }
}

async fn run_scenario(name: Option<String>, list: bool) -> Result<()> {
    if list {
        println!("Available scenarios:");
        println!("{:<10} {:<30} PORTS", "NAME", "DESCRIPTION");
        println!("{}", "-".repeat(60));
        for scenario in SCENARIOS {
            let ports: Vec<String> = scenario
                .entries
                .iter()
                .map(|e| {
                    if e.should_listen {
                        format!("{}", e.port)
                    } else {
                        format!("{}(off)", e.port)
                    }
                })
                .collect();
            println!(
                "{:<10} {:<30} {}",
                scenario.name,
                scenario.description,
                ports.join(", ")
            );
        }
        return Ok(());
    }

    let name = name.ok_or_else(|| {
        anyhow::anyhow!("Scenario name required. Use --list to see available scenarios.")
    })?;
    let scenario = find_scenario(&name).ok_or_else(|| {
        anyhow::anyhow!("Unknown scenario '{name}'. Use --list to see available scenarios.")
    })?;

    println!(
        "Starting scenario '{}': {}",
        scenario.name, scenario.description
    );

    // Collect ports that should listen
    let listen_ports: Vec<u16> = scenario
        .entries
        .iter()
        .filter(|e| e.should_listen)
        .map(|e| e.port)
        .collect();

    // Spawn background listeners for open ports (best-effort; ports may already be in use)
    let handles = if listen_ports.is_empty() {
        Vec::new()
    } else {
        match listen::spawn_listeners(listen_ports, false).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Note: could not bind listeners ({e}), showing scenario entries only");
                Vec::new()
            }
        }
    };

    // Build PortEntry list from all scenario entries
    let mut entries: Vec<PortEntry> = scenario
        .entries
        .iter()
        .map(|e| PortEntry {
            source: PortSource::Local,
            local_port: e.port,
            remote_host: None,
            remote_port: None,
            process_name: e.label.to_string(),
            pid: None,
            container_id: None,
            container_name: None,
            ssh_host: None,
            is_open: e.should_listen,
            is_loopback: false,
        })
        .collect();
    entries.sort_by_key(|e| (!e.is_open, e.local_port));

    // Launch TUI with the scenario entries
    let result = crate::run_tui_with_entries(Some(entries), None, None).await;

    // Abort listeners on TUI exit
    for handle in handles {
        handle.abort();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_lookup() {
        assert!(find_scenario("web").is_some());
        assert!(find_scenario("micro").is_some());
        assert!(find_scenario("full").is_some());
        assert!(find_scenario("nonexistent").is_none());
    }

    #[test]
    fn test_scenario_web_ports() {
        let scenario = find_scenario("web").unwrap();
        let ports: Vec<u16> = scenario.entries.iter().map(|e| e.port).collect();
        assert_eq!(ports, vec![3000, 5432, 6379]);
    }

    #[test]
    fn test_scenario_micro_has_five() {
        let scenario = find_scenario("micro").unwrap();
        assert_eq!(scenario.entries.len(), 5);
    }

    #[test]
    fn test_scenario_full_has_inactive() {
        let scenario = find_scenario("full").unwrap();
        let inactive: Vec<_> = scenario
            .entries
            .iter()
            .filter(|e| !e.should_listen)
            .collect();
        assert!(!inactive.is_empty());
        assert_eq!(inactive.len(), 2);
    }
}
