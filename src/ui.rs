use crate::app::{App, ConnectionField, ConnectionPopupMode, Filter, ForwardField, InputMode, Popup};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
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

    draw_header(frame, app, chunks[0]);
    draw_filter_bar(frame, app, chunks[1]);
    draw_table(frame, app, chunks[2]);
    draw_footer(frame, app, chunks[3]);

    // Draw popup if active
    match app.popup {
        Popup::Details => draw_details_popup(frame, app),
        Popup::Help => draw_help_popup(frame, app),
        Popup::Forward => draw_forward_popup(frame, app),
        Popup::Presets => draw_presets_popup(frame, app),
        Popup::Connections => draw_connections_popup(frame, app),
        Popup::None => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let content = if app.has_multiple_connections() {
        let conn_name = app
            .active_connection()
            .map_or("Unknown", |c| c.name.as_str());
        let index = app.active_connection + 1;
        let total = app.connections.len();

        let mut spans = vec![
            Span::styled(
                "Quay  ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("\u{25c0} ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                conn_name,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" \u{25b6}", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("  [{index}/{total}]"),
                Style::default().fg(Color::DarkGray),
            ),
        ];

        // Show remote/docker info
        match (&app.remote_host, &app.docker_target) {
            (Some(host), Some(target)) => {
                spans.push(Span::styled(
                    format!("  [remote: {host}] [docker: {target}]"),
                    Style::default().fg(Color::Cyan),
                ));
            }
            (Some(host), None) => {
                spans.push(Span::styled(
                    format!("  [remote: {host}]"),
                    Style::default().fg(Color::Cyan),
                ));
            }
            (None, Some(target)) => {
                spans.push(Span::styled(
                    format!("  [docker: {target}]"),
                    Style::default().fg(Color::Cyan),
                ));
            }
            (None, None) => {}
        }

        Line::from(spans)
    } else {
        let title_text = match (&app.remote_host, &app.docker_target) {
            (Some(host), Some(target)) => {
                format!("Quay [remote: {host}] [docker: {target}]")
            }
            (None, Some(target)) => format!("Quay [docker: {target}]"),
            (Some(host), None) => format!("Quay [remote: {host}]"),
            (None, None) => "Quay - Port Manager".to_string(),
        };
        Line::from(Span::styled(
            title_text,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
    };

    let title = Paragraph::new(content).block(Block::default().borders(Borders::ALL));
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
                Span::styled(
                    "_",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
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

    let paragraph =
        Paragraph::new(Line::from(content)).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_table(frame: &mut Frame, app: &App, area: Rect) {
    let header_cells = ["TYPE", "LOCAL", "REMOTE", "PROCESS/CONTAINER"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .filtered_entries
        .iter()
        .map(|entry| {
            let (indicator, color) = if entry.is_open {
                ("●", Color::Green)
            } else {
                ("○", Color::DarkGray)
            };
            let local_cell = Line::from(vec![
                Span::styled(indicator, Style::default().fg(color)),
                Span::raw(format!(" :{}", entry.local_port)),
            ]);
            Row::new(vec![
                Cell::from(entry.source.to_string()),
                Cell::from(local_cell),
                Cell::from(entry.remote_display()),
                Cell::from(entry.process_display()),
            ])
        })
        .collect();

    let total = app.filtered_entries.len();
    let current = if total > 0 { app.selected + 1 } else { 0 };
    let title = format!("Ports ({current}/{total})");

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(20),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title))
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
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
        let switch_hint = if app.has_multiple_connections() {
            "[h/l] Switch  "
        } else {
            ""
        };
        let help_text = match app.input_mode {
            InputMode::Search => "[Enter/Esc] Done  [Backspace] Delete".to_string(),
            InputMode::Normal => {
                if app.is_remote() || app.is_docker_target() {
                    format!("{switch_hint}[j/k] Navigate  [Enter] Details  [F] Quick Forward  [f] Forward  [K] Kill  [?] Help  [q] Quit")
                } else {
                    format!("{switch_hint}[j/k] Navigate  [Enter] Details  [K] Kill  [f] Forward  [p] Presets  [?] Help  [q] Quit")
                }
            }
        };
        Line::from(Span::styled(
            help_text,
            Style::default().fg(Color::DarkGray),
        ))
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

    let Some(entry) = app.selected_entry() else {
        return;
    };

    let (open_text, open_color) = if entry.is_open {
        ("Yes", Color::Green)
    } else {
        ("No", Color::DarkGray)
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
            Span::styled("Open: ", Style::default().fg(Color::Yellow)),
            Span::styled(open_text, Style::default().fg(open_color)),
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
            Span::raw(entry.pid.map_or_else(|| "-".to_string(), |p| p.to_string())),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Close"),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Details")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}

fn draw_help_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, frame.area());
    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/↓     Move down"),
        Line::from("  k/↑     Move up"),
        Line::from("  g/Home  Go to first"),
        Line::from("  G/End   Go to last"),
        Line::from(""),
        Line::from(Span::styled(
            "Filtering",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  /       Search mode"),
        Line::from("  0       Show all"),
        Line::from("  1       Local only"),
        Line::from("  2       SSH only"),
        Line::from("  3       Docker only"),
        Line::from(""),
        Line::from(Span::styled(
            "Actions",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Enter   Show details"),
        Line::from("  K       Kill process"),
        Line::from("  f       New SSH forward"),
    ];

    if app.is_remote() || app.is_docker_target() {
        lines.push(Line::from("  F       Quick forward (same port)"));
    }

    if app.is_docker_target() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Docker Target",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from("  Container ports discovered via ss"));
        if let Some(ref ip) = app.container_ip {
            lines.push(Line::from(format!("  Container IP: {ip}")));
        }
        lines.push(Line::from("  F tunnels through SSH to container"));
    }

    lines.extend([
        Line::from("  p       Show presets"),
        Line::from("  r       Refresh"),
        Line::from("  a       Toggle auto-refresh"),
        Line::from("  q/Esc   Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Connections",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  h       Previous connection"),
        Line::from("  l       Next connection"),
        Line::from("  c       Connection manager"),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Close"),
        ]),
    ]);

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Help")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}

#[allow(clippy::too_many_lines)]
fn draw_forward_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);

    let input = &app.forward_input;
    let active = input.active_field;

    let field_valid = |field: ForwardField| -> bool {
        match field {
            ForwardField::LocalPort => input.is_local_port_valid(),
            ForwardField::RemoteHost => input.is_remote_host_valid(),
            ForwardField::RemotePort => input.is_remote_port_valid(),
            ForwardField::SshHost => input.is_ssh_host_valid(),
        }
    };

    let is_remote = app.is_remote();
    let is_docker_target = app.is_docker_target();

    let field_style = |field: ForwardField| {
        // In remote mode, SSH Host is locked/dimmed
        if is_remote && field == ForwardField::SshHost {
            return Style::default().fg(Color::DarkGray);
        }
        // In docker target mode, Remote Host is also locked (container IP)
        if is_docker_target && field == ForwardField::RemoteHost {
            return Style::default().fg(Color::DarkGray);
        }
        let valid = field_valid(field);
        if field == active {
            if valid {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            }
        } else if valid {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Red)
        }
    };

    let cursor = |field: ForwardField| {
        if field == active {
            let color = if field_valid(field) {
                Color::Yellow
            } else {
                Color::Red
            };
            Span::styled(
                "_",
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::SLOW_BLINK),
            )
        } else {
            Span::raw("")
        }
    };

    let footer = if input.is_valid() {
        Line::from(Span::styled(
            "Tab/↑↓: Switch field  Enter: Create  Esc: Cancel",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        let invalid = input.invalid_field_names();
        let fix_text = format!("Fix: {}  Tab/↑↓: Switch  Esc: Cancel", invalid.join(", "));
        Line::from(Span::styled(fix_text, Style::default().fg(Color::Red)))
    };

    let lines = vec![
        Line::from(Span::styled(
            "Create SSH Port Forward",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Local Port:  ", field_style(ForwardField::LocalPort)),
            Span::styled(
                input.local_port.as_str(),
                field_style(ForwardField::LocalPort),
            ),
            cursor(ForwardField::LocalPort),
        ]),
        Line::from(if is_docker_target {
            vec![
                Span::styled("Remote Host: ", field_style(ForwardField::RemoteHost)),
                Span::styled(
                    input.remote_host.as_str(),
                    field_style(ForwardField::RemoteHost),
                ),
                Span::styled(" (container IP)", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("Remote Host: ", field_style(ForwardField::RemoteHost)),
                Span::styled(
                    input.remote_host.as_str(),
                    field_style(ForwardField::RemoteHost),
                ),
                cursor(ForwardField::RemoteHost),
            ]
        }),
        Line::from(vec![
            Span::styled("Remote Port: ", field_style(ForwardField::RemotePort)),
            Span::styled(
                input.remote_port.as_str(),
                field_style(ForwardField::RemotePort),
            ),
            cursor(ForwardField::RemotePort),
        ]),
        Line::from(if is_remote {
            vec![
                Span::styled("SSH Host:    ", field_style(ForwardField::SshHost)),
                Span::styled(input.ssh_host.as_str(), field_style(ForwardField::SshHost)),
                Span::styled(" (locked)", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("SSH Host:    ", field_style(ForwardField::SshHost)),
                Span::styled(input.ssh_host.as_str(), field_style(ForwardField::SshHost)),
                cursor(ForwardField::SshHost),
            ]
        }),
        Line::from(""),
        footer,
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("New Forward")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}

#[allow(clippy::too_many_lines)]
fn draw_connections_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    if app.connection_popup_mode == ConnectionPopupMode::AddNew {
        draw_connection_add_form(frame, app, area);
        return;
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "Connections",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, conn) in app.connections.iter().enumerate() {
        let is_selected = i == app.connection_selected;
        let is_active = i == app.active_connection;
        let prefix = if is_selected { "> " } else { "  " };
        let active_marker = if is_active { " *" } else { "" };

        let style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(
            format!("{prefix}{}{active_marker}", conn.name),
            style,
        )));

        // Show remote/docker details
        let mut details = Vec::new();
        if let Some(ref host) = conn.remote_host {
            details.push(format!("remote: {host}"));
        }
        if let Some(ref target) = conn.docker_target {
            details.push(format!("docker: {target}"));
        }
        if !details.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("    {}", details.join("  ")),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[j/k] Navigate  [Enter] Switch  [a] Add  [d] Delete  [Esc] Close",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Connections")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}

fn draw_connection_add_form(frame: &mut Frame, app: &App, area: Rect) {
    let input = &app.connection_input;
    let active = input.active_field;

    let field_style = |field: ConnectionField| {
        if field == active {
            if field == ConnectionField::Name && !input.is_name_valid() {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            }
        } else if field == ConnectionField::Name && !input.is_name_valid() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::White)
        }
    };

    let cursor = |field: ConnectionField| {
        if field == active {
            let color = if field == ConnectionField::Name && !input.is_name_valid() {
                Color::Red
            } else {
                Color::Yellow
            };
            Span::styled(
                "_",
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::SLOW_BLINK),
            )
        } else {
            Span::raw("")
        }
    };

    let footer = if input.is_valid() {
        Line::from(Span::styled(
            "[Tab] Next field  [Enter] Save  [Esc] Cancel",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            "Name is required  [Tab] Next field  [Esc] Cancel",
            Style::default().fg(Color::Red),
        ))
    };

    let lines = vec![
        Line::from(Span::styled(
            "New Connection",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Name:           ", field_style(ConnectionField::Name)),
            Span::styled(input.name.as_str(), field_style(ConnectionField::Name)),
            cursor(ConnectionField::Name),
        ]),
        Line::from(vec![
            Span::styled(
                "Remote Host:    ",
                field_style(ConnectionField::RemoteHost),
            ),
            Span::styled(
                input.remote_host.as_str(),
                field_style(ConnectionField::RemoteHost),
            ),
            cursor(ConnectionField::RemoteHost),
        ]),
        Line::from(vec![
            Span::styled(
                "Docker Target:  ",
                field_style(ConnectionField::DockerTarget),
            ),
            Span::styled(
                input.docker_target.as_str(),
                field_style(ConnectionField::DockerTarget),
            ),
            cursor(ConnectionField::DockerTarget),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "(Remote Host / Docker Target are optional)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        footer,
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("New Connection")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}

#[allow(clippy::too_many_lines)]
fn draw_presets_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    if app.presets.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "No Presets",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Create presets in:"),
            Line::from(Span::styled(
                "~/.config/quay/presets.toml",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from("Example:"),
            Line::from(Span::styled(
                "[[preset]]",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "name = \"My Server\"",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "local_port = 8080",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "remote_host = \"localhost\"",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "remote_port = 80",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "ssh_host = \"myserver\"",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
                Span::raw("Close"),
            ]),
        ];

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Presets")
                .style(Style::default().bg(Color::Black)),
        );
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "SSH Forward Presets",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, preset) in app.presets.iter().enumerate() {
        let is_selected = i == app.preset_selected;
        let prefix = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let key_str = preset
            .key
            .as_ref()
            .map(|k| format!("[{k}] "))
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("{}{}{}", prefix, key_str, preset.name),
            style,
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "    {}:{} -> {}:{}",
                preset.local_port, preset.ssh_host, preset.remote_host, preset.remote_port
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k: Navigate  Enter: Launch  Esc: Cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Presets")
            .style(Style::default().bg(Color::Black)),
    );
    frame.render_widget(paragraph, area);
}
