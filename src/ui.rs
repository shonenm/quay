use crate::app::{App, Filter, ForwardField, InputMode, Popup};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Filter/Search
            Constraint::Min(5),    // Table
            Constraint::Length(2), // Footer
        ])
        .split(frame.area());

    draw_header(frame, chunks[0]);
    draw_filter_bar(frame, app, chunks[1]);
    draw_table(frame, app, chunks[2]);
    draw_footer(frame, app, chunks[3]);

    // Draw popup if active
    match app.popup {
        Popup::Details => draw_details_popup(frame, app),
        Popup::Help => draw_help_popup(frame),
        Popup::Forward => draw_forward_popup(frame, app),
        Popup::None => {}
    }
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new("Quay - Port Manager")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, area);
}

fn draw_filter_bar(frame: &mut Frame, app: &App, area: Rect) {
    let filter_text = match app.filter {
        Filter::All => "[0] All",
        Filter::Local => "[1] Local",
        Filter::Ssh => "[2] SSH",
        Filter::Docker => "[3] Docker",
    };

    let auto_refresh_indicator = if app.auto_refresh {
        Span::styled(" [A] Auto", Style::default().fg(Color::Green))
    } else {
        Span::styled(" [a] auto", Style::default().fg(Color::DarkGray))
    };

    let content = match app.input_mode {
        InputMode::Search => {
            vec![
                Span::raw("Search: "),
                Span::styled(&app.search_query, Style::default().fg(Color::Yellow)),
                Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
            ]
        }
        InputMode::Normal => {
            vec![
                Span::raw("Filter: "),
                Span::styled(filter_text, Style::default().fg(Color::Green)),
                auto_refresh_indicator,
                Span::raw("  [/] search  [?] help"),
            ]
        }
    };

    let paragraph = Paragraph::new(Line::from(content))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_table(frame: &mut Frame, app: &App, area: Rect) {
    let header_cells = ["TYPE", "LOCAL", "REMOTE", "PROCESS/CONTAINER"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .filtered_entries
        .iter()
        .map(|entry| {
            Row::new(vec![
                Cell::from(entry.source.to_string()),
                Cell::from(format!(":{}", entry.local_port)),
                Cell::from(entry.remote_display()),
                Cell::from(entry.process_display()),
            ])
        })
        .collect();

    let total = app.filtered_entries.len();
    let current = if total > 0 { app.selected + 1 } else { 0 };
    let title = format!("Ports ({}/{})", current, total);

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(20),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title))
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
    .highlight_symbol("> ");

    let mut state = TableState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    // Show status message if present, otherwise show help text
    let content = if let Some((ref message, _)) = app.status_message {
        Line::from(Span::styled(message, Style::default().fg(Color::Yellow)))
    } else {
        let help_text = match app.input_mode {
            InputMode::Search => "[Enter/Esc] Done  [Backspace] Delete",
            InputMode::Normal => "[j/k] Navigate  [Enter] Details  [K] Kill  [f] Forward  [?] Help  [q] Quit",
        };
        Line::from(Span::styled(help_text, Style::default().fg(Color::DarkGray)))
    };

    let paragraph = Paragraph::new(content);
    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_details_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);

    let entry = match app.selected_entry() {
        Some(e) => e,
        None => return,
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Type: ", Style::default().fg(Color::Yellow)),
            Span::raw(entry.source.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Local Port: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", entry.local_port)),
        ]),
        Line::from(vec![
            Span::styled("Remote: ", Style::default().fg(Color::Yellow)),
            Span::raw(entry.remote_display()),
        ]),
        Line::from(vec![
            Span::styled("Process: ", Style::default().fg(Color::Yellow)),
            Span::raw(&entry.process_name),
        ]),
        Line::from(vec![
            Span::styled("PID: ", Style::default().fg(Color::Yellow)),
            Span::raw(entry.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string())),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Close"),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Details")
                .style(Style::default().bg(Color::Black)),
        );
    frame.render_widget(paragraph, area);
}

fn draw_help_popup(frame: &mut Frame) {
    let area = centered_rect(50, 70, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled("Navigation", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from("  j/↓     Move down"),
        Line::from("  k/↑     Move up"),
        Line::from("  g/Home  Go to first"),
        Line::from("  G/End   Go to last"),
        Line::from(""),
        Line::from(Span::styled("Filtering", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from("  /       Search mode"),
        Line::from("  0       Show all"),
        Line::from("  1       Local only"),
        Line::from("  2       SSH only"),
        Line::from("  3       Docker only"),
        Line::from(""),
        Line::from(Span::styled("Actions", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from("  Enter   Show details"),
        Line::from("  K       Kill process"),
        Line::from("  f       New SSH forward"),
        Line::from("  r       Refresh"),
        Line::from("  a       Toggle auto-refresh"),
        Line::from("  q/Esc   Quit"),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Close"),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .style(Style::default().bg(Color::Black)),
        );
    frame.render_widget(paragraph, area);
}

fn draw_forward_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);

    let input = &app.forward_input;
    let active = input.active_field;

    let field_style = |field: ForwardField| {
        if field == active {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        }
    };

    let cursor = |field: ForwardField| {
        if field == active {
            Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK))
        } else {
            Span::raw("")
        }
    };

    let lines = vec![
        Line::from(Span::styled(
            "Create SSH Port Forward",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Local Port:  ", field_style(ForwardField::LocalPort)),
            Span::raw(&input.local_port),
            cursor(ForwardField::LocalPort),
        ]),
        Line::from(vec![
            Span::styled("Remote Host: ", field_style(ForwardField::RemoteHost)),
            Span::raw(&input.remote_host),
            cursor(ForwardField::RemoteHost),
        ]),
        Line::from(vec![
            Span::styled("Remote Port: ", field_style(ForwardField::RemotePort)),
            Span::raw(&input.remote_port),
            cursor(ForwardField::RemotePort),
        ]),
        Line::from(vec![
            Span::styled("SSH Host:    ", field_style(ForwardField::SshHost)),
            Span::raw(&input.ssh_host),
            cursor(ForwardField::SshHost),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Tab/↑↓: Switch field  Enter: Create  Esc: Cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("New Forward")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}
