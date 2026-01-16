use crate::port::{PortEntry, PortSource};

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
        }
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
