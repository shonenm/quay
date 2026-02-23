use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

// Colors
pub const BRAND: Color = Color::Cyan;
pub const ACCENT: Color = Color::Yellow;
pub const SUCCESS: Color = Color::Green;
pub const ERROR: Color = Color::Red;
pub const MUTED: Color = Color::DarkGray;
pub const POPUP_BG: Color = Color::Black;
pub const HIGHLIGHT_BG: Color = Color::Indexed(237);

// Reusable styles
pub fn title() -> Style {
    Style::default().fg(BRAND).add_modifier(Modifier::BOLD)
}

pub fn highlight() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn error() -> Style {
    Style::default().fg(ERROR)
}

pub fn error_bold() -> Style {
    Style::default().fg(ERROR).add_modifier(Modifier::BOLD)
}

pub fn cursor(valid: bool) -> Style {
    let color = if valid { ACCENT } else { ERROR };
    Style::default().fg(color).add_modifier(Modifier::SLOW_BLINK)
}

pub fn row_highlight() -> Style {
    Style::default()
        .bg(HIGHLIGHT_BG)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

// Block builders
pub fn block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
}

pub fn popup_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Line::from(Span::styled(
            format!(" {title} "),
            title_style(),
        )))
        .style(Style::default().bg(POPUP_BG))
}

pub fn plain_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}

fn title_style() -> Style {
    Style::default().fg(BRAND).add_modifier(Modifier::BOLD)
}

// Footer key hint helper
pub fn key_hint<'a>(key: &str, action: &str) -> Vec<Span<'a>> {
    vec![
        Span::styled(key.to_string(), Style::default().fg(BRAND)),
        Span::styled(format!(" {action}  "), muted()),
    ]
}
