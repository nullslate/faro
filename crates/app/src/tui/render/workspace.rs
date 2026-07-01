use super::*;

pub(super) fn render_normal_layout(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &mut WorkbenchState,
) {
    render_normal_content(frame, area, app);
}

fn render_normal_content(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    match app.view {
        WorkbenchView::Network => render_network_view(frame, area, app),
        WorkbenchView::Console => console::render(frame, area, app),
        WorkbenchView::WebSockets => websockets::render(frame, area, app),
        WorkbenchView::Scripts => scripts::render(frame, area, app),
        WorkbenchView::Storage => data::render_storage(frame, area, app),
        WorkbenchView::Cookies => data::render_cookies(frame, area, app),
    }
}

fn render_network_view(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let root = match app.density_mode {
        DensityMode::Compact => Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(12)])
            .split(area),
        DensityMode::Comfortable => Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(if app.active_route_summary().is_some() {
                    4
                } else {
                    3
                }),
                Constraint::Min(12),
            ])
            .split(area),
    };

    let content_area = match app.density_mode {
        DensityMode::Compact => {
            render_network_compact_bar(frame, root[0], app);
            root[1]
        }
        DensityMode::Comfortable => {
            render_network_bar(frame, root[0], app);
            render_stats_panel(frame, root[1], app);
            root[2]
        }
    };

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.requests_percent),
            Constraint::Percentage(100 - app.requests_percent),
        ])
        .split(content_area);

    requests::render(frame, body[0], app);
    if app.detail_tab == DetailTab::Replay {
        detail::render_replay_workspace(frame, body[1], app);
        return;
    }

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.detail_percent),
            Constraint::Percentage(100 - app.detail_percent),
        ])
        .split(body[1]);

    detail::render_detail(frame, right[0], app);
    detail::render_body(frame, right[1], app);
}

pub(super) fn render_focused_layout(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &mut WorkbenchState,
) {
    match app.focus {
        FocusPane::Requests => requests::render(frame, area, app),
        FocusPane::Detail if app.detail_tab == DetailTab::Replay => {
            detail::render_replay_workspace(frame, area, app)
        }
        FocusPane::Detail => detail::render_detail(frame, area, app),
        FocusPane::Body if app.detail_tab == DetailTab::Replay => {
            detail::render_replay_workspace(frame, area, app)
        }
        FocusPane::Body => detail::render_body(frame, area, app),
        FocusPane::Console => console::render(frame, area, app),
        FocusPane::WebSockets => websockets::render(frame, area, app),
        FocusPane::Scripts => scripts::render(frame, area, app),
        FocusPane::Storage => data::render_storage(frame, area, app),
        FocusPane::Cookies => data::render_cookies(frame, area, app),
    }
}

fn render_network_bar(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let mut line = Line::from(vec![
        Span::styled("filter", label_style()),
        Span::raw(" "),
        Span::raw(if app.request_filter.is_empty() {
            "all".to_string()
        } else {
            app.request_filter.clone()
        }),
        Span::styled("   preset ", muted_style()),
        Span::raw(app.active_filter_preset_label().unwrap_or("-")),
        Span::styled("   split ", muted_style()),
        Span::raw(format!(
            "{}:{} / {}:{}",
            app.requests_percent,
            100 - app.requests_percent,
            app.detail_percent,
            100 - app.detail_percent
        )),
        Span::styled("   density ", muted_style()),
        Span::raw(app.density_mode.label()),
        Span::styled("   sql ", muted_style()),
        Span::raw(
            app.sql_request_filter_ids
                .as_ref()
                .map(|ids| ids.len().to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::raw("   "),
    ]);
    line.spans.extend(route_breadcrumb_spans(app, 56));
    line.spans
        .extend([Span::raw("   "), route_summary_span(app, 64)]);
    frame.render_widget(Paragraph::new(line).style(Style::default().fg(GB_FG)), area);
}

fn render_network_compact_bar(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let stats = &app.request_stats;
    let mut traffic_line = status_meter_line(stats);
    let mut spans = vec![
        Span::styled("filter ", label_style()),
        Span::raw(if app.request_filter.is_empty() {
            "all".to_string()
        } else {
            compact_value(&app.request_filter, 36)
        }),
        Span::styled("  preset ", muted_style()),
        Span::raw(app.active_filter_preset_label().unwrap_or("-")),
        Span::styled("  sql ", muted_style()),
        Span::raw(
            app.sql_request_filter_ids
                .as_ref()
                .map(|ids| ids.len().to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::raw("  "),
    ];
    spans.extend(route_breadcrumb_spans(app, 36));
    spans.extend([
        Span::raw("  "),
        route_summary_span(app, 42),
        Span::raw("  "),
    ]);
    spans.append(&mut traffic_line.spans);
    spans.extend([
        Span::raw("  "),
        Span::styled("lat ", muted_style()),
        Span::raw(
            stats
                .avg_duration_ms
                .map(|value| format!("{value}ms"))
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::styled("  slow ", muted_style()),
        Span::raw(stats.slow.to_string()),
        Span::styled("  bytes ", muted_style()),
        Span::raw(format_bytes(stats.total_size)),
        Span::styled("  split ", muted_style()),
        Span::raw(format!(
            "{}:{}",
            app.requests_percent,
            100 - app.requests_percent
        )),
    ]);
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().fg(GB_FG)),
        area,
    );
}

fn render_stats_panel(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let stats = &app.request_stats;
    let mut traffic_line = status_meter_line(stats);
    traffic_line.spans.extend([
        Span::raw("  "),
        Span::styled("pending ", muted_style()),
        Span::raw(stats.pending.to_string()),
        Span::raw("  "),
        Span::styled("replayed ", muted_style()),
        Span::raw(stats.replayed.to_string()),
    ]);
    let lines = vec![
        traffic_line,
        Line::from(vec![
            Span::styled("latency ", muted_style()),
            Span::raw(
                stats
                    .avg_duration_ms
                    .map(|value| format!("avg {value}ms"))
                    .unwrap_or_else(|| "avg -".to_string()),
            ),
            Span::raw("  "),
            Span::raw(
                stats
                    .max_duration_ms
                    .map(|value| format!("max {value}ms"))
                    .unwrap_or_else(|| "max -".to_string()),
            ),
            Span::raw("  "),
            Span::styled("slow ", muted_style()),
            Span::raw(stats.slow.to_string()),
            Span::raw("  "),
            Span::styled("bytes ", muted_style()),
            Span::raw(format_bytes(stats.total_size)),
        ]),
    ];
    let lines = if app.active_route_summary().is_some() {
        let mut lines = lines;
        lines.push(Line::from(vec![route_summary_span(app, 120)]));
        lines
    } else {
        lines
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block("Signal", false))
            .style(Style::default().fg(GB_FG)),
        area,
    );
}

fn status_meter_line(stats: &RequestStats) -> Line<'static> {
    let total = stats.ok + stats.redirect + stats.client + stats.server + stats.pending;
    if total == 0 {
        return Line::from(vec![
            Span::styled("traffic ", muted_style()),
            Span::styled("no traffic", Style::default().fg(GB_BG2)),
        ]);
    }

    let mut spans = vec![Span::styled("traffic ", muted_style())];
    for (index, (label, count, color)) in [
        ("2xx", stats.ok, GB_GREEN),
        ("3xx", stats.redirect, GB_BLUE),
        ("4xx", stats.client, GB_YELLOW),
        ("5xx", stats.server, GB_RED),
        ("...", stats.pending, GB_MUTED),
    ]
    .into_iter()
    .filter(|(_, count, _)| *count > 0)
    .enumerate()
    {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(format!("{label} "), muted_style()));
        spans.extend(segment_bar(count, total, 8, color));
    }
    Line::from(spans)
}

fn segment_bar(count: usize, total: usize, width: usize, color: Color) -> Vec<Span<'static>> {
    let filled = (count * width).div_ceil(total);
    vec![
        Span::styled("━".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "─".repeat(width.saturating_sub(filled)),
            Style::default().fg(GB_BG2),
        ),
    ]
}
