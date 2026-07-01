use super::*;

pub(super) fn render_storage(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);
    let entries = app.current_storage_entries();
    let list_lines = storage_list_lines(app, &entries, chunks[0]);
    let detail_lines = storage_detail_lines(app, entries.get(app.storage_selected));

    frame.render_widget(
        Paragraph::new(list_lines)
            .block(panel_block(
                format!("Storage {} keys", entries.len()),
                app.focus == FocusPane::Storage,
            ))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(faded_lines(
            detail_lines,
            app.storage_scroll,
            chunks[1],
            &app.config.theme,
            app.config.ui.bottom_fade_rows,
        ))
        .block(panel_block("Value", app.focus == FocusPane::Storage))
        .scroll((app.storage_scroll, 0))
        .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

pub(super) fn render_cookies(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);
    let entries = app.current_cookie_entries();
    let list_lines = cookie_list_lines(app, &entries, chunks[0]);
    let detail_lines = cookie_detail_lines(app, entries.get(app.cookie_selected));

    frame.render_widget(
        Paragraph::new(list_lines)
            .block(panel_block(
                format!("Cookies {} keys", entries.len()),
                app.focus == FocusPane::Cookies,
            ))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(faded_lines(
            detail_lines,
            app.cookie_scroll,
            chunks[1],
            &app.config.theme,
            app.config.ui.bottom_fade_rows,
        ))
        .block(panel_block("Value", app.focus == FocusPane::Cookies))
        .scroll((app.cookie_scroll, 0))
        .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn storage_list_lines(
    app: &WorkbenchState,
    entries: &[CurrentStorageEntry],
    area: Rect,
) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return vec![Line::raw("No storage values captured yet.")];
    }
    let visible_rows = pane_visible_rows(area);
    let start = selected_window_start(app.storage_selected, visible_rows, entries.len());
    let content_width = area.width.saturating_sub(2).max(1) as usize;
    let fixed_width = 8;
    let available = content_width.saturating_sub(fixed_width);
    let key_width = if available >= 24 {
        (available / 2).clamp(8, 28)
    } else {
        (available / 2).max(4)
    };
    let value_width = available.saturating_sub(key_width).max(4);
    entries
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_rows)
        .map(|(index, entry)| {
            let selected = index == app.storage_selected;
            let fade = bottom_overlay_fade(
                index,
                start,
                visible_rows,
                start + visible_rows < entries.len(),
                app.config.ui.bottom_fade_rows,
            );
            let selected_modifier = if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    fade.accent_style(app.config.theme.accent),
                ),
                Span::styled(
                    format!("{} ", storage_type_short(&entry.storage_type)),
                    fade.accent_style(app.config.theme.panel_title),
                ),
                Span::styled(
                    compact_value(&entry.key, key_width),
                    fade.base_style(&app.config.theme)
                        .add_modifier(selected_modifier),
                ),
                Span::styled(" = ", fade.secondary_style(&app.config.theme)),
                Span::styled(
                    compact_value(&entry.value, value_width),
                    fade.secondary_style(&app.config.theme),
                ),
            ])
        })
        .collect()
}

fn storage_type_short(storage_type: &str) -> &'static str {
    match storage_type {
        "localStorage" => "LS",
        "sessionStorage" => "SS",
        _ => "--",
    }
}

fn storage_detail_lines(
    app: &WorkbenchState,
    entry: Option<&CurrentStorageEntry>,
) -> Vec<Line<'static>> {
    let Some(entry) = entry else {
        return vec![Line::raw("No storage value selected.")];
    };
    let mut lines = vec![
        labeled_line("type", entry.storage_type.clone()),
        labeled_line("origin", entry.origin.clone()),
        labeled_line("key", entry.key.clone()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("value ", label_style()),
            Span::styled("e to edit", key_style()),
        ]),
        Line::raw(""),
    ];
    if entry.value.is_empty() {
        lines.push(Line::styled("(empty string)", muted_style()));
    } else {
        lines.extend(plain_value_lines(&entry.value));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("{} live events", app.storage_events.len()),
        muted_style(),
    ));
    lines
}

fn cookie_list_lines(
    app: &WorkbenchState,
    entries: &[CurrentCookieEntry],
    area: Rect,
) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return vec![Line::raw("No cookies captured yet.")];
    }
    let visible_rows = pane_visible_rows(area);
    let start = selected_window_start(app.cookie_selected, visible_rows, entries.len());
    entries
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_rows)
        .map(|(index, cookie)| {
            let selected = index == app.cookie_selected;
            let fade = bottom_overlay_fade(
                index,
                start,
                visible_rows,
                start + visible_rows < entries.len(),
                app.config.ui.bottom_fade_rows,
            );
            let selected_modifier = if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    fade.accent_style(app.config.theme.accent),
                ),
                Span::styled(
                    compact_value(&cookie.name, 26),
                    fade.base_style(&app.config.theme)
                        .add_modifier(selected_modifier),
                ),
                Span::styled(
                    format!("  {}{}", cookie.domain, cookie.path),
                    fade.secondary_style(&app.config.theme),
                ),
            ])
        })
        .collect()
}

fn cookie_detail_lines(
    app: &WorkbenchState,
    cookie: Option<&CurrentCookieEntry>,
) -> Vec<Line<'static>> {
    let Some(cookie) = cookie else {
        return vec![Line::raw("No cookie selected.")];
    };
    let mut lines = vec![
        labeled_line("name", cookie.name.clone()),
        labeled_line("domain", cookie.domain.clone()),
        labeled_line("path", cookie.path.clone()),
        labeled_line("flags", cookie.flags.clone()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("value ", label_style()),
            Span::styled("e to edit", key_style()),
        ]),
        Line::raw(""),
    ];
    if cookie.value.is_empty() {
        lines.push(Line::styled("(empty string)", muted_style()));
    } else {
        lines.extend(plain_value_lines(&cookie.value));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("{} live events", app.cookie_events.len()),
        muted_style(),
    ));
    lines
}

fn plain_value_lines(value: &str) -> Vec<Line<'static>> {
    value
        .lines()
        .map(|line| Line::styled(line.to_string(), Style::default().fg(GB_FG)))
        .collect()
}
