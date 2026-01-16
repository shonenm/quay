use crate::app::ForwardInput;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
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
            if let Event::Key(key) = event::read()? {
                return Ok(AppEvent::Key(key));
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
        KeyCode::Char('f') => Some(Action::StartForward),
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

pub fn handle_forward_key(key: KeyEvent, input: &mut ForwardInput) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ClosePopup),
        KeyCode::Enter => Some(Action::SubmitForward),
        KeyCode::Tab | KeyCode::Down => {
            input.active_field = input.active_field.next();
            None
        }
        KeyCode::BackTab | KeyCode::Up => {
            input.active_field = input.active_field.prev();
            None
        }
        KeyCode::Backspace => {
            input.active_value().pop();
            None
        }
        KeyCode::Char(c) => {
            input.active_value().push(c);
            None
        }
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
    Refresh,
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
}
