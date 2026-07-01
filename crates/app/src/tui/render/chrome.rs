use super::*;

pub(super) fn render_header(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let header_bg = Color::Rgb(29, 32, 33);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let mut title_spans = brand_spans(header_bg, app.config.theme.accent);
    title_spans.push(Span::raw("  "));
    title_spans.extend(favicon_spans(app));
    title_spans.extend(if area.width < 90 {
        compact_header_spans(app)
    } else {
        full_header_spans(app)
    });
    let title = Line::from(title_spans);
    let block = Paragraph::new(title).style(Style::default().bg(header_bg));
    frame.render_widget(block, rows[0]);
    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(header_bg)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(view_tabs_line(app)).style(Style::default().fg(GB_FG).bg(header_bg)),
        rows[2],
    );
}

fn brand_spans(header_bg: Color, accent: Color) -> Vec<Span<'static>> {
    vec![
        Span::styled("", Style::default().fg(accent).bg(header_bg)),
        Span::styled(
            "faro",
            Style::default()
                .fg(header_bg)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("", Style::default().fg(accent).bg(header_bg)),
    ]
}

pub(super) fn view_tabs_line(app: &WorkbenchState) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, tab) in [
        (
            "1",
            format!("Net {}", app.requests.len()),
            app.view == WorkbenchView::Network,
        ),
        (
            "2",
            format!("Console {}", console_error_badge(app)),
            app.view == WorkbenchView::Console,
        ),
        (
            "3",
            format!("WS {}", app.websocket_frames.len()),
            app.view == WorkbenchView::WebSockets,
        ),
        (
            "4",
            format!("Scripts {}", app.scripts.len()),
            app.view == WorkbenchView::Scripts,
        ),
        (
            "5",
            format!("Storage {}", app.storage_events.len()),
            app.view == WorkbenchView::Storage,
        ),
        (
            "6",
            format!("Cookies {}", cookie_count(app)),
            app.view == WorkbenchView::Cookies,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.extend(view_tab_spans(tab.0, tab.1, tab.2, &app.config.theme));
    }
    Line::from(spans)
}

fn view_tab_spans(
    key: &'static str,
    label: String,
    active: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let header_bg = Color::Rgb(29, 32, 33);
    if active {
        vec![
            Span::styled("", Style::default().fg(theme.accent).bg(header_bg)),
            Span::styled(
                format!("{key} "),
                Style::default()
                    .fg(header_bg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                label,
                Style::default()
                    .fg(theme.text)
                    .bg(GB_BG2)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("", Style::default().fg(GB_BG2).bg(header_bg)),
        ]
    } else {
        vec![Span::styled(
            format!(" {key} {label} "),
            Style::default().fg(theme.muted).bg(header_bg),
        )]
    }
}

fn console_error_badge(app: &WorkbenchState) -> String {
    let errors = console_error_count(app);
    if errors == 0 {
        app.console_logs.len().to_string()
    } else {
        format!("{errors}!")
    }
}

fn console_error_count(app: &WorkbenchState) -> usize {
    app.console_logs
        .iter()
        .filter(|log| matches!(log.level, ConsoleLevel::Error | ConsoleLevel::Fatal))
        .count()
}

fn compact_header_spans(app: &WorkbenchState) -> Vec<Span<'static>> {
    vec![
        Span::styled(site_domain(app), Style::default().fg(GB_FG)),
        Span::styled(format!(" · {}", transient_status(app)), muted_style()),
    ]
}

fn full_header_spans(app: &WorkbenchState) -> Vec<Span<'static>> {
    vec![
        Span::styled(site_domain(app), Style::default().fg(GB_FG)),
        Span::raw("  "),
        Span::styled(transient_status(app), Style::default().fg(GB_BLUE)),
    ]
}

fn transient_status(app: &WorkbenchState) -> String {
    if app.status_updated_at.elapsed() > std::time::Duration::from_secs(5) {
        if app.cdp_websocket_url.is_some() {
            "live".to_string()
        } else {
            "idle".to_string()
        }
    } else if app.status.is_empty() {
        "idle".to_string()
    } else {
        compact_value(&app.status, 80)
    }
}

fn site_domain(app: &WorkbenchState) -> String {
    app.selected_request()
        .map(|request| domain_for_url(&request.request.url))
        .unwrap_or_else(|| domain_for_url(&app.target_url))
}

fn favicon_spans(app: &WorkbenchState) -> Vec<Span<'static>> {
    let Some((mime, data)) = captured_favicon(app) else {
        return vec![Span::styled("[icon]", muted_style()), Span::raw("  ")];
    };
    match terminal_image_protocol() {
        Some(ImageProtocol::Kitty) => vec![Span::raw(kitty_favicon_escape(data)), Span::raw(" ")],
        Some(ImageProtocol::ITerm) => vec![Span::raw(iterm_favicon_escape(data)), Span::raw(" ")],
        None => vec![
            Span::styled("[favicon]", Style::default().fg(GB_GREEN)),
            Span::raw(format!(" {mime} ")),
        ],
    }
}

fn captured_favicon(app: &WorkbenchState) -> Option<(&str, &str)> {
    app.requests.iter().find_map(|request| {
        let mime = request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref())?;
        if !mime.starts_with("image/") {
            return None;
        }
        let path = path_for_url(&request.request.url).to_lowercase();
        if !(path.contains("favicon")
            || path.contains("apple-touch-icon")
            || path.ends_with(".ico"))
        {
            return None;
        }
        let body = request.response_body.as_deref()?;
        parse_image_data_url(body)
    })
}
pub(super) fn render_status(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let keys = match app.input_mode {
        InputMode::Filtering => filter_help_line(),
        InputMode::BodySearch => body_search_help_line(app),
        _ => compact_help_line(app),
    };
    let mut status_spans = vec![
        Span::styled("status ", label_style()),
        Span::raw(transient_status(app)),
    ];

    if app.view == WorkbenchView::Network {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("sort ", label_style()),
            Span::raw(format!(
                "{}{}",
                app.sort_mode.label(),
                if app.sort_descending { " desc" } else { " asc" }
            )),
        ]);
    }

    let active_filters = active_filter_count(app);
    if active_filters > 0 {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("filters ", label_style()),
            Span::raw(active_filters.to_string()),
        ]);
    }
    if app.input_mode != InputMode::Normal {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("mode ", label_style()),
            Span::raw(app.input_mode.label()),
        ]);
    }
    if app.layout_mode != LayoutMode::Normal {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("layout ", label_style()),
            Span::raw(app.layout_mode.label()),
        ]);
    }

    let status = Line::from(status_spans);
    frame.render_widget(
        Paragraph::new(vec![keys, status]).style(Style::default().fg(GB_FG)),
        area,
    );
}

fn active_filter_count(app: &WorkbenchState) -> usize {
    usize::from(!app.request_filter.is_empty())
        + usize::from(!app.console_filter.is_empty())
        + usize::from(!app.websocket_filter.is_empty())
        + usize::from(app.sql_request_filter_ids.is_some())
        + usize::from(app.active_request_route_breadcrumb().is_some())
}

fn filter_help_line() -> Line<'static> {
    Line::from(vec![
        Span::styled("type", key_style()),
        Span::raw(" live filter (substring or regex)  "),
        Span::styled("enter", key_style()),
        Span::raw(" done  "),
        Span::styled("esc", key_style()),
        Span::raw(" done  "),
        Span::styled("backspace", key_style()),
        Span::raw(" delete"),
    ])
}

fn body_search_help_line(app: &WorkbenchState) -> Line<'static> {
    Line::from(vec![
        Span::styled("body search ", label_style()),
        Span::raw(if app.body_search_query.is_empty() {
            "type to find in response body".to_string()
        } else {
            app.body_search_query.clone()
        }),
        Span::raw("  "),
        Span::styled("enter", key_style()),
        Span::raw(" done  "),
        Span::styled("esc", key_style()),
        Span::raw(" close  "),
        Span::styled("backspace", key_style()),
        Span::raw(" delete"),
    ])
}

fn compact_help_line(app: &WorkbenchState) -> Line<'static> {
    // Show the keys that matter for the pane the user is actually in.
    match app.focus {
        FocusPane::Console => key_hints(&[
            ("e", "eval"),
            ("c", "clear"),
            ("j/k", "select"),
            ("u/d", "scroll"),
            ("/", "filter"),
            ("?", "keys"),
        ]),
        FocusPane::WebSockets => key_hints(&[
            ("j/k", "select"),
            ("u/d", "payload"),
            ("g/G", "ends"),
            ("/", "filter"),
            ("?", "keys"),
        ]),
        FocusPane::Scripts => key_hints(&[
            ("n", "new"),
            ("e", "edit"),
            ("r", "run"),
            ("D", "dup"),
            ("x", "delete"),
            ("?", "keys"),
        ]),
        FocusPane::Storage | FocusPane::Cookies => key_hints(&[
            ("e", "edit"),
            ("x", "delete"),
            ("tab", "origin"),
            ("j/k", "select"),
            ("/", "filter"),
            ("?", "keys"),
        ]),
        FocusPane::Detail | FocusPane::Body => key_hints(&[
            ("h/l", "tabs"),
            ("u/d", "scroll"),
            ("g/G", "ends"),
            ("e", "editor"),
            ("Y", "copy"),
            ("/", "find"),
            ("y", "curl"),
            ("?", "keys"),
        ]),
        FocusPane::Requests => key_hints(&[
            ("j/k", "move"),
            ("enter", "route"),
            ("backspace", "up"),
            ("h/l", "tabs"),
            ("/", "filter"),
            ("r", "replay"),
            ("S", "sessions"),
            ("p", "palette"),
            ("?", "keys"),
        ]),
    }
}

fn key_hints(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(hints.len() * 3);
    for (index, (key, label)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(key.to_string(), key_style()));
        spans.push(Span::raw(format!(" {label}")));
    }
    Line::from(spans)
}

fn cookie_count(app: &WorkbenchState) -> usize {
    app.cookie_events.len()
        + app
            .cookie_snapshots
            .last()
            .map(|snapshot| snapshot.cookies.len())
            .unwrap_or(0)
}
