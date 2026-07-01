use super::*;
use crate::tui::state::ConsoleDetailLineCache;

pub(super) fn render(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    if app.console_logs.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::styled("No console logs captured yet.", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("o ", key_style()),
                Span::raw("open browser and attach CDP"),
            ]),
            Line::from(vec![
                Span::styled("e ", key_style()),
                Span::raw("open editor, evaluate JavaScript in the page"),
                Span::raw("  "),
                Span::styled("c ", key_style()),
                Span::raw("clear"),
            ]),
        ])
        .block(panel_block("Console", app.focus == FocusPane::Console))
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    } else {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(8)])
            .split(area);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(root[1]);

        frame.render_widget(
            Paragraph::new(console_summary_lines(app)).style(Style::default().fg(GB_FG)),
            root[0],
        );
        render_console_stream(frame, body[0], app);
        render_console_detail(frame, body[1], app);
    }
}

fn render_console_stream(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let total = app.filtered_console_indices.len();
    let visible_rows = visible_list_rows(area);
    let selected = app
        .console_state
        .selected()
        .unwrap_or(0)
        .min(total.saturating_sub(1));
    let (offset, end) = visible_list_window(selected, visible_rows, total);
    let items = app
        .filtered_console_indices
        .get(offset..end)
        .unwrap_or(&[])
        .iter()
        .filter_map(|index| app.console_logs.get(*index))
        .map(console_stream_item)
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel_block(
            console_stream_title(app),
            app.focus == FocusPane::Console,
        ))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    let mut visible_state = visible_list_state(app.console_state.selected(), offset, total);
    frame.render_stateful_widget(list, area, &mut visible_state);
}

fn render_console_detail(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let selected = app.selected_console_log();

    let lines = selected
        .map(|log| cached_console_detail_lines(app, log))
        .unwrap_or_else(|| vec![Line::styled("No console entry selected.", muted_style())]);
    let title = selected
        .map(console_detail_title)
        .unwrap_or_else(|| "Console Detail".to_string());
    let paragraph = Paragraph::new(lines)
        .block(panel_block(title, app.focus == FocusPane::Console))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn cached_console_detail_lines(app: &WorkbenchState, log: &ConsoleLog) -> Vec<Line<'static>> {
    if let Some(cache) = app.console_detail_line_cache.borrow().as_ref()
        && cache.log_id == log.id
        && cache.message_len == log.message.len()
    {
        return cache.lines.clone();
    }
    let lines = console_detail_lines(log);
    app.console_detail_line_cache
        .replace(Some(ConsoleDetailLineCache {
            log_id: log.id.clone(),
            message_len: log.message.len(),
            lines: lines.clone(),
        }));
    lines
}

fn console_summary_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let stats = &app.console_stats;

    vec![
        Line::from(vec![
            Span::styled("console ", label_style()),
            Span::raw(format!(
                "{}/{} events",
                app.filtered_console_indices.len(),
                app.console_logs.len()
            )),
            Span::styled("  errors ", label_style()),
            Span::styled(
                stats.errors.to_string(),
                if stats.errors == 0 {
                    Style::default().fg(GB_FG)
                } else {
                    console_style(&ConsoleLevel::Error)
                },
            ),
            Span::styled("  warnings ", label_style()),
            Span::styled(
                stats.warnings.to_string(),
                if stats.warnings == 0 {
                    Style::default().fg(GB_FG)
                } else {
                    console_style(&ConsoleLevel::Warning)
                },
            ),
            Span::styled("  evals ", label_style()),
            Span::raw(stats.evals.to_string()),
        ]),
        Line::from(vec![
            Span::styled("e ", key_style()),
            Span::raw("evaluate JS  "),
            Span::styled("c ", key_style()),
            Span::raw("clear visible console  "),
            Span::styled("j/k ", key_style()),
            Span::raw("select  "),
            Span::styled("/", key_style()),
            Span::raw("filter"),
        ]),
    ]
}

fn console_stream_title(app: &WorkbenchState) -> String {
    if app.console_filter.is_empty() {
        "Console Stream".to_string()
    } else {
        format!(
            "Console Stream /{} ({}/{})",
            app.console_filter,
            app.filtered_console_indices.len(),
            app.console_logs.len()
        )
    }
}

fn console_stream_item(log: &ConsoleLog) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("{:<5}", console_level_label(&log.level)),
            console_style(&log.level),
        ),
        Span::raw(" "),
        Span::styled(
            console_stream_message(log),
            console_message_style(&log.level),
        ),
        Span::styled(
            log.source
                .as_deref()
                .map(|source| format!("  {source}"))
                .unwrap_or_default(),
            muted_style(),
        ),
    ]))
}

fn console_stream_message(log: &ConsoleLog) -> String {
    if let Some((expression, _)) = console_eval_parts(log) {
        return format!("eval {}", compact_value(expression, 88));
    }

    compact_value(log.message.lines().next().unwrap_or_default(), 96)
}

fn console_detail_title(log: &ConsoleLog) -> String {
    format!("Console Detail {}", console_level_label(&log.level))
}

fn console_detail_lines(log: &ConsoleLog) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("level ", label_style()),
            Span::styled(console_level_label(&log.level), console_style(&log.level)),
            Span::styled("  source ", label_style()),
            Span::raw(log.source.clone().unwrap_or_else(|| "-".to_string())),
        ]),
        Line::from(vec![
            Span::styled("time ", label_style()),
            Span::raw(log.ts.to_string()),
            Span::styled("  line ", label_style()),
            Span::raw(
                log.line
                    .map(|line| line.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]),
        Line::raw(""),
    ];

    lines.extend(console_log_lines(log));

    if let Some(stack) = &log.stack_json {
        lines.push(Line::raw(""));
        lines.push(Line::styled("stack", label_style()));
        lines.extend(console_stack_lines(stack));
    }

    lines
}

pub(super) fn console_stack_lines(stack: &serde_json::Value) -> Vec<Line<'static>> {
    let Some(frames) = stack_frames(stack) else {
        return syntax_body_lines(
            serde_json::to_string_pretty(stack).unwrap_or_else(|_| stack.to_string()),
        );
    };
    if frames.is_empty() {
        return vec![Line::styled("  no frames", muted_style())];
    }

    frames
        .iter()
        .take(24)
        .enumerate()
        .map(|(index, frame)| {
            let function = json_string_field(frame, "functionName")
                .or_else(|| json_string_field(frame, "function"))
                .unwrap_or("(anonymous)");
            let url = json_string_field(frame, "url")
                .or_else(|| json_string_field(frame, "scriptId"))
                .unwrap_or("-");
            let line = json_i64_field(frame, "lineNumber")
                .or_else(|| json_i64_field(frame, "line"))
                .map(|value| value + 1);
            let column = json_i64_field(frame, "columnNumber")
                .or_else(|| json_i64_field(frame, "column"))
                .map(|value| value + 1);
            Line::from(vec![
                Span::styled(format!("{:>2} ", index + 1), muted_style()),
                Span::styled(compact_value(function, 36), Style::default().fg(GB_AQUA)),
                Span::styled("  at ", muted_style()),
                Span::styled(compact_value(url, 72), Style::default().fg(GB_FG)),
                Span::styled(
                    match (line, column) {
                        (Some(line), Some(column)) => format!(":{line}:{column}"),
                        (Some(line), None) => format!(":{line}"),
                        _ => String::new(),
                    },
                    muted_style(),
                ),
            ])
        })
        .collect()
}

fn stack_frames(stack: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    stack
        .get("callFrames")
        .and_then(serde_json::Value::as_array)
        .or_else(|| {
            stack
                .get("stack")
                .and_then(|value| value.get("callFrames"))
                .and_then(serde_json::Value::as_array)
        })
        .or_else(|| stack.as_array())
}

fn json_string_field<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(serde_json::Value::as_str)
}

fn json_i64_field(value: &serde_json::Value, field: &str) -> Option<i64> {
    value.get(field).and_then(serde_json::Value::as_i64)
}
