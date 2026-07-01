use super::*;
use crate::tui::render::detail::content::detail_not_loaded_lines;
use crate::tui::state::ResponseBodyLineCache;

mod image;
mod sse;

pub(crate) use image::{
    ImageProtocol, image_preview_lines, is_image_request, iterm_favicon_escape,
    kitty_favicon_escape, parse_image_data_url, terminal_image_protocol,
};
#[cfg(test)]
pub(crate) use sse::parse_sse_events;
pub(crate) use sse::{is_sse_request, sse_body_lines};

pub(super) struct ResponseBodyPanel {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) pre_scrolled: bool,
}

impl ResponseBodyPanel {
    fn new(lines: Vec<Line<'static>>) -> Self {
        Self {
            lines,
            pre_scrolled: false,
        }
    }
}

pub(super) fn response_body_panel_lines(
    app: &WorkbenchState,
    visible_rows: usize,
) -> ResponseBodyPanel {
    let Some(request) = app.selected_request() else {
        return ResponseBodyPanel::new(empty_state_lines(
            "no request selected",
            "capture traffic or move with j/k",
        ));
    };

    if !request.details_loaded {
        return ResponseBodyPanel::new(detail_not_loaded_lines());
    }

    if request.response_body.is_some() {
        if is_image_request(request) {
            return ResponseBodyPanel::new(image_preview_lines(request));
        }
        if is_sse_request(request) {
            return ResponseBodyPanel::new(sse_body_lines(request));
        }
        let body_tree_items = app.body_tree_items();
        if app.focus == FocusPane::Body && !body_tree_items.is_empty() {
            return ResponseBodyPanel {
                lines: body_tree_lines(app, visible_rows, &body_tree_items),
                pre_scrolled: true,
            };
        }
        return ResponseBodyPanel::new(cached_response_body_content_lines(
            app,
            request,
            app.focus == FocusPane::Body,
        ));
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

    ResponseBodyPanel::new(lines)
}

pub(super) fn body_tree_lines(
    app: &WorkbenchState,
    visible_rows: usize,
    items: &[BodyTreeItem],
) -> Vec<Line<'static>> {
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
    let item_budget = visible_rows.saturating_sub(lines.len()).max(1);
    lines.extend(
        items
            .iter()
            .skip(app.body_scroll as usize)
            .take(item_budget)
            .enumerate()
            .map(|(visible_index, item)| {
                let index = app.body_scroll as usize + visible_index;
                body_tree_line(item, index == app.body_tree_selected)
            }),
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
    response_body_content_lines_uncached(request, active)
}

fn cached_response_body_content_lines(
    app: &WorkbenchState,
    request: &RequestView,
    active: bool,
) -> Vec<Line<'static>> {
    let response_body_ref = request
        .response
        .as_ref()
        .and_then(|response| response.body_ref.clone());
    let response_body_len = request
        .response_body
        .as_ref()
        .map(String::len)
        .unwrap_or_default();
    if let Some(cache) = app.response_body_line_cache.borrow().as_ref()
        && cache.request_id == request.request.id
        && cache.response_body_ref == response_body_ref
        && cache.response_body_len == response_body_len
        && cache.active == active
    {
        return cache.lines.clone();
    }
    let lines = response_body_content_lines_uncached(request, active);
    app.response_body_line_cache
        .replace(Some(ResponseBodyLineCache {
            request_id: request.request.id.clone(),
            response_body_ref,
            response_body_len,
            active,
            lines: lines.clone(),
        }));
    lines
}

fn response_body_content_lines_uncached(request: &RequestView, active: bool) -> Vec<Line<'static>> {
    let mut body = render_response_body(request);
    if !active {
        body = body.lines().take(80).collect::<Vec<_>>().join("\n");
    }
    let lines = syntax_body_lines_for_request(request, body);
    if active { numbered_lines(lines) } else { lines }
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
