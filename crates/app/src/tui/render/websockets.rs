use super::*;
use crate::tui::state::WebSocketDetailLineCache;

pub(super) fn render(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    if app.websocket_frames.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::styled("No WebSocket frames captured yet.", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("o ", key_style()),
                Span::raw("open browser and attach CDP"),
            ]),
            Line::from(vec![
                Span::styled("/", key_style()),
                Span::raw("filter frames  "),
                Span::styled("j/k ", key_style()),
                Span::raw("select"),
            ]),
        ])
        .block(panel_block(
            "WebSockets",
            app.focus == FocusPane::WebSockets,
        ))
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
        return;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(root[1]);

    frame.render_widget(
        Paragraph::new(websocket_summary_lines(app)).style(Style::default().fg(GB_FG)),
        root[0],
    );
    render_websocket_stream(frame, body[0], app);
    render_websocket_detail(frame, body[1], app);
}

fn render_websocket_stream(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let total = app.filtered_websocket_indices.len();
    let visible_rows = visible_list_rows(area);
    let selected = app
        .websocket_state
        .selected()
        .unwrap_or(0)
        .min(total.saturating_sub(1));
    let (offset, end) = visible_list_window(selected, visible_rows, total);
    let items = app
        .filtered_websocket_indices
        .get(offset..end)
        .unwrap_or(&[])
        .iter()
        .filter_map(|index| app.websocket_frames.get(*index))
        .map(websocket_stream_item)
        .collect::<Vec<_>>();
    let title = if app.websocket_filter.is_empty() {
        format!(
            "WebSocket Stream {}/{}",
            app.filtered_websocket_indices.len(),
            app.websocket_frames.len()
        )
    } else {
        format!(
            "WebSocket Stream /{} ({}/{})",
            app.websocket_filter,
            app.filtered_websocket_indices.len(),
            app.websocket_frames.len()
        )
    };
    let list = List::new(items)
        .block(panel_block(title, app.focus == FocusPane::WebSockets))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    let mut visible_state = visible_list_state(app.websocket_state.selected(), offset, total);
    frame.render_stateful_widget(list, area, &mut visible_state);
}

fn render_websocket_detail(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let selected = app.selected_websocket_frame();
    let lines = selected
        .map(|frame| cached_websocket_detail_lines(app, frame))
        .unwrap_or_else(|| vec![Line::styled("No frame selected.", muted_style())]);
    let title = selected
        .map(|frame| {
            format!(
                "Frame {} {}",
                direction_label(frame),
                websocket_opcode_label(frame.opcode)
            )
        })
        .unwrap_or_else(|| "Frame Detail".to_string());
    frame.render_widget(
        Paragraph::new(faded_lines(
            lines,
            app.websocket_detail_scroll,
            area,
            &app.config.theme,
            app.config.ui.bottom_fade_rows,
        ))
        .block(panel_block(title, app.focus == FocusPane::WebSockets))
        .scroll((app.websocket_detail_scroll, 0))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn cached_websocket_detail_lines(
    app: &WorkbenchState,
    frame: &WebSocketFrameRecord,
) -> Vec<Line<'static>> {
    if let Some(cache) = app.websocket_detail_line_cache.borrow().as_ref()
        && cache.frame_id == frame.id
        && cache.payload_len == frame.payload.len()
    {
        return cache.lines.clone();
    }
    let lines = websocket_detail_lines(frame);
    app.websocket_detail_line_cache
        .replace(Some(WebSocketDetailLineCache {
            frame_id: frame.id.clone(),
            payload_len: frame.payload.len(),
            lines: lines.clone(),
        }));
    lines
}

fn websocket_summary_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let stats = &app.websocket_stats;

    vec![
        Line::from(vec![
            Span::styled("frames ", label_style()),
            Span::raw(format!(
                "{}/{}",
                app.filtered_websocket_indices.len(),
                app.websocket_frames.len()
            )),
            Span::styled("  conns ", label_style()),
            Span::raw(stats.connections.to_string()),
            Span::styled("  in ", label_style()),
            Span::styled(stats.received.to_string(), Style::default().fg(GB_BLUE)),
            Span::styled("  out ", label_style()),
            Span::styled(stats.sent.to_string(), Style::default().fg(GB_GREEN)),
            Span::styled("  payload ", label_style()),
            Span::raw(format_bytes(stats.bytes as i64)),
        ]),
        Line::from(vec![
            Span::styled("j/k ", key_style()),
            Span::raw("select  "),
            Span::styled("u/d ", key_style()),
            Span::raw("scroll payload  "),
            Span::styled("/", key_style()),
            Span::raw("filter  "),
            Span::styled("g/G ", key_style()),
            Span::raw("top/bottom"),
        ]),
    ]
}

fn websocket_stream_item(frame: &WebSocketFrameRecord) -> ListItem<'static> {
    let direction_style = match frame.direction {
        WebSocketFrameDirection::Sent => Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD),
        WebSocketFrameDirection::Received => {
            Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD)
        }
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{:<3}", direction_label(frame)), direction_style),
        Span::raw(" "),
        Span::styled(
            format!("{:<6}", websocket_opcode_label(frame.opcode)),
            Style::default().fg(GB_YELLOW),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:>7}", format_bytes(frame.payload.len() as i64)),
            muted_style(),
        ),
        Span::raw(" "),
        Span::raw(compact_value(&frame.payload, 80)),
    ]))
}

fn websocket_detail_lines(frame: &WebSocketFrameRecord) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("direction ", label_style()),
            Span::raw(direction_label(frame).to_string()),
            Span::styled("  opcode ", label_style()),
            Span::raw(format!(
                "{} ({})",
                frame.opcode,
                websocket_opcode_label(frame.opcode)
            )),
            Span::styled("  size ", label_style()),
            Span::raw(format_bytes(frame.payload.len() as i64)),
        ]),
        Line::from(vec![
            Span::styled("time ", label_style()),
            Span::raw(frame.ts.to_string()),
            Span::styled("  request ", label_style()),
            Span::raw(frame.browser_request_id.clone()),
            Span::styled("  mask ", label_style()),
            Span::raw(if frame.mask { "yes" } else { "no" }),
        ]),
        Line::raw(""),
        Line::styled("payload", label_style()),
    ];
    lines.extend(format_websocket_payload(frame));
    lines
}

fn format_websocket_payload(frame: &WebSocketFrameRecord) -> Vec<Line<'static>> {
    let payload = frame.payload.clone();
    if websocket_opcode_label(frame.opcode) == "text"
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload)
    {
        return syntax_body_lines(
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| payload.clone()),
        );
    }
    payload
        .lines()
        .map(|line| Line::raw(line.to_string()))
        .collect::<Vec<_>>()
}

fn direction_label(frame: &WebSocketFrameRecord) -> &'static str {
    match frame.direction {
        WebSocketFrameDirection::Sent => "out",
        WebSocketFrameDirection::Received => "in",
    }
}
