use crate::tui::render::{
    compact_value, format_bytes, key_style, label_style, muted_style, warning_style,
};
use crate::tui::state::ReplayView;
use ratatui::text::{Line, Span};

pub(super) fn replay_history_line(
    replay: &ReplayView,
    selected: bool,
    compact: bool,
) -> Line<'static> {
    let body = replay
        .body
        .as_deref()
        .map(|body| format!(" body={}", format_bytes(body.len() as i64)))
        .unwrap_or_default();
    let marker = if selected { ">" } else { " " };
    let id_style = if selected {
        label_style()
    } else {
        muted_style()
    };
    if compact {
        return Line::from(vec![
            Span::styled(marker, key_style()),
            Span::raw(" "),
            replay_status_span(replay),
            Span::raw(" "),
            Span::styled(compact_value(&replay.record.id, 10), id_style),
            Span::styled(body, muted_style()),
        ]);
    }

    Line::from(vec![
        Span::styled(marker, key_style()),
        Span::raw(" "),
        Span::styled(compact_value(&replay.record.id, 12), id_style),
        Span::raw("  "),
        replay_status_span(replay),
        Span::raw("  exit="),
        Span::raw(
            replay
                .record
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::raw("  ts="),
        Span::styled(replay.record.ts.to_string(), muted_style()),
        Span::styled(body, muted_style()),
    ])
}

pub(super) fn replay_status_span(replay: &ReplayView) -> Span<'static> {
    let status = replay
        .record
        .status_code
        .map(|status| status.to_string())
        .unwrap_or_else(|| "-".to_string());
    let style = match replay.record.status_code {
        Some(200..=399) => label_style(),
        Some(400..=599) => warning_style(),
        _ => muted_style(),
    };
    Span::styled(status, style)
}
