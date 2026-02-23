use crate::app::{
    App, ConnectionField, ConnectionPopupMode, Filter, ForwardField, InputMode, Popup,
};
use crate::theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, Clear, Paragraph, Row, Table, TableState},
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
            Span::styled("\u{2693} Quay  ", theme::title()),
            Span::styled("\u{25c0} ", theme::muted()),
            Span::styled(conn_name, theme::highlight()),
            Span::styled(" \u{25b6}", theme::muted()),
            Span::styled(format!("  [{index}/{total}]"), theme::muted()),
        ];

        // Show remote/docker info
        match (&app.remote_host, &app.docker_target) {
            (Some(host), Some(target)) => {
                spans.push(Span::styled(
                    format!("  [remote: {host}] [docker: {target}]"),
                    Style::default().fg(theme::BRAND),
                ));
            }
            (Some(host), None) => {
                spans.push(Span::styled(
                    format!("  [remote: {host}]"),
                    Style::default().fg(theme::BRAND),
                ));
            }
            (None, Some(target)) => {
                spans.push(Span::styled(
                    format!("  [docker: {target}]"),
                    Style::default().fg(theme::BRAND),
                ));
            }
            (None, None) => {}
        }

        Line::from(spans)
    } else {
        let title_text = match (&app.remote_host, &app.docker_target) {
            (Some(host), Some(target)) => {
                format!("\u{2693} Quay [remote: {host}] [docker: {target}]")
            }
            (None, Some(target)) => format!("\u{2693} Quay [docker: {target}]"),
            (Some(host), None) => format!("\u{2693} Quay [remote: {host}]"),
            (None, None) => "\u{2693} Quay - Port Manager".to_string(),
        };
        Line::from(Span::styled(title_text, theme::title()))
    };

    let title = Paragraph::new(content).block(theme::plain_block());
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
        Span::styled(" [A] Auto", theme::success())
    } else {
        Span::styled(" [a] auto", theme::muted())
    };

    let content = match app.input_mode {
        InputMode::Search => {
            vec![
                Span::raw("Search: "),
                Span::styled(&app.search_query, Style::default().fg(theme::ACCENT)),
                Span::styled("_", theme::cursor(true)),
            ]
        }
        InputMode::Normal => {
            let mut spans = vec![
                Span::raw("Filter: "),
                Span::styled(filter_text, theme::success()),
                auto_refresh_indicator,
            ];
            if !app.search_query.is_empty() {
                spans.push(Span::styled(
                    format!("  Search: \"{}\"", app.search_query),
                    Style::default().fg(theme::ACCENT),
                ));
            }
            spans.push(Span::raw("  [/] search  [?] help"));
            spans
        }
    };

    let paragraph = Paragraph::new(Line::from(content)).block(theme::plain_block());
    frame.render_widget(paragraph, area);
}

fn draw_empty_state(frame: &mut Frame, app: &App, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(r"   __ _ _   _  __ _ _   _ ", theme::title())),
        Line::from(Span::styled(r"  / _` | | | |/ _` | | | |", theme::title())),
        Line::from(Span::styled(r" | (_| | |_| | (_| | |_| |", theme::title())),
        Line::from(Span::styled(r"  \__, |\__,_|\__,_|\__, |", theme::title())),
        Line::from(Span::styled(r"     |_|             |_| ", theme::title())),
        Line::from(""),
        Line::from(Span::styled(format!("v{version}"), theme::muted())),
        Line::from(""),
    ];

    let hints = if app.loading {
        const SPINNER: &[&str] = &["|", "/", "-", "\\"];
        let frame = SPINNER[app.tick_count as usize % SPINNER.len()];
        vec![Line::from(vec![
            Span::styled(format!("{frame} "), Style::default().fg(theme::BRAND)),
            Span::styled("Loading...", Style::default().fg(Color::White)),
        ])]
    } else if app.search_query.is_empty() {
        match app.filter {
            Filter::All => vec![
                Line::from(Span::styled(
                    "No ports found",
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::styled("[r] Refresh  [?] Help", theme::muted())),
            ],
            Filter::Local => vec![
                Line::from(Span::styled(
                    "No Local ports found",
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::styled("[0] Show all  [r] Refresh", theme::muted())),
            ],
            Filter::Ssh => vec![
                Line::from(Span::styled(
                    "No SSH ports found",
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::styled("[0] Show all  [r] Refresh", theme::muted())),
            ],
            Filter::Docker => vec![
                Line::from(Span::styled(
                    "No Docker ports found",
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::styled("[0] Show all  [r] Refresh", theme::muted())),
            ],
        }
    } else {
        vec![
            Line::from(Span::styled(
                format!("No results for \"{}\"", app.search_query),
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled("[Esc] Clear search", theme::muted())),
        ]
    };

    lines.extend(hints);

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(theme::block("Ports (0/0)"));
    frame.render_widget(paragraph, area);
}

fn draw_table(frame: &mut Frame, app: &App, area: Rect) {
    if app.filtered_entries.is_empty() {
        draw_empty_state(frame, app, area);
        return;
    }

    let header_cells = ["TYPE", "LOCAL", "REMOTE", "PROCESS/CONTAINER"]
        .iter()
        .map(|h| Cell::from(*h).style(theme::highlight()));
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .filtered_entries
        .iter()
        .map(|entry| {
            let (indicator, color) = if app.docker_target.is_some() {
                if entry.is_open {
                    ("\u{25cf}", theme::SUCCESS)
                } else {
                    ("\u{25cf}", theme::ACCENT)
                }
            } else if entry.is_open {
                ("\u{25cf}", theme::SUCCESS)
            } else {
                ("\u{25cb}", theme::MUTED)
            };
            let local_cell = if let Some(fwd) = entry.forwarded_port {
                Line::from(vec![
                    Span::styled(indicator, Style::default().fg(color)),
                    Span::raw(format!(" :{}", entry.local_port)),
                    Span::styled(format!("\u{2192}:{fwd}"), Style::default().fg(theme::BRAND)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(indicator, Style::default().fg(color)),
                    Span::raw(format!(" :{}", entry.local_port)),
                ])
            };
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
            Constraint::Length(16),
            Constraint::Length(20),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(theme::block(&title))
    .row_highlight_style(theme::row_highlight())
    .highlight_symbol("> ");

    let mut state = TableState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    // Show status message if present, otherwise show help text
    let content = if let Some((ref message, _)) = app.status_message {
        Line::from(Span::styled(message, Style::default().fg(theme::ACCENT)))
    } else {
        match app.input_mode {
            InputMode::Search => {
                let mut spans = Vec::new();
                spans.extend(theme::key_hint("Enter/Esc", "Done"));
                spans.extend(theme::key_hint("Backspace", "Delete"));
                Line::from(spans)
            }
            InputMode::Normal => {
                let mut spans = Vec::new();
                if app.has_multiple_connections() {
                    spans.extend(theme::key_hint("h/l", "Switch"));
                }
                spans.extend(theme::key_hint("j/k", "Navigate"));
                spans.extend(theme::key_hint("Enter", "Details"));
                if app.is_remote() || app.is_docker_target() {
                    spans.extend(theme::key_hint("F", "Quick Forward"));
                }
                spans.extend(theme::key_hint("f", "Forward"));
                if !app.is_remote() && !app.is_docker_target() {
                    spans.extend(theme::key_hint("p", "Presets"));
                }
                spans.extend(theme::key_hint("K", "Kill"));
                spans.extend(theme::key_hint("?", "Help"));
                spans.extend(theme::key_hint("q", "Quit"));
                Line::from(spans)
            }
        }
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

    let is_docker_target = app.docker_target.is_some();

    let (open_text, open_color) = if is_docker_target || entry.is_open {
        ("Yes", theme::SUCCESS)
    } else {
        ("No", theme::MUTED)
    };

    let (accessible_text, accessible_color) = if entry.is_open {
        ("Yes", theme::SUCCESS)
    } else {
        ("No", theme::ACCENT)
    };

    let label = Style::default().fg(theme::ACCENT);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Type: ", label),
            Span::raw(entry.source.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Local Port: ", label),
            Span::raw(format!("{}", entry.local_port)),
        ]),
        Line::from(vec![
            Span::styled("Open: ", label),
            Span::styled(open_text, Style::default().fg(open_color)),
        ]),
    ];
    if is_docker_target {
        lines.push(Line::from(vec![
            Span::styled("Accessible: ", label),
            Span::styled(accessible_text, Style::default().fg(accessible_color)),
        ]));
        if let Some(fwd) = entry.forwarded_port {
            lines.push(Line::from(vec![
                Span::styled("Forwarded: ", label),
                Span::styled(
                    format!("\u{2192} :{fwd}"),
                    Style::default().fg(theme::BRAND),
                ),
            ]));
        }
    }
    lines.extend([
        Line::from(vec![
            Span::styled("Remote: ", label),
            Span::raw(entry.remote_display()),
        ]),
        Line::from(vec![
            Span::styled("Process: ", label),
            Span::raw(&entry.process_name),
        ]),
        Line::from(vec![
            Span::styled("PID: ", label),
            Span::raw(entry.pid.map_or_else(|| "-".to_string(), |p| p.to_string())),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc] ", theme::muted()),
            Span::raw("Close"),
        ]),
    ]);

    let paragraph = Paragraph::new(lines).block(theme::popup_block("Details"));
    frame.render_widget(paragraph, area);
}

fn draw_help_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 70, frame.area());
    frame.render_widget(Clear, area);

    let help_key = |key: &str, desc: &str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {key:<10}"), Style::default().fg(theme::BRAND)),
            Span::raw(desc.to_string()),
        ])
    };

    let mut lines = vec![
        Line::from(Span::styled("Navigation", theme::highlight())),
        help_key("j/\u{2193}", "Move down"),
        help_key("k/\u{2191}", "Move up"),
        help_key("g/Home", "Go to first"),
        help_key("G/End", "Go to last"),
        Line::from(""),
        Line::from(Span::styled("Filtering", theme::highlight())),
        help_key("/", "Search mode"),
        help_key("0", "Show all"),
        help_key("1", "Local only"),
        help_key("2", "SSH only"),
        help_key("3", "Docker only"),
        Line::from(""),
        Line::from(Span::styled("Actions", theme::highlight())),
        help_key("Enter", "Show details"),
        help_key("K", "Kill process"),
        help_key("f", "New SSH forward"),
    ];

    if app.is_remote() || app.is_docker_target() {
        lines.push(help_key("F", "Quick forward (same port)"));
    }

    if app.is_docker_target() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Docker Target",
            theme::highlight(),
        )));
        lines.push(Line::from("  Container ports discovered via ss"));
        if let Some(ref ip) = app.container_ip {
            lines.push(Line::from(format!("  Container IP: {ip}")));
        }
        lines.push(Line::from("  F tunnels through SSH to container"));
    }

    lines.extend([
        help_key("p", "Show presets"),
        help_key("r", "Refresh"),
        help_key("a", "Toggle auto-refresh"),
        help_key("q/Esc", "Quit"),
        Line::from(""),
        Line::from(Span::styled("Connections", theme::highlight())),
        help_key("h", "Previous connection"),
        help_key("l", "Next connection"),
        help_key("c", "Connection manager"),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Esc] ", theme::muted()),
            Span::raw("Close"),
        ]),
    ]);

    let paragraph = Paragraph::new(lines).block(theme::popup_block("Help"));
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
        if is_remote && field == ForwardField::SshHost {
            return theme::muted();
        }
        if is_docker_target && field == ForwardField::RemoteHost {
            return theme::muted();
        }
        let valid = field_valid(field);
        if field == active {
            if valid {
                theme::highlight()
            } else {
                theme::error_bold()
            }
        } else if valid {
            Style::default().fg(Color::White)
        } else {
            theme::error()
        }
    };

    let cursor = |field: ForwardField| {
        if field == active {
            Span::styled("_", theme::cursor(field_valid(field)))
        } else {
            Span::raw("")
        }
    };

    let footer = if input.is_valid() {
        Line::from(Span::styled(
            "Tab/\u{2191}\u{2193}: Switch field  Enter: Create  Esc: Cancel",
            theme::muted(),
        ))
    } else {
        let invalid = input.invalid_field_names();
        let fix_text = format!(
            "Fix: {}  Tab/\u{2191}\u{2193}: Switch  Esc: Cancel",
            invalid.join(", ")
        );
        Line::from(Span::styled(fix_text, theme::error()))
    };

    let lines = vec![
        Line::from(Span::styled("Create SSH Port Forward", theme::title())),
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
                Span::styled(" (container IP)", theme::muted()),
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
                Span::styled(" (locked)", theme::muted()),
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

    let paragraph = Paragraph::new(lines).block(theme::popup_block("New Forward"));
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
        Line::from(Span::styled("Connections", theme::title())),
        Line::from(""),
    ];

    for (i, conn) in app.connections.iter().enumerate() {
        let is_selected = i == app.connection_selected;
        let is_active = i == app.active_connection;
        let prefix = if is_selected { "> " } else { "  " };
        let active_marker = if is_active { " *" } else { "" };

        let style = if is_selected {
            theme::highlight()
        } else {
            Style::default()
        };

        lines.push(Line::from(Span::styled(
            format!("{prefix}{}{active_marker}", conn.name),
            style,
        )));

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
                theme::muted(),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[j/k] Navigate  [Enter] Switch  [a] Add  [d] Delete  [Esc] Close",
        theme::muted(),
    )));

    let paragraph = Paragraph::new(lines).block(theme::popup_block("Connections"));
    frame.render_widget(paragraph, area);
}

fn draw_connection_add_form(frame: &mut Frame, app: &App, area: Rect) {
    let input = &app.connection_input;
    let active = input.active_field;

    let field_style = |field: ConnectionField| {
        if field == active {
            if field == ConnectionField::Name && !input.is_name_valid() {
                theme::error_bold()
            } else {
                theme::highlight()
            }
        } else if field == ConnectionField::Name && !input.is_name_valid() {
            theme::error()
        } else {
            Style::default().fg(Color::White)
        }
    };

    let cursor = |field: ConnectionField| {
        if field == active {
            let valid = field != ConnectionField::Name || input.is_name_valid();
            Span::styled("_", theme::cursor(valid))
        } else {
            Span::raw("")
        }
    };

    let footer = if input.is_valid() {
        Line::from(Span::styled(
            "[Tab] Next field  [Enter] Save  [Esc] Cancel",
            theme::muted(),
        ))
    } else {
        Line::from(Span::styled(
            "Name is required  [Tab] Next field  [Esc] Cancel",
            theme::error(),
        ))
    };

    let lines = vec![
        Line::from(Span::styled("New Connection", theme::title())),
        Line::from(""),
        Line::from(vec![
            Span::styled("Name:           ", field_style(ConnectionField::Name)),
            Span::styled(input.name.as_str(), field_style(ConnectionField::Name)),
            cursor(ConnectionField::Name),
        ]),
        Line::from(vec![
            Span::styled("Remote Host:    ", field_style(ConnectionField::RemoteHost)),
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
            theme::muted(),
        )),
        Line::from(""),
        footer,
    ];

    let paragraph = Paragraph::new(lines).block(theme::popup_block("New Connection"));
    frame.render_widget(paragraph, area);
}

#[allow(clippy::too_many_lines)]
fn draw_presets_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    if app.presets.is_empty() {
        let lines = vec![
            Line::from(Span::styled("No Presets", theme::highlight())),
            Line::from(""),
            Line::from("Create presets in:"),
            Line::from(Span::styled(
                "~/.config/quay/presets.toml",
                Style::default().fg(theme::BRAND),
            )),
            Line::from(""),
            Line::from("Example:"),
            Line::from(Span::styled("[[preset]]", theme::muted())),
            Line::from(Span::styled("name = \"My Server\"", theme::muted())),
            Line::from(Span::styled("local_port = 8080", theme::muted())),
            Line::from(Span::styled("remote_host = \"localhost\"", theme::muted())),
            Line::from(Span::styled("remote_port = 80", theme::muted())),
            Line::from(Span::styled("ssh_host = \"myserver\"", theme::muted())),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Esc] ", theme::muted()),
                Span::raw("Close"),
            ]),
        ];

        let paragraph = Paragraph::new(lines).block(theme::popup_block("Presets"));
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines = vec![
        Line::from(Span::styled("SSH Forward Presets", theme::title())),
        Line::from(""),
    ];

    for (i, preset) in app.presets.iter().enumerate() {
        let is_selected = i == app.preset_selected;
        let prefix = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            theme::highlight()
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
            theme::muted(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k: Navigate  Enter: Launch  Esc: Cancel",
        theme::muted(),
    )));

    let paragraph = Paragraph::new(lines).block(theme::popup_block("Presets"));
    frame.render_widget(paragraph, area);
}
