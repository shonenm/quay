use crate::port::{PortEntry, PortSource};
use crate::preset::Preset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    None,
    Details,
    Help,
    Forward,
    Presets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForwardField {
    #[default]
    LocalPort,
    RemoteHost,
    RemotePort,
    SshHost,
}

impl ForwardField {
    pub fn next(self) -> Self {
        match self {
            ForwardField::LocalPort => ForwardField::RemoteHost,
            ForwardField::RemoteHost => ForwardField::RemotePort,
            ForwardField::RemotePort => ForwardField::SshHost,
            ForwardField::SshHost => ForwardField::LocalPort,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ForwardField::LocalPort => ForwardField::SshHost,
            ForwardField::RemoteHost => ForwardField::LocalPort,
            ForwardField::RemotePort => ForwardField::RemoteHost,
            ForwardField::SshHost => ForwardField::RemotePort,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ForwardInput {
    pub local_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub ssh_host: String,
    pub active_field: ForwardField,
}

impl ForwardInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_value(&mut self) -> &mut String {
        match self.active_field {
            ForwardField::LocalPort => &mut self.local_port,
            ForwardField::RemoteHost => &mut self.remote_host,
            ForwardField::RemotePort => &mut self.remote_port,
            ForwardField::SshHost => &mut self.ssh_host,
        }
    }

    pub fn is_local_port_valid(&self) -> bool {
        !self.local_port.is_empty() && self.local_port.parse::<u16>().is_ok()
    }

    pub fn is_remote_host_valid(&self) -> bool {
        !self.remote_host.trim().is_empty()
    }

    pub fn is_remote_port_valid(&self) -> bool {
        !self.remote_port.is_empty() && self.remote_port.parse::<u16>().is_ok()
    }

    pub fn is_ssh_host_valid(&self) -> bool {
        !self.ssh_host.trim().is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.is_local_port_valid()
            && self.is_remote_host_valid()
            && self.is_remote_port_valid()
            && self.is_ssh_host_valid()
    }

    pub fn invalid_field_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if !self.is_local_port_valid() {
            names.push("Local Port");
        }
        if !self.is_remote_host_valid() {
            names.push("Remote Host");
        }
        if !self.is_remote_port_valid() {
            names.push("Remote Port");
        }
        if !self.is_ssh_host_valid() {
            names.push("SSH Host");
        }
        names
    }

    pub fn from_entry(entry: &PortEntry) -> Self {
        let has_ssh_host = entry.ssh_host.as_ref().is_some_and(|h| !h.is_empty());
        Self {
            local_port: entry.local_port.to_string(),
            remote_host: "localhost".to_string(),
            remote_port: entry.local_port.to_string(),
            ssh_host: entry.ssh_host.clone().unwrap_or_default(),
            active_field: if has_ssh_host {
                ForwardField::LocalPort
            } else {
                ForwardField::SshHost
            },
        }
    }

    pub fn for_remote_entry(entry: &PortEntry, remote_host: &str) -> Self {
        Self {
            local_port: entry.local_port.to_string(),
            remote_host: "localhost".to_string(),
            remote_port: entry.local_port.to_string(),
            ssh_host: remote_host.to_string(),
            active_field: ForwardField::LocalPort,
        }
    }

    pub fn to_spec(&self) -> Option<(String, String)> {
        if !self.is_valid() {
            return None;
        }
        let local_port: u16 = self.local_port.parse().ok()?;
        let remote_port: u16 = self.remote_port.parse().ok()?;
        let spec = format!("{}:{}:{}", local_port, self.remote_host, remote_port);
        Some((spec, self.ssh_host.clone()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Local,
    Ssh,
    Docker,
}

pub struct App {
    pub entries: Vec<PortEntry>,
    pub filtered_entries: Vec<PortEntry>,
    pub selected: usize,
    pub filter: Filter,
    pub search_query: String,
    pub input_mode: InputMode,
    pub popup: Popup,
    pub should_quit: bool,
    pub forward_input: ForwardInput,
    pub auto_refresh: bool,
    pub tick_count: u32,
    pub refresh_ticks: u32,
    pub status_message: Option<(String, u32)>, // (message, ticks_remaining)
    pub presets: Vec<Preset>,
    pub preset_selected: usize,
    pub remote_host: Option<String>,
    pub docker_target: Option<String>,
    pub container_ip: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            filtered_entries: Vec::new(),
            selected: 0,
            filter: Filter::All,
            search_query: String::new(),
            input_mode: InputMode::Normal,
            popup: Popup::None,
            should_quit: false,
            forward_input: ForwardInput::new(),
            auto_refresh: false,
            tick_count: 0,
            refresh_ticks: 20,
            status_message: None,
            presets: Vec::new(),
            preset_selected: 0,
            remote_host: None,
            docker_target: None,
            container_ip: None,
        }
    }

    pub fn is_remote(&self) -> bool {
        self.remote_host.is_some()
    }

    pub fn is_docker_target(&self) -> bool {
        self.docker_target.is_some()
    }

    pub fn preset_next(&mut self) {
        if !self.presets.is_empty() {
            self.preset_selected = (self.preset_selected + 1) % self.presets.len();
        }
    }

    pub fn preset_previous(&mut self) {
        if !self.presets.is_empty() {
            self.preset_selected = self
                .preset_selected
                .checked_sub(1)
                .unwrap_or(self.presets.len() - 1);
        }
    }

    pub fn selected_preset(&self) -> Option<&Preset> {
        self.presets.get(self.preset_selected)
    }

    pub fn set_status(&mut self, message: &str) {
        // Show message for ~3 seconds (12 ticks at 250ms)
        self.status_message = Some((message.to_string(), 12));
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        // Decrement status message timer
        if let Some((_, ref mut ticks)) = self.status_message {
            if *ticks > 0 {
                *ticks -= 1;
            } else {
                self.status_message = None;
            }
        }
    }

    pub fn should_refresh(&self) -> bool {
        self.auto_refresh && self.tick_count > 0 && self.tick_count % self.refresh_ticks == 0
    }

    pub fn reset_forward_input(&mut self) {
        self.forward_input = ForwardInput::new();
    }

    pub fn set_entries(&mut self, entries: Vec<PortEntry>) {
        self.entries = entries;
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        self.filtered_entries = self
            .entries
            .iter()
            .filter(|e| {
                let source_match = match self.filter {
                    Filter::All => true,
                    Filter::Local => e.source == PortSource::Local,
                    Filter::Ssh => e.source == PortSource::Ssh,
                    Filter::Docker => e.source == PortSource::Docker,
                };

                let search_match = if self.search_query.is_empty() {
                    true
                } else {
                    let query = self.search_query.to_lowercase();
                    e.process_name.to_lowercase().contains(&query)
                        || e.local_port.to_string().contains(&query)
                        || e.remote_host
                            .as_ref()
                            .is_some_and(|h| h.to_lowercase().contains(&query))
                };

                source_match && search_match
            })
            .cloned()
            .collect();

        if self.selected >= self.filtered_entries.len() {
            self.selected = self.filtered_entries.len().saturating_sub(1);
        }
    }

    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
        self.apply_filter();
    }

    pub fn next(&mut self) {
        if !self.filtered_entries.is_empty() {
            self.selected = (self.selected + 1) % self.filtered_entries.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.filtered_entries.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered_entries.len() - 1);
        }
    }

    pub fn first(&mut self) {
        self.selected = 0;
    }

    pub fn last(&mut self) {
        if !self.filtered_entries.is_empty() {
            self.selected = self.filtered_entries.len() - 1;
        }
    }

    pub fn selected_entry(&self) -> Option<&PortEntry> {
        self.filtered_entries.get(self.selected)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_ticks_default() {
        let app = App::new();
        assert_eq!(app.refresh_ticks, 20);
    }

    #[test]
    fn test_should_refresh_uses_refresh_ticks() {
        let mut app = App::new();
        app.auto_refresh = true;
        app.refresh_ticks = 10;

        // tick_count 0 should not refresh (guard)
        app.tick_count = 0;
        assert!(!app.should_refresh());

        // tick_count 5 should not refresh
        app.tick_count = 5;
        assert!(!app.should_refresh());

        // tick_count 10 should refresh
        app.tick_count = 10;
        assert!(app.should_refresh());

        // tick_count 20 should refresh
        app.tick_count = 20;
        assert!(app.should_refresh());

        // auto_refresh off should not refresh
        app.auto_refresh = false;
        app.tick_count = 10;
        assert!(!app.should_refresh());
    }

    #[test]
    fn test_forward_input_empty_is_invalid() {
        let input = ForwardInput::new();
        assert!(!input.is_valid());
        assert!(!input.is_local_port_valid());
        assert!(!input.is_remote_host_valid());
        assert!(!input.is_remote_port_valid());
        assert!(!input.is_ssh_host_valid());
    }

    #[test]
    fn test_forward_input_valid() {
        let input = ForwardInput {
            local_port: "8080".to_string(),
            remote_host: "localhost".to_string(),
            remote_port: "80".to_string(),
            ssh_host: "myserver".to_string(),
            active_field: ForwardField::LocalPort,
        };
        assert!(input.is_valid());
        assert!(input.is_local_port_valid());
        assert!(input.is_remote_host_valid());
        assert!(input.is_remote_port_valid());
        assert!(input.is_ssh_host_valid());
    }

    #[test]
    fn test_forward_input_bad_port() {
        let input = ForwardInput {
            local_port: "99999".to_string(),
            remote_host: "localhost".to_string(),
            remote_port: "80".to_string(),
            ssh_host: "myserver".to_string(),
            active_field: ForwardField::LocalPort,
        };
        assert!(!input.is_local_port_valid());
        assert!(!input.is_valid());
    }

    #[test]
    fn test_forward_input_non_numeric_port() {
        let input = ForwardInput {
            local_port: "abc".to_string(),
            remote_host: "localhost".to_string(),
            remote_port: "80".to_string(),
            ssh_host: "myserver".to_string(),
            active_field: ForwardField::LocalPort,
        };
        assert!(!input.is_local_port_valid());
        assert!(!input.is_valid());
    }

    #[test]
    fn test_forward_input_whitespace_host() {
        let input = ForwardInput {
            local_port: "8080".to_string(),
            remote_host: "   ".to_string(),
            remote_port: "80".to_string(),
            ssh_host: "myserver".to_string(),
            active_field: ForwardField::LocalPort,
        };
        assert!(!input.is_remote_host_valid());
        assert!(!input.is_valid());
    }

    #[test]
    fn test_forward_input_from_entry() {
        let entry = PortEntry {
            source: PortSource::Local,
            local_port: 3000,
            remote_host: None,
            remote_port: None,
            process_name: "node".to_string(),
            pid: Some(1234),
            container_id: None,
            container_name: None,
            ssh_host: None,
            is_open: true,
            is_loopback: false,
        };
        let input = ForwardInput::from_entry(&entry);
        assert_eq!(input.local_port, "3000");
        assert_eq!(input.remote_host, "localhost");
        assert_eq!(input.remote_port, "3000");
        assert_eq!(input.ssh_host, "");
        assert_eq!(input.active_field, ForwardField::SshHost);
    }

    #[test]
    fn test_forward_input_from_entry_with_ssh_host() {
        let entry = PortEntry {
            source: PortSource::Ssh,
            local_port: 9000,
            remote_host: Some("localhost".to_string()),
            remote_port: Some(80),
            process_name: "ssh".to_string(),
            pid: Some(4567),
            container_id: None,
            container_name: None,
            ssh_host: Some("myserver".to_string()),
            is_open: true,
            is_loopback: false,
        };
        let input = ForwardInput::from_entry(&entry);
        assert_eq!(input.local_port, "9000");
        assert_eq!(input.remote_host, "localhost");
        assert_eq!(input.remote_port, "9000");
        assert_eq!(input.ssh_host, "myserver");
        assert_eq!(input.active_field, ForwardField::LocalPort);
    }

    #[test]
    fn test_forward_input_to_spec() {
        let input = ForwardInput {
            local_port: "8080".to_string(),
            remote_host: "localhost".to_string(),
            remote_port: "80".to_string(),
            ssh_host: "myserver".to_string(),
            active_field: ForwardField::LocalPort,
        };
        let (spec, host) = input.to_spec().unwrap();
        assert_eq!(spec, "8080:localhost:80");
        assert_eq!(host, "myserver");
    }

    #[test]
    fn test_forward_input_to_spec_invalid() {
        let input = ForwardInput::new();
        assert!(input.to_spec().is_none());
    }

    #[test]
    fn test_is_remote() {
        let mut app = App::new();
        assert!(!app.is_remote());
        app.remote_host = Some("user@server".to_string());
        assert!(app.is_remote());
    }

    #[test]
    fn test_is_docker_target() {
        let mut app = App::new();
        assert!(!app.is_docker_target());
        app.docker_target = Some("my-container".to_string());
        assert!(app.is_docker_target());
    }

    #[test]
    fn test_forward_input_for_remote_entry() {
        let entry = PortEntry {
            source: PortSource::Local,
            local_port: 18080,
            remote_host: None,
            remote_port: None,
            process_name: "python".to_string(),
            pid: Some(5555),
            container_id: None,
            container_name: None,
            ssh_host: None,
            is_open: true,
            is_loopback: false,
        };
        let input = ForwardInput::for_remote_entry(&entry, "user@server");
        assert_eq!(input.local_port, "18080");
        assert_eq!(input.remote_host, "localhost");
        assert_eq!(input.remote_port, "18080");
        assert_eq!(input.ssh_host, "user@server");
        assert_eq!(input.active_field, ForwardField::LocalPort);
    }

    #[test]
    fn test_forward_input_invalid_field_names() {
        let input = ForwardInput::new();
        let names = input.invalid_field_names();
        assert_eq!(names.len(), 4);

        let input = ForwardInput {
            local_port: "8080".to_string(),
            remote_host: "localhost".to_string(),
            remote_port: "80".to_string(),
            ssh_host: String::new(),
            active_field: ForwardField::LocalPort,
        };
        let names = input.invalid_field_names();
        assert_eq!(names, vec!["SSH Host"]);
    }
}
