use super::*;

mod history;

use history::{replay_history_line, replay_status_span};

pub(crate) fn replay_lines(
    app: &WorkbenchState,
    request: &RequestView,
    width: u16,
) -> Vec<Line<'static>> {
    if request.replays.is_empty() {
        return vec![
            Line::styled("No replay captured for this request.", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("r ", key_style()),
                Span::raw("run replay  "),
                Span::styled("R ", key_style()),
                Span::raw("edit and replay  "),
                Span::styled("D ", key_style()),
                Span::raw("diff selected"),
            ]),
        ];
    };

    let selected_index = app
        .selected_replay_display_index()
        .unwrap_or_else(|| request.replays.len().saturating_sub(1));
    let Some(replay) = request.replays.get(selected_index) else {
        return Vec::new();
    };
    let latest = request.replays.last();

    if width < 54 {
        return compact_replay_lines(request, replay, selected_index);
    }

    let mut lines = vec![
        Line::from(vec![
            Span::styled("history ", label_style()),
            Span::raw(request.replays.len().to_string()),
            Span::raw("  "),
            Span::styled("selected ", label_style()),
            Span::raw(format!("{}/{}", selected_index + 1, request.replays.len())),
            Span::raw("  "),
            Span::styled("latest ", label_style()),
            latest
                .map(replay_status_span)
                .unwrap_or_else(|| Span::styled("-", muted_style())),
        ]),
        Line::from(vec![
            Span::styled("r ", key_style()),
            Span::raw("run  "),
            Span::styled("R ", key_style()),
            Span::raw("edit  "),
            Span::styled("D ", key_style()),
            Span::raw("diff selected  "),
            Span::styled("j/k ", key_style()),
            Span::raw("select replay  "),
            Span::styled("Y ", key_style()),
            Span::raw("copy body  "),
            Span::styled("p ", key_style()),
            Span::raw("palette"),
        ]),
        Line::raw(""),
        labeled_line("replay id", replay.record.id.clone()),
        labeled_line("source", replay.record.source_request_id.clone()),
        labeled_line("timestamp", replay.record.ts.to_string()),
        labeled_line(
            "status",
            replay
                .record
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "exit",
            replay
                .record
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "output",
            replay
                .record
                .output_path
                .clone()
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "body",
            replay
                .body
                .as_deref()
                .map(|body| format_bytes(body.len() as i64))
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line("command", replay.record.command.clone()),
    ];
    if let Some(error) = replay.record.error.as_deref() {
        lines.push(labeled_line("error", compact_value(error, 120)));
    }
    lines.extend([
        Line::raw(""),
        Line::styled("replay history", label_style()),
        Line::raw(""),
    ]);

    for (index, replay) in request.replays.iter().enumerate().rev().take(10) {
        lines.push(replay_history_line(replay, index == selected_index, false));
    }

    lines.extend([
        Line::raw(""),
        Line::from(vec![
            Span::styled("body ", label_style()),
            Span::raw("hidden in replay view  "),
            Span::styled("Y ", key_style()),
            Span::raw("copy  "),
            Span::styled("w ", key_style()),
            Span::raw("save  "),
            Span::styled("D/d ", key_style()),
            Span::raw("diff"),
        ]),
    ]);

    lines
}

pub(super) fn replay_summary_lines(
    app: &WorkbenchState,
    width: u16,
    height: usize,
) -> Vec<Line<'static>> {
    let Some(request) = app.selected_request() else {
        return empty_state_lines("no request selected", "capture traffic or move with j/k");
    };
    if request.replays.is_empty() {
        return replay_lines(app, request, width);
    }

    let selected_index = app
        .selected_replay_display_index()
        .unwrap_or_else(|| request.replays.len().saturating_sub(1));
    let Some(replay) = request.replays.get(selected_index) else {
        return Vec::new();
    };

    let compact = width < 72 || height < 8;
    let body_len = replay
        .body
        .as_deref()
        .map(|body| format_bytes(body.len() as i64))
        .unwrap_or_else(|| "-".to_string());
    let mut lines = vec![
        Line::from(vec![
            Span::styled("selected ", label_style()),
            Span::raw(format!(
                "{}/{}  ",
                selected_index + 1,
                request.replays.len()
            )),
            replay_status_span(replay),
            Span::raw("  "),
            Span::styled("exit ", label_style()),
            Span::raw(
                replay
                    .record
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Span::raw("  "),
            Span::styled("body ", label_style()),
            Span::raw(body_len),
        ]),
        Line::from(vec![
            Span::styled("j/k ", key_style()),
            Span::raw("select  "),
            Span::styled("D/d ", key_style()),
            Span::raw("diff  "),
            Span::styled("Y ", key_style()),
            Span::raw("copy  "),
            Span::styled("w ", key_style()),
            Span::raw("save"),
        ]),
    ];

    if !compact {
        lines.push(Line::from(vec![
            Span::styled("id ", label_style()),
            Span::raw(compact_value(&replay.record.id, 18)),
            Span::raw("  "),
            Span::styled("ts ", label_style()),
            Span::raw(replay.record.ts.to_string()),
            Span::raw("  "),
            Span::styled("out ", label_style()),
            Span::raw(
                replay
                    .record
                    .output_path
                    .as_deref()
                    .map(|path| compact_value(path, 34))
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]));
    }

    if let Some(error) = replay.record.error.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("error ", warning_style()),
            Span::raw(compact_value(
                error,
                usize::from(width.saturating_sub(8)).max(16),
            )),
        ]));
    }

    let reserved = lines.len().saturating_add(1);
    let history_take = height.saturating_sub(reserved).clamp(1, 8);
    lines.push(Line::styled("history", label_style()));
    for (index, replay) in request.replays.iter().enumerate().rev().take(history_take) {
        lines.push(replay_history_line(replay, index == selected_index, true));
    }
    lines.extend([
        Line::raw(""),
        Line::from(vec![
            Span::styled("body ", label_style()),
            Span::raw("hidden here; use "),
            Span::styled("Y ", key_style()),
            Span::raw("copy, "),
            Span::styled("w ", key_style()),
            Span::raw("save, "),
            Span::styled("D/d ", key_style()),
            Span::raw("diff"),
        ]),
    ]);
    lines
}

fn compact_replay_lines(
    request: &RequestView,
    replay: &ReplayView,
    selected_index: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("replay ", label_style()),
            Span::raw(format!("{}/{} ", selected_index + 1, request.replays.len())),
            replay_status_span(replay),
            Span::raw("  "),
            Span::styled("j/k ", key_style()),
            Span::raw("select"),
        ]),
        Line::from(vec![
            Span::styled("id ", label_style()),
            Span::raw(compact_value(&replay.record.id, 12)),
            Span::raw("  "),
            Span::styled("exit ", label_style()),
            Span::raw(
                replay
                    .record
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Span::raw("  "),
            Span::styled("body ", label_style()),
            Span::raw(
                replay
                    .body
                    .as_deref()
                    .map(|body| format_bytes(body.len() as i64))
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("D ", key_style()),
            Span::raw("diff  "),
            Span::styled("Y ", key_style()),
            Span::raw("copy  "),
            Span::styled("w ", key_style()),
            Span::raw("save"),
        ]),
        Line::raw(""),
        Line::styled("history", label_style()),
    ];

    for (index, replay) in request.replays.iter().enumerate().rev().take(5) {
        lines.push(replay_history_line(replay, index == selected_index, true));
    }

    lines.extend([
        Line::raw(""),
        Line::from(vec![
            Span::styled("body ", label_style()),
            Span::raw("hidden here; "),
            Span::styled("Y ", key_style()),
            Span::raw("copy  "),
            Span::styled("w ", key_style()),
            Span::raw("save  "),
            Span::styled("D/d ", key_style()),
            Span::raw("diff"),
        ]),
    ]);
    lines
}
