use crate::tui::render::{
    GB_GREEN, compact_value, label_style, muted_style, syntax_body_lines, warning_style,
};
use crate::tui::state::RequestView;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub(crate) fn is_sse_request(request: &RequestView) -> bool {
    request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .map(|mime| mime.contains("event-stream"))
        .unwrap_or(false)
        || request
            .response_body
            .as_deref()
            .map(|body| {
                body.lines().any(|line| {
                    line.starts_with("data:")
                        || line.starts_with("event:")
                        || line.starts_with("id:")
                        || line.starts_with("retry:")
                })
            })
            .unwrap_or(false)
}

pub(crate) fn sse_body_lines(request: &RequestView) -> Vec<Line<'static>> {
    let Some(body) = request.response_body.as_deref() else {
        return vec![Line::styled("no SSE body captured", warning_style())];
    };
    let events = parse_sse_events(body);
    let mut lines = vec![
        Line::styled("server-sent events", label_style()),
        Line::from(vec![
            Span::styled("events ", label_style()),
            Span::raw(events.len().to_string()),
            Span::styled("  mime ", label_style()),
            Span::raw(
                request
                    .response
                    .as_ref()
                    .and_then(|response| response.mime_type.clone())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]),
        Line::raw(""),
    ];

    if events.is_empty() {
        lines.push(Line::styled("No complete SSE events found.", muted_style()));
        lines.extend(syntax_body_lines(body.to_string()).into_iter().take(40));
        return lines;
    }

    for (index, event) in events.iter().enumerate().take(40) {
        lines.push(Line::from(vec![
            Span::styled(format!("#{} ", index + 1), muted_style()),
            Span::styled(
                event.event.clone().unwrap_or_else(|| "message".to_string()),
                Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(
                event
                    .id
                    .as_ref()
                    .map(|id| format!("  id={id}"))
                    .unwrap_or_default(),
            ),
            Span::raw(
                event
                    .retry
                    .as_ref()
                    .map(|retry| format!("  retry={retry}"))
                    .unwrap_or_default(),
            ),
        ]));
        for data in &event.data {
            lines.push(Line::from(vec![
                Span::styled("data ", label_style()),
                Span::raw(compact_value(data, 160)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    if events.len() > 40 {
        lines.push(Line::styled(
            format!("... {} more events", events.len() - 40),
            muted_style(),
        ));
    }
    lines
}

#[derive(Debug, Default)]
pub(crate) struct SseEvent {
    pub(crate) event: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) retry: Option<String>,
    pub(crate) data: Vec<String>,
}

pub(crate) fn parse_sse_events(body: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut current = SseEvent::default();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if current.event.is_some()
                || current.id.is_some()
                || current.retry.is_some()
                || !current.data.is_empty()
            {
                events.push(current);
                current = SseEvent::default();
            }
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line
            .split_once(':')
            .map(|(field, value)| (field, value.trim_start()))
            .unwrap_or((line, ""));
        match field {
            "event" => current.event = Some(value.to_string()),
            "id" => current.id = Some(value.to_string()),
            "retry" => current.retry = Some(value.to_string()),
            "data" => current.data.push(value.to_string()),
            _ => {}
        }
    }
    if current.event.is_some()
        || current.id.is_some()
        || current.retry.is_some()
        || !current.data.is_empty()
    {
        events.push(current);
    }
    events
}
