use super::*;
use std::borrow::Cow;

pub(super) fn render(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let render_started = std::time::Instant::now();
    let highlight_terms = filter_highlight_terms(&app.request_filter);
    let rows = if app.filtered_request_rows.is_empty() {
        vec![
            Row::new([
                Cell::from(" "),
                Cell::from("WAIT"),
                Cell::from("-"),
                Cell::from("capture"),
                Cell::from("-"),
                Cell::from("open, refresh, or wait for fresh matching requests"),
                Cell::from("-"),
                Cell::from("-"),
            ])
            .style(muted_style()),
        ]
    } else {
        let total = app.filtered_request_rows.len();
        let visible_rows = visible_request_rows(area);
        let selected = app.table_state.selected().unwrap_or(0).min(total - 1);
        let offset = request_window_start(selected, visible_rows, total);
        let end = offset.saturating_add(visible_rows).min(total);
        let has_more_below = end < total;
        app.filtered_request_rows
            .get(offset..end)
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .filter_map(|(visible_index, index)| {
                let row_index = offset + visible_index;
                let fade = bottom_overlay_fade(
                    row_index,
                    offset,
                    visible_rows,
                    has_more_below,
                    app.config.ui.bottom_fade_rows,
                );
                let theme = &app.config.theme;
                let base_style = fade.base_style(theme);
                let request = app.requests.get(*index)?;
                let resource_type = request.request.resource_type.as_deref().unwrap_or("-");
                let resource_label = resource_label(resource_type);
                let tree_meta = app.request_tree_metas.get(*index);
                let can_drill_down = app.request_can_drill_down(*index);
                let domain = tree_meta
                    .map(|meta| Cow::Borrowed(meta.domain.as_str()))
                    .unwrap_or_else(|| Cow::Owned(domain_for_url(&request.request.url)));
                let route_remainder = app.request_route_remainder(*index);
                let path = route_remainder
                    .as_deref()
                    .map(Cow::Borrowed)
                    .or_else(|| tree_meta.map(|meta| Cow::Borrowed(meta.path.as_str())))
                    .unwrap_or_else(|| Cow::Owned(path_for_url(&request.request.url)));
                Some(
                    Row::new([
                        Cell::from(request_tree_marker(
                            row_index,
                            total,
                            tree_meta,
                            can_drill_down,
                            fade,
                            theme,
                        )),
                        Cell::from(Span::styled(
                            status_text(request),
                            status_style(request.status_code(), fade, theme),
                        )),
                        Cell::from(highlight_text(&request.request.method, &highlight_terms))
                            .style(method_style(&request.request.method, fade, theme)),
                        Cell::from(resource_type_line(resource_label, &highlight_terms))
                            .style(resource_style(resource_type, fade, theme)),
                        Cell::from(highlight_text(domain.as_ref(), &highlight_terms))
                            .style(fade.secondary_style(theme)),
                        Cell::from(highlight_text(path.as_ref(), &highlight_terms))
                            .style(base_style),
                        Cell::from(match request.duration_ms() {
                            Some(duration) => {
                                let mut spans = vec![Span::styled(
                                    format!("{duration}ms "),
                                    duration_style(Some(duration), fade, theme),
                                )];
                                spans.extend(latency_bar(duration, theme));
                                Line::from(spans)
                            }
                            None => {
                                Line::from(Span::styled("-", duration_style(None, fade, theme)))
                            }
                        }),
                        Cell::from(
                            request
                                .response
                                .as_ref()
                                .and_then(|response| response.body_size)
                                .map(format_bytes)
                                .unwrap_or_else(|| "-".to_string()),
                        )
                        .style(fade.accent_style(theme.resource_image)),
                    ])
                    .style(base_style),
                )
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(20),
            Constraint::Min(24),
            Constraint::Length(11),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new([
            ">", "CODE", "METHOD", "TYPE", "DOMAIN", "PATH", "TIME", "SIZE",
        ])
        .style(muted_style().add_modifier(Modifier::BOLD)),
    )
    .block(themed_panel_block(
        requests_title(app),
        Some('R'),
        app.focus == FocusPane::Requests,
        &app.config.theme,
    ))
    .row_highlight_style(
        Style::default()
            .fg(app.config.theme.active_border)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▎ ");

    let mut visible_state = visible_request_table_state(app, visible_request_rows(area));
    frame.render_stateful_widget(table, area, &mut visible_state);
    app.perf.last_request_render_ms = render_started.elapsed().as_millis();
    app.perf.max_request_render_ms = app
        .perf
        .max_request_render_ms
        .max(app.perf.last_request_render_ms);
}
