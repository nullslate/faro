use super::*;

pub(super) fn console_level_label(level: &ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Trace => "trace",
        ConsoleLevel::Debug => "debug",
        ConsoleLevel::Info => "info",
        ConsoleLevel::Warning => "warn",
        ConsoleLevel::Error => "error",
        ConsoleLevel::Fatal => "fatal",
    }
}

pub(super) fn console_style(level: &ConsoleLevel) -> Style {
    match level {
        ConsoleLevel::Warning => Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD),
        ConsoleLevel::Error | ConsoleLevel::Fatal => {
            Style::default().fg(GB_RED).add_modifier(Modifier::BOLD)
        }
        ConsoleLevel::Debug | ConsoleLevel::Trace => muted_style(),
        ConsoleLevel::Info => Style::default().fg(GB_GREEN),
    }
}

pub(super) fn console_log_lines(log: &ConsoleLog) -> Vec<Line<'static>> {
    if let Some(lines) = console_eval_lines(log) {
        return lines;
    }

    let mut message_lines = log.message.lines();
    let first_message = message_lines.next().unwrap_or_default();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{:<7}", console_level_label(&log.level)),
            console_style(&log.level),
        ),
        Span::raw(" "),
        Span::styled(first_message.to_string(), console_message_style(&log.level)),
        Span::styled(
            log.source
                .as_deref()
                .map(|source| format!("  {source}"))
                .unwrap_or_default(),
            muted_style(),
        ),
    ])];

    for line in message_lines {
        lines.push(Line::from(vec![
            Span::styled("        ", muted_style()),
            Span::styled(line.to_string(), console_message_style(&log.level)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::styled("(empty)", muted_style()));
    }

    lines
}

fn console_eval_lines(log: &ConsoleLog) -> Option<Vec<Line<'static>>> {
    let (expression, result) = console_eval_parts(log)?;

    let mut lines = vec![Line::from(vec![
        Span::styled("eval   ", console_style(&log.level)),
        Span::styled(" faro-console", muted_style()),
    ])];

    for (index, line) in expression.lines().enumerate() {
        let prompt = if index == 0 { "> " } else { "| " };
        lines.push(prefixed_line(
            Span::styled(prompt, key_style()),
            highlight_javascript_line(line),
        ));
    }

    if result.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("< ", muted_style()),
            Span::styled("undefined", muted_style()),
        ]));
        return Some(lines);
    }

    for line in console_result_lines(result, &log.level) {
        lines.push(prefixed_line(Span::styled("< ", muted_style()), line));
    }

    Some(lines)
}

pub(super) fn console_eval_parts(log: &ConsoleLog) -> Option<(&str, &str)> {
    if log.source.as_deref() != Some("faro-console") || !log.message.starts_with("> ") {
        return None;
    }

    Some(
        log.message
            .split_once('\n')
            .map(|(expression, result)| (expression.trim_start_matches("> "), result))
            .unwrap_or_else(|| (log.message.trim_start_matches("> "), "")),
    )
}

fn console_result_lines(result: &str, level: &ConsoleLevel) -> Vec<Line<'static>> {
    if serde_json::from_str::<serde_json::Value>(result).is_ok() {
        return syntax_body_lines(result.to_string());
    }

    result
        .lines()
        .map(|line| {
            if matches!(level, ConsoleLevel::Error | ConsoleLevel::Fatal)
                || line.starts_with("Error:")
                || line.starts_with("TypeError:")
                || line.starts_with("ReferenceError:")
                || line.starts_with("SyntaxError:")
            {
                Line::styled(line.to_string(), console_style(level))
            } else {
                highlight_javascript_line(line)
            }
        })
        .collect()
}

fn prefixed_line(prefix: Span<'static>, mut line: Line<'static>) -> Line<'static> {
    let mut spans = vec![prefix];
    spans.append(&mut line.spans);
    Line::from(spans)
}

pub(super) fn console_message_style(level: &ConsoleLevel) -> Style {
    match level {
        ConsoleLevel::Error | ConsoleLevel::Fatal => Style::default().fg(GB_RED),
        ConsoleLevel::Warning => Style::default().fg(GB_YELLOW),
        ConsoleLevel::Debug | ConsoleLevel::Trace => muted_style(),
        ConsoleLevel::Info => Style::default().fg(GB_FG),
    }
}
