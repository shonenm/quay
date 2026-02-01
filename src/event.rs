use crate::app::{ForwardField, ForwardInput};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
}

pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    pub fn next(&self) -> anyhow::Result<AppEvent> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                Event::Key(key) => return Ok(AppEvent::Key(key)),
                Event::Mouse(mouse) => return Ok(AppEvent::Mouse(mouse)),
                _ => {}
            }
        }
        Ok(AppEvent::Tick)
    }
}

pub fn handle_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Up),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::First),
        KeyCode::Char('G') | KeyCode::End => Some(Action::Last),
        KeyCode::Char('/') => Some(Action::EnterSearch),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Char('a') => Some(Action::ToggleAutoRefresh),
        KeyCode::Char('f') => Some(Action::StartForward),
        KeyCode::Char('F') => Some(Action::QuickForward),
        KeyCode::Char('p') => Some(Action::ShowPresets),
        KeyCode::Char('0') => Some(Action::FilterAll),
        KeyCode::Char('1') => Some(Action::FilterLocal),
        KeyCode::Char('2') => Some(Action::FilterSsh),
        KeyCode::Char('3') => Some(Action::FilterDocker),
        KeyCode::Char('K') => Some(Action::Kill),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Enter => Some(Action::Select),
        _ => None,
    }
}

pub fn handle_popup_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => Some(Action::ClosePopup),
        _ => None,
    }
}

pub fn handle_search_key(key: KeyEvent, query: &mut String) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitSearch),
        KeyCode::Enter => Some(Action::ExitSearch),
        KeyCode::Backspace => {
            query.pop();
            Some(Action::UpdateSearch)
        }
        KeyCode::Char(c) => {
            query.push(c);
            Some(Action::UpdateSearch)
        }
        _ => None,
    }
}

pub fn handle_forward_key(key: KeyEvent, input: &mut ForwardInput, remote_mode: bool, docker_mode: bool) -> Option<Action> {
    let is_locked = |field: ForwardField| -> bool {
        (remote_mode && field == ForwardField::SshHost)
            || (docker_mode && field == ForwardField::RemoteHost)
    };

    match key.code {
        KeyCode::Esc => Some(Action::ClosePopup),
        KeyCode::Enter => {
            if input.is_valid() {
                Some(Action::SubmitForward)
            } else {
                None
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            input.active_field = input.active_field.next();
            // Skip locked fields
            if is_locked(input.active_field) {
                input.active_field = input.active_field.next();
            }
            // Second skip in case both are locked (remote+docker)
            if is_locked(input.active_field) {
                input.active_field = input.active_field.next();
            }
            None
        }
        KeyCode::BackTab | KeyCode::Up => {
            input.active_field = input.active_field.prev();
            if is_locked(input.active_field) {
                input.active_field = input.active_field.prev();
            }
            if is_locked(input.active_field) {
                input.active_field = input.active_field.prev();
            }
            None
        }
        KeyCode::Backspace => {
            if is_locked(input.active_field) {
                return None;
            }
            input.active_value().pop();
            None
        }
        KeyCode::Char(c) => {
            if is_locked(input.active_field) {
                return None;
            }
            input.active_value().push(c);
            None
        }
        _ => None,
    }
}

pub fn handle_preset_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::ClosePopup),
        KeyCode::Enter => Some(Action::LaunchPreset),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Up),
        _ => None,
    }
}

pub fn handle_mouse(event: MouseEvent, table_top: u16, table_height: u16) -> Option<Action> {
    match event.kind {
        MouseEventKind::Down(_) => {
            // Check if click is within table area (accounting for header row)
            if event.row > table_top && event.row < table_top + table_height {
                let row_index = (event.row - table_top - 1) as usize; // -1 for header
                return Some(Action::SelectRow(row_index));
            }
            None
        }
        MouseEventKind::ScrollDown => Some(Action::Down),
        MouseEventKind::ScrollUp => Some(Action::Up),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Quit,
    Up,
    Down,
    First,
    Last,
    Select,
    SelectRow(usize),
    Refresh,
    ToggleAutoRefresh,
    EnterSearch,
    ExitSearch,
    UpdateSearch,
    FilterAll,
    FilterLocal,
    FilterSsh,
    FilterDocker,
    Kill,
    ShowHelp,
    ClosePopup,
    StartForward,
    SubmitForward,
    ShowPresets,
    LaunchPreset,
    QuickForward,
}
