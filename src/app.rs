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

    pub fn to_spec(&self) -> Option<(String, String)> {
        let local_port: u16 = self.local_port.parse().ok()?;
        let remote_port: u16 = self.remote_port.parse().ok()?;
        if self.remote_host.is_empty() || self.ssh_host.is_empty() {
            return None;
        }
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
        }
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
        self.auto_refresh && self.tick_count > 0 && self.tick_count.is_multiple_of(self.refresh_ticks)
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
                            .map(|h| h.to_lowercase().contains(&query))
                            .unwrap_or(false)
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
}
