use super::*;

pub(super) fn requests_title(app: &WorkbenchState) -> String {
    let sql_filter = app
        .sql_request_filter_ids
        .as_ref()
        .map(|ids| format!(" sql:{}", ids.len()))
        .unwrap_or_default();
    let route = app
        .active_request_route_breadcrumb()
        .map(|route| format!(" route:{}", compact_value(&route, 32)))
        .unwrap_or_default();
    if app.request_filter.is_empty() {
        format!(" Requests{sql_filter}{route} ")
    } else {
        format!(
            " Requests{sql_filter}{route} /{} ({}/{}) ",
            app.request_filter,
            app.filtered_request_indices.len(),
            app.requests.len()
        )
    }
}

pub(super) fn route_summary_span(app: &WorkbenchState, max_width: usize) -> Span<'static> {
    let Some(summary) = app.active_route_summary() else {
        return Span::raw("");
    };
    Span::styled(
        compact_value(
            &format!(
                "route_stats {} req · {} err · {} slow · {} pending · max {} · {}",
                summary.count,
                summary.errors,
                summary.slow,
                summary.pending,
                summary
                    .max_duration_ms
                    .map(|duration| format!("{duration}ms"))
                    .unwrap_or_else(|| "-".to_string()),
                format_bytes(summary.total_size)
            ),
            max_width,
        ),
        muted_style(),
    )
}

pub(super) fn route_breadcrumb_spans(app: &WorkbenchState, max_width: usize) -> Vec<Span<'static>> {
    let Some(route) = app.active_request_route_breadcrumb() else {
        return vec![Span::styled("route ", muted_style()), Span::raw("-")];
    };
    let route = compact_value(&route, max_width);
    let mut spans = vec![Span::styled("route ", muted_style())];
    for (index, segment) in route
        .split(" / ")
        .filter(|segment| !segment.is_empty())
        .enumerate()
    {
        if index > 0 {
            spans.push(Span::styled(" › ", muted_style()));
        }
        spans.push(Span::styled(
            format!(" {segment} "),
            Style::default()
                .fg(GB_GREEN)
                .bg(GB_BG2)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans
}

pub(super) fn filter_highlight_terms(filter: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for token in filter.split_whitespace() {
        let value = token
            .split_once(':')
            .map(|(_, value)| value)
            .unwrap_or(token)
            .trim()
            .to_lowercase();
        if value.is_empty()
            || value == "-"
            || value.ends_with("xx")
            || matches!(
                token.split_once(':').map(|(key, _)| key),
                Some("status" | "has" | "body" | "reqbody" | "resbody" | "header")
            )
        {
            continue;
        }
        if !terms.contains(&value) {
            terms.push(value);
        }
    }
    terms
}

pub(super) fn highlight_text(value: &str, terms: &[String]) -> Line<'static> {
    if terms.is_empty() {
        return Line::raw(value.to_string());
    }

    let lower = value.to_lowercase();
    let mut matches = terms
        .iter()
        .filter_map(|term| lower.find(term).map(|start| (start, start + term.len())))
        .collect::<Vec<_>>();
    matches.sort_by_key(|(start, _)| *start);

    let Some((start, end)) = matches.first().copied() else {
        return Line::raw(value.to_string());
    };

    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::raw(value[..start].to_string()));
    }
    spans.push(Span::styled(
        value[start..end].to_string(),
        Style::default()
            .fg(Color::Black)
            .bg(GB_YELLOW)
            .add_modifier(Modifier::BOLD),
    ));
    if end < value.len() {
        spans.push(Span::raw(value[end..].to_string()));
    }

    Line::from(spans)
}

#[derive(Clone, Copy)]
pub(super) enum RowFade {
    Full,
    Soft,
    Dim,
    Ghost,
}

impl RowFade {
    pub(super) fn base_style(self, theme: &Theme) -> Style {
        self.fg(theme.text)
    }

    pub(super) fn secondary_style(self, theme: &Theme) -> Style {
        self.fg(theme.muted)
    }

    pub(super) fn accent_style(self, color: Color) -> Style {
        self.fg(color)
    }

    fn fg(self, color: Color) -> Style {
        Style::default().fg(dim_color(color, self.factor()))
    }

    fn factor(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Soft => 0.62,
            Self::Dim => 0.38,
            Self::Ghost => 0.2,
        }
    }
}

pub(super) fn bottom_overlay_fade(
    row_index: usize,
    offset: usize,
    visible_rows: usize,
    has_more_below: bool,
    fade_rows: usize,
) -> RowFade {
    if fade_rows == 0 || !has_more_below || row_index < offset {
        return RowFade::Full;
    }
    let visible_index = row_index - offset;
    if visible_index >= visible_rows {
        return RowFade::Full;
    }
    let rows_from_bottom = visible_rows.saturating_sub(visible_index + 1);
    if rows_from_bottom >= fade_rows {
        return RowFade::Full;
    }
    let fade_step = fade_rows - rows_from_bottom;
    let soft_cutoff = (fade_rows / 3).max(1);
    let dim_cutoff = ((fade_rows * 2) / 3).max(soft_cutoff + 1);
    match fade_step {
        step if step <= soft_cutoff => RowFade::Soft,
        step if step <= dim_cutoff => RowFade::Dim,
        _ => RowFade::Ghost,
    }
}

pub(super) fn status_style(status: Option<i64>, fade: RowFade, theme: &Theme) -> Style {
    match status {
        Some(200..=299) => fade.accent_style(theme.ok).add_modifier(Modifier::BOLD),
        Some(300..=399) => fade.accent_style(theme.redirect),
        Some(400..=499) => fade
            .accent_style(theme.client_error)
            .add_modifier(Modifier::BOLD),
        Some(500..=599) => fade
            .accent_style(theme.server_error)
            .add_modifier(Modifier::BOLD),
        Some(_) => fade.accent_style(theme.accent),
        None => fade.secondary_style(theme),
    }
}

fn dim_color(color: Color, factor: f32) -> Color {
    let (red, green, blue) = color_rgb(color);
    Color::Rgb(
        (red as f32 * factor) as u8,
        (green as f32 * factor) as u8,
        (blue as f32 * factor) as u8,
    )
}

fn color_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Black => (0, 0, 0),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::Gray => (150, 150, 150),
        Color::DarkGray => (84, 84, 84),
        Color::LightRed => (241, 76, 76),
        Color::LightGreen => (35, 209, 139),
        Color::LightYellow => (245, 245, 67),
        Color::LightBlue => (59, 142, 234),
        Color::LightMagenta => (214, 112, 214),
        Color::LightCyan => (41, 184, 219),
        Color::White => (229, 229, 229),
        Color::Rgb(red, green, blue) => (red, green, blue),
        Color::Indexed(_) | Color::Reset => (150, 150, 150),
    }
}

pub(super) fn request_tree_marker(
    _row_index: usize,
    _total: usize,
    meta: Option<&RequestTreeMeta>,
    can_drill_down: bool,
    fade: RowFade,
    theme: &Theme,
) -> Line<'static> {
    let indent = meta
        .map(|meta| "  ".repeat(meta.depth.saturating_sub(1).min(6)))
        .unwrap_or_default();
    let dot = if can_drill_down { "›" } else { "" };
    let dot_style = if can_drill_down {
        fade.accent_style(theme.active_border)
            .add_modifier(Modifier::BOLD)
    } else {
        fade.secondary_style(theme)
    };

    Line::from(vec![
        Span::styled(dot.to_string(), dot_style),
        Span::raw(indent),
    ])
}

pub(super) fn method_style(method: &str, fade: RowFade, theme: &Theme) -> Style {
    match method {
        "GET" => fade.accent_style(theme.method_get),
        "POST" => fade
            .accent_style(theme.method_post)
            .add_modifier(Modifier::BOLD),
        "PUT" | "PATCH" => fade.accent_style(theme.method_write),
        "DELETE" => fade
            .accent_style(theme.method_delete)
            .add_modifier(Modifier::BOLD),
        _ => fade.base_style(theme),
    }
}

pub(super) fn resource_style(resource_type: &str, fade: RowFade, theme: &Theme) -> Style {
    match resource_type {
        "xhr" | "fetch" => fade.accent_style(theme.resource_xhr),
        "image" => fade.accent_style(theme.resource_image),
        "script" => fade.accent_style(theme.resource_script),
        "stylesheet" => fade.accent_style(theme.resource_style),
        "eventsource" => fade.accent_style(theme.resource_sse),
        "document" => fade.base_style(theme),
        _ => fade.secondary_style(theme),
    }
}

pub(super) fn resource_label(resource_type: &str) -> &'static str {
    match resource_type {
        "document" => "Doc",
        "stylesheet" => "CSS",
        "script" => "JS",
        "image" => "Img",
        "xhr" => "XHR",
        "fetch" => "Fetch",
        "eventsource" => "SSE",
        "websocket" => "WS",
        "manifest" => "Man",
        "font" => "Font",
        "media" => "Media",
        "" | "-" => "-",
        _ => "Other",
    }
}

pub(super) fn resource_type_line(
    resource_label: &str,
    highlight_terms: &[String],
) -> Line<'static> {
    highlight_text(resource_label, highlight_terms)
}

pub(super) fn duration_style(duration: Option<i64>, fade: RowFade, theme: &Theme) -> Style {
    match duration {
        Some(0..=99) => fade.accent_style(theme.ok),
        Some(100..=499) => fade.accent_style(theme.redirect),
        Some(500..=999) => fade.accent_style(theme.client_error),
        Some(_) => fade
            .accent_style(theme.server_error)
            .add_modifier(Modifier::BOLD),
        None => fade.secondary_style(theme),
    }
}

pub(super) fn status_text(request: &RequestView) -> String {
    request
        .status_code()
        .map(|status| status.to_string())
        .unwrap_or_else(|| "---".to_string())
}

pub(super) fn response_body_title(app: &WorkbenchState) -> String {
    let Some(request) = app.selected_request() else {
        return " Response Body ".to_string();
    };
    let status = request
        .status_code()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let size = request
        .response
        .as_ref()
        .and_then(|response| response.body_size)
        .map(format_bytes)
        .unwrap_or_else(|| "-".to_string());
    let mime = request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .map(|mime| compact_value(mime, 28))
        .unwrap_or_else(|| "-".to_string());
    let kind = if is_image_request(request) {
        "Image"
    } else if is_sse_request(request) {
        "SSE"
    } else {
        "Response Body"
    };
    let search = if app.body_search_query.is_empty() {
        String::new()
    } else {
        format!(" find:{}", compact_value(&app.body_search_query, 24))
    };
    format!(" {kind} {status} {size} {mime}{search} ")
}

pub(super) fn query_params_for_url(url: &str) -> Vec<(String, String)> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .unwrap_or_else(|| (part.to_string(), String::new()))
        })
        .collect()
}
