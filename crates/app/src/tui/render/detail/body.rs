use super::*;
use crate::tui::render::detail::content::detail_not_loaded_lines;

mod image;
mod sse;

pub(crate) use image::{
    ImageProtocol, image_preview_lines, is_image_request, iterm_favicon_escape,
    kitty_favicon_escape, parse_image_data_url, terminal_image_protocol,
};
#[cfg(test)]
pub(crate) use sse::parse_sse_events;
pub(crate) use sse::{is_sse_request, sse_body_lines};

pub(super) fn response_body_panel_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let Some(request) = app.selected_request() else {
        return empty_state_lines("no request selected", "capture traffic or move with j/k");
    };

    if !request.details_loaded {
        return detail_not_loaded_lines();
    }

    if request.response_body.is_some() {
        if is_image_request(request) {
            return image_preview_lines(request);
        }
        if is_sse_request(request) {
            return sse_body_lines(request);
        }
        if app.focus == FocusPane::Body && !app.body_tree_items().is_empty() {
            return body_tree_lines(app);
        }
        return response_body_content_lines(request, app.focus == FocusPane::Body);
    }

    let mut lines = vec![
        Line::styled("no response body", warning_style()),
        Line::styled("metadata for the selected request", muted_style()),
        Line::raw(""),
        labeled_line("method", request.request.method.clone()),
        labeled_line("url", compact_value(&request.request.url, 140)),
        labeled_line(
            "status",
            request
                .status_code()
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
            "response headers",
            request
                .response
                .as_ref()
                .map(|response| response.response_headers.len().to_string())
                .unwrap_or_else(|| "0".to_string()),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled("hint ", label_style()),
            Span::raw("use h/l for headers, r to replay, p for replay tools, w to save exchange"),
        ]),
    ];

    if request.request_body.is_some() {
        lines.push(Line::from(vec![
            Span::styled("request body ", label_style()),
            Span::raw("captured; open the request-body tab or press e"),
        ]));
    }

    lines
}

pub(super) fn body_tree_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let items = app.body_tree_items();
    if items.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![
        Line::from(vec![
            Span::styled("response body tree", label_style()),
            Span::raw("  "),
            Span::styled("enter/space", key_style()),
            Span::raw(" collapse"),
        ]),
        Line::raw(""),
    ];
    lines.extend(
        items
            .iter()
            .enumerate()
            .map(|(index, item)| body_tree_line(item, index == app.body_tree_selected)),
    );
    lines
}

fn body_tree_line(item: &BodyTreeItem, selected: bool) -> Line<'static> {
    let marker = if item.expandable {
        if item.collapsed { "▸" } else { "▾" }
    } else {
        "·"
    };
    let base = if selected {
        Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(GB_FG)
    };
    let mut spans = vec![
        Span::styled(if selected { "> " } else { "  " }, muted_style()),
        Span::styled("  ".repeat(item.depth.min(10)), muted_style()),
        Span::styled(marker, if selected { base } else { muted_style() }),
        Span::raw(" "),
        Span::styled(item.label.clone(), base),
    ];
    if let Some(value) = &item.value {
        spans.push(Span::styled(": ", muted_style()));
        spans.push(Span::styled(
            value.clone(),
            if selected { base } else { muted_style() },
        ));
    }
    Line::from(spans)
}
pub(super) fn body_lines(title: &'static str, body: String) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(title, label_style()), Line::raw("")];
    lines.extend(syntax_body_lines(body));
    lines
}

pub(super) fn body_preview_lines(title: &'static str, body: String) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(title, label_style()), Line::raw("")];
    lines.extend(
        body.lines()
            .take(80)
            .map(|line| Line::styled(line.to_string(), Style::default().fg(GB_FG))),
    );
    lines
}

pub(super) fn response_body_lines(request: &RequestView, active: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled("response body", label_style()), Line::raw("")];
    lines.extend(response_body_content_lines(request, active));
    lines
}

pub(crate) fn response_body_content_lines(
    request: &RequestView,
    active: bool,
) -> Vec<Line<'static>> {
    let body = render_response_body(request);
    let lines = syntax_body_lines_for_request(request, body);
    if active {
        numbered_lines(lines)
    } else {
        lines.into_iter().take(80).collect()
    }
}

fn render_response_body(request: &RequestView) -> String {
    const MAX_RENDER_BODY_BYTES: usize = 256 * 1024;
    let Some(body) = request.response_body.as_deref() else {
        return formatted_response_body(request);
    };
    if body.len() <= MAX_RENDER_BODY_BYTES {
        return formatted_response_body(request);
    }
    let end = body
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= MAX_RENDER_BODY_BYTES)
        .last()
        .unwrap_or(0);
    let mut preview = body[..end].to_string();
    preview.push_str("\n\n[render preview truncated; copy/open body for full response]");
    preview
}

fn numbered_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let width = lines.len().max(1).to_string().len().max(3);
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let mut spans = vec![
                Span::styled(format!("{:>width$} ", index + 1), muted_style()),
                Span::styled("│ ", Style::default().fg(GB_BG2)),
            ];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}
