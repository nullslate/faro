use super::{
    body_lines, body_preview_lines, image_preview_lines, is_image_request, is_sse_request,
    replay_lines, response_body_lines, sse_body_lines,
};
use crate::tui::render::{
    compact_value, domain_for_url, format_bytes, formatted_request_body, label_style, muted_style,
    path_for_url, query_params_for_url, resource_label, warning_style,
};
use crate::tui::state::{DetailTab, FocusPane, RequestView, WorkbenchState};
use ratatui::text::{Line, Span};

pub(super) fn detail_lines(app: &WorkbenchState, width: u16) -> Vec<Line<'static>> {
    let Some(request) = app.selected_request() else {
        return empty_state_lines("no request selected", "capture traffic or move with j/k");
    };

    match app.detail_tab {
        DetailTab::Overview => overview_lines(request),
        DetailTab::RequestHeaders => {
            header_lines("request headers", &request.request.request_headers)
        }
        DetailTab::RequestBody if !request.details_loaded => detail_not_loaded_lines(),
        DetailTab::RequestBody if app.focus == FocusPane::Detail => {
            body_lines("request body", formatted_request_body(request))
        }
        DetailTab::RequestBody => {
            body_preview_lines("request body", formatted_request_body(request))
        }
        DetailTab::ResponseHeaders => match request.response.as_ref() {
            Some(response) => header_lines("response headers", &response.response_headers),
            None => empty_state_lines("no response captured", "refresh while capture is active"),
        },
        DetailTab::ResponseBody if !request.details_loaded => detail_not_loaded_lines(),
        DetailTab::ResponseBody if is_image_request(request) => image_preview_lines(request),
        DetailTab::ResponseBody if is_sse_request(request) => sse_body_lines(request),
        DetailTab::ResponseBody => response_body_lines(request, app.focus == FocusPane::Detail),
        DetailTab::Timing => timing_lines(request),
        DetailTab::Replay if !request.details_loaded => detail_not_loaded_lines(),
        DetailTab::Replay => replay_lines(app, request, width),
    }
}

pub(crate) fn detail_not_loaded_lines() -> Vec<Line<'static>> {
    empty_state_lines(
        "detail not loaded",
        "focus this pane to load body and replay details",
    )
}

fn overview_lines(request: &RequestView) -> Vec<Line<'static>> {
    let mut lines = vec![
        labeled_line("method", request.request.method.clone()),
        labeled_line("url", compact_value(&request.request.url, 140)),
        labeled_line("state", format!("{:?}", request.request.status)),
        labeled_line("domain", domain_for_url(&request.request.url)),
        labeled_line("path", path_for_url(&request.request.url)),
        labeled_line(
            "status",
            request
                .response
                .as_ref()
                .and_then(|response| response.status_code)
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "mime",
            request
                .response
                .as_ref()
                .and_then(|response| response.mime_type.clone())
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "resource",
            request
                .request
                .resource_type
                .as_deref()
                .map(resource_label)
                .map(str::to_string)
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "body",
            request
                .response
                .as_ref()
                .and_then(|response| response.body_size)
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "duration",
            request
                .duration_ms()
                .map(|duration| format!("{duration}ms"))
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line("replays", request.replays.len().to_string()),
    ];

    if let Some(replay) = request.replays.last() {
        lines.push(labeled_line(
            "last replay",
            format!(
                "status {} exit {}",
                replay
                    .record
                    .status_code
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                replay
                    .record
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
        ));
        if let Some(path) = &replay.record.output_path {
            lines.push(labeled_line("output", path.clone()));
        }
    }

    let query_params = query_params_for_url(&request.request.url);
    if !query_params.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("query params", label_style()));
        for (key, value) in query_params {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key}: "), label_style()),
                Span::raw(value),
            ]));
        }
    }

    if let Some(response) = request.response.as_ref()
        && response.body_truncated
    {
        lines.push(Line::styled("response body was truncated", warning_style()));
    }

    lines
}

fn timing_lines(request: &RequestView) -> Vec<Line<'static>> {
    vec![
        labeled_line("started", request.request.started_at.to_string()),
        labeled_line(
            "completed",
            request
                .request
                .completed_at
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "duration",
            request
                .duration_ms()
                .map(|duration| format!("{duration}ms"))
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "received",
            request
                .response
                .as_ref()
                .map(|response| response.received_at.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "body size",
            request
                .response
                .as_ref()
                .and_then(|response| response.body_size)
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string()),
        ),
        labeled_line(
            "truncated",
            request
                .response
                .as_ref()
                .map(|response| response.body_truncated.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
    ]
}

fn header_lines(title: &'static str, headers: &[faro_core::Header]) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(title, label_style()), Line::raw("")];
    if headers.is_empty() {
        lines.push(Line::raw("(none)"));
        return lines;
    }

    for header in headers {
        lines.push(Line::from(vec![
            Span::styled(format!("{}: ", header.name), label_style()),
            Span::raw(header.value.clone()),
        ]));
    }
    lines
}

pub(crate) fn labeled_line(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), label_style()),
        Span::raw(value),
    ])
}

pub(crate) fn empty_state_lines(title: &'static str, hint: &'static str) -> Vec<Line<'static>> {
    vec![
        Line::styled(title, label_style()),
        Line::styled(hint, muted_style()),
    ]
}
