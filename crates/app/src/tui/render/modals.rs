use super::{
    GB_BG2, GB_FG, compact_value, key_style, label_style, modal_section_style,
    modal_selection_style, muted_style, panel_block, themed_panel_block, warning_style,
};
use crate::tui::state::{SqlResultsView, WorkbenchState, WorkbenchView};
use faro_core::now_ms;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table, Wrap};

pub(super) fn render_theme_preview(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 72, 22);
    let theme = &app.config.theme;
    let lines = vec![
        Line::from(vec![
            Span::styled("Theme Preview", super::panel_title_style(true)),
            Span::styled("  esc closes", muted_style()),
        ]),
        Line::raw(""),
        theme_swatch("text", theme.text),
        theme_swatch("muted", theme.muted),
        theme_swatch("accent", theme.accent),
        theme_swatch("panel title", theme.panel_title),
        theme_swatch("panel border", theme.panel_border),
        theme_swatch("active border", theme.active_border),
        theme_swatch("tree edge", theme.tree_edge),
        Line::raw(""),
        theme_swatch("ok / 2xx", theme.ok),
        theme_swatch("redirect / 3xx", theme.redirect),
        theme_swatch("client error", theme.client_error),
        theme_swatch("server error", theme.server_error),
        Line::raw(""),
        theme_swatch("xhr/fetch", theme.resource_xhr),
        theme_swatch("image", theme.resource_image),
        theme_swatch("script", theme.resource_script),
        theme_swatch("style", theme.resource_style),
        theme_swatch("sse", theme.resource_sse),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(themed_panel_block(
                " Theme Preview ",
                Some('T'),
                true,
                &app.config.theme,
            ))
            .style(Style::default().fg(GB_FG))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn theme_swatch(label: &'static str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), label_style()),
        Span::styled("██".to_string(), Style::default().fg(color)),
        Span::raw("  "),
        Span::styled(format!("{color:?}"), muted_style()),
    ])
}

pub(super) fn render_help(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 82, 24);
    let lines = vec![
        Line::from(vec![
            Span::styled("Faro Keys", super::panel_title_style(true)),
            Span::styled("  press ", muted_style()),
            Span::styled("?", key_style()),
            Span::styled(" or ", muted_style()),
            Span::styled("esc", key_style()),
            Span::styled(" to close", muted_style()),
        ]),
        Line::styled(
            "-".repeat(area.width.saturating_sub(4) as usize),
            muted_style(),
        ),
        Line::from(vec![
            Span::styled("NAV", modal_section_style()),
            Span::raw("      "),
            Span::styled("p", key_style()),
            Span::raw(" palette  "),
            Span::styled("tab", key_style()),
            Span::raw(" focus  "),
            Span::styled("1-6", key_style()),
            Span::raw(" views  "),
            Span::styled("j/k", key_style()),
            Span::raw(" move  "),
            Span::styled("u/d", key_style()),
            Span::raw(" scroll  "),
            Span::styled("g/G", key_style()),
            Span::raw(" top/bottom"),
        ]),
        Line::from(vec![
            Span::styled("NETWORK", modal_section_style()),
            Span::raw("  "),
            Span::styled("h/l", key_style()),
            Span::raw(" detail tabs  "),
            Span::styled("s", key_style()),
            Span::raw(" sort  "),
            Span::styled("S", key_style()),
            Span::raw(" sessions  "),
            Span::styled("f", key_style()),
            Span::raw(" preset  "),
            Span::styled("enter", key_style()),
            Span::raw(" enter route  "),
            Span::styled("space", key_style()),
            Span::raw(" collapse  "),
            Span::styled("backspace", key_style()),
            Span::raw(" up  "),
            Span::styled("c", key_style()),
            Span::raw(" clear visible"),
        ]),
        Line::from(vec![
            Span::styled("CAPTURE", modal_section_style()),
            Span::raw("  "),
            Span::styled("o", key_style()),
            Span::raw(" open browser  "),
            Span::styled("F5", key_style()),
            Span::raw(" refresh page  "),
            Span::styled("e", key_style()),
            Span::raw(" body/editor  "),
            Span::styled("y", key_style()),
            Span::raw(" copy curl  "),
            Span::styled("w", key_style()),
            Span::raw(" save exchange"),
        ]),
        Line::from(vec![
            Span::styled("PANES", modal_section_style()),
            Span::raw("    "),
            Span::styled("R", key_style()),
            Span::raw(" requests  "),
            Span::styled("D", key_style()),
            Span::raw(" detail  "),
            Span::styled("B", key_style()),
            Span::raw(" body"),
        ]),
        Line::from(vec![
            Span::styled("REPLAY", modal_section_style()),
            Span::raw("   "),
            Span::styled("r", key_style()),
            Span::raw(" replay  "),
            Span::styled("p", key_style()),
            Span::raw(" palette for edit replay and diff replay"),
        ]),
        Line::from(vec![
            Span::styled("SCRIPTS", modal_section_style()),
            Span::raw("  "),
            Span::styled("4", key_style()),
            Span::raw(" scripts  "),
            Span::styled("n", key_style()),
            Span::raw(" new  "),
            Span::styled("e", key_style()),
            Span::raw(" edit  "),
            Span::styled("r", key_style()),
            Span::raw(" run  "),
            Span::styled("R", key_style()),
            Span::raw(" rename  "),
            Span::styled("D", key_style()),
            Span::raw(" duplicate  "),
            Span::styled("x", key_style()),
            Span::raw(" delete"),
        ]),
        Line::from(vec![
            Span::styled("LAYOUT", modal_section_style()),
            Span::raw("   "),
            Span::styled("m", key_style()),
            Span::raw(" maximize/focus  "),
            Span::styled("z", key_style()),
            Span::raw(" density  "),
            Span::styled("ctrl+left/right", key_style()),
            Span::raw(" request width  "),
            Span::styled("ctrl+up/down", key_style()),
            Span::raw(" detail height"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("FILTERS", modal_section_style()),
            Span::raw("  plain terms, structured tokens, or regex patterns"),
        ]),
        Line::styled(
            "         status:5xx method:post domain:localhost has:body",
            muted_style(),
        ),
        Line::styled(
            "         path:/api/v[0-9]+  method:^(post|put)$  /graphql|rest/",
            muted_style(),
        ),
        Line::styled(
            "         duration:>500 size:>100kb reqbody:email resbody:error",
            muted_style(),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled("CONSOLE", modal_section_style()),
            Span::raw("  "),
            Span::styled("2", key_style()),
            Span::raw(" console view  "),
            Span::styled("e", key_style()),
            Span::raw(" evaluate JS  "),
            Span::styled("c", key_style()),
            Span::raw(" clear visible console  filters: level:error kind:eval /token.*/"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("STATE", modal_section_style()),
            Span::raw(format!(
                "    view={}  focus={}  density={}  filter={}  split={}:{} / {}:{}",
                app.view.label(),
                app.focus.label(),
                app.density_mode.label(),
                active_filter_text(app),
                app.requests_percent,
                100 - app.requests_percent,
                app.detail_percent,
                100 - app.detail_percent
            )),
        ]),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(themed_panel_block(
                " Command Matrix ",
                Some('?'),
                true,
                &app.config.theme,
            ))
            .style(Style::default().fg(GB_FG))
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn render_sessions(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 92, 24);
    frame.render_widget(Clear, area);
    frame.render_widget(
        themed_panel_block(" Sessions ", Some('S'), true, &app.config.theme),
        area,
    );
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("sessions ", label_style()),
                Span::raw(app.sessions.len().to_string()),
                Span::styled("  active ", muted_style()),
                Span::raw(
                    app.active_session_id
                        .as_deref()
                        .map(|id| compact_value(id, 12))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("enter", key_style()),
                Span::raw(" open  "),
                Span::styled("x", key_style()),
                Span::raw(" delete  "),
                Span::styled("esc", key_style()),
                Span::raw(" close"),
            ]),
        ])
        .style(Style::default().fg(GB_FG)),
        chunks[0],
    );

    if app.sessions.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No sessions captured yet.", warning_style()))
                .style(Style::default().fg(GB_FG)),
            chunks[1],
        );
    } else {
        let selected = app.session_state.selected();
        let visible_rows = chunks[1].height.saturating_sub(3).max(1) as usize;
        let visible_start = selected
            .unwrap_or(0)
            .saturating_sub(visible_rows.saturating_sub(1));
        let rows = app
            .sessions
            .iter()
            .enumerate()
            .skip(visible_start)
            .take(visible_rows)
            .map(|(index, entry)| {
                let is_selected = selected == Some(index);
                let is_active = app.active_session_id.as_deref() == Some(entry.session.id.as_str());
                let base = if is_selected {
                    Style::default().fg(app.config.theme.text).bg(GB_BG2)
                } else {
                    Style::default().fg(GB_FG)
                };
                let muted = if is_selected {
                    muted_style().bg(GB_BG2)
                } else {
                    muted_style()
                };
                Row::new([
                    Cell::from(Span::styled(if is_active { "*" } else { " " }, base)),
                    Cell::from(Span::styled(
                        session_created_label(entry.session.created_at),
                        muted,
                    )),
                    Cell::from(Span::styled(
                        entry.request_count.to_string(),
                        if is_selected {
                            label_style().bg(GB_BG2)
                        } else {
                            label_style()
                        },
                    )),
                    Cell::from(Span::styled(
                        entry.console_error_count.to_string(),
                        if entry.console_error_count > 0 {
                            warning_style()
                        } else {
                            muted
                        },
                    )),
                    Cell::from(Span::styled(entry.replay_count.to_string(), muted)),
                    Cell::from(Span::styled(entry.websocket_count.to_string(), muted)),
                    Cell::from(Span::styled(entry.storage_count.to_string(), muted)),
                    Cell::from(Span::styled(entry.cookie_count.to_string(), muted)),
                    Cell::from(Span::styled(session_title(entry), base)),
                    Cell::from(Span::styled(compact_value(&entry.session.id, 6), muted)),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(2),
                    Constraint::Length(12),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(4),
                    Constraint::Length(4),
                    Constraint::Length(5),
                    Constraint::Length(4),
                    Constraint::Min(28),
                    Constraint::Length(8),
                ],
            )
            .header(
                Row::new([
                    "", "CREATED", "REQS", "ERRS", "RPL", "WS", "STORE", "CK", "SESSION", "ID",
                ])
                .style(muted_style().add_modifier(Modifier::BOLD)),
            )
            .style(Style::default().fg(GB_FG)),
            chunks[1],
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", key_style()),
            Span::raw(" move  "),
            Span::styled("enter", key_style()),
            Span::raw(" switch  "),
            Span::styled("x", key_style()),
            Span::raw(" delete selected session"),
        ]))
        .style(Style::default().fg(GB_FG)),
        chunks[2],
    );
}

fn session_title(entry: &crate::tui::state::SessionView) -> String {
    let Some(root_url) = entry.session.root_url.as_deref() else {
        return entry
            .session
            .name
            .as_deref()
            .map(|value| compact_value(value, 72))
            .unwrap_or_else(|| "untitled session".to_string());
    };
    let host = host_for_url(root_url);
    let name = entry.session.name.as_deref().unwrap_or_default();
    if name.is_empty() || name == "CDP session" {
        compact_value(&format!("{host}  {root_url}"), 72)
    } else {
        compact_value(&format!("{host}  {name}  {root_url}"), 72)
    }
}

fn host_for_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    without_scheme
        .split(['/', '?', '#'])
        .next()
        .filter(|host| !host.is_empty())
        .unwrap_or(url)
        .to_string()
}

fn session_created_label(created_at: i64) -> String {
    if created_at <= 0 {
        return "-".to_string();
    }
    let elapsed_ms = now_ms().saturating_sub(created_at).max(0);
    let elapsed_seconds = elapsed_ms / 1000;
    if elapsed_seconds < 60 {
        return format!("{elapsed_seconds}s ago");
    }
    let elapsed_minutes = elapsed_seconds / 60;
    if elapsed_minutes < 60 {
        return format!("{elapsed_minutes}m ago");
    }
    let elapsed_hours = elapsed_minutes / 60;
    if elapsed_hours < 48 {
        return format!("{elapsed_hours}h ago");
    }
    let elapsed_days = elapsed_hours / 24;
    format!("{elapsed_days}d ago")
}

pub(super) fn render_palette(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = centered_rect(frame.area(), 82, 22);
    let entries = app.filtered_palette_entries();
    frame.render_widget(Clear, area);
    frame.render_widget(
        themed_panel_block(" Command Palette ", Some('P'), true, &app.config.theme),
        area,
    );
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(inner);
    let query = if app.palette_query.is_empty() {
        "type to fuzzy search commands, presets, and views".to_string()
    } else {
        app.palette_query.clone()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("> ", key_style()),
                Span::styled(query, Style::default().fg(app.config.theme.text)),
            ]),
            Line::from(vec![
                Span::styled("matches ", muted_style()),
                Span::raw(entries.len().to_string()),
                Span::styled("  active ", muted_style()),
                Span::raw(app.view.label()),
                Span::styled("  filter ", muted_style()),
                Span::raw(active_filter_text(app)),
            ]),
        ])
        .style(Style::default().fg(GB_FG)),
        chunks[0],
    );
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No commands match.", warning_style()))
                .style(Style::default().fg(GB_FG)),
            chunks[1],
        );
    } else {
        let visible_rows = chunks[1].height.saturating_sub(3).max(1) as usize;
        let visible_start = app.palette_selected.saturating_sub(visible_rows / 2);
        let rows = entries
            .iter()
            .enumerate()
            .skip(visible_start)
            .take(visible_rows)
            .map(|(index, entry)| {
                let selected = index == app.palette_selected;
                let title_style = if selected {
                    Style::default()
                        .fg(app.config.theme.text)
                        .bg(GB_BG2)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(GB_FG)
                };
                let hint_style = if selected {
                    muted_style().bg(GB_BG2)
                } else {
                    muted_style()
                };
                Row::new([
                    Cell::from(Span::styled(
                        palette_entry_group(entry.title),
                        modal_selection_style(selected),
                    )),
                    Cell::from(Span::styled(palette_entry_name(entry.title), title_style)),
                    Cell::from(Span::styled(compact_value(entry.hint, 36), hint_style)),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(12),
                    Constraint::Length(28),
                    Constraint::Min(18),
                ],
            )
            .header(
                Row::new(["SCOPE", "COMMAND", "MATCH"])
                    .style(muted_style().add_modifier(Modifier::BOLD)),
            )
            .style(Style::default().fg(GB_FG))
            .row_highlight_style(Style::default().bg(GB_BG2).fg(app.config.theme.text))
            .highlight_symbol("▎ "),
            chunks[1],
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("enter", key_style()),
            Span::raw(" run  "),
            Span::styled("esc", key_style()),
            Span::raw(" close  "),
            Span::styled("up/down", key_style()),
            Span::raw(" select"),
        ]))
        .style(Style::default().fg(GB_FG)),
        chunks[2],
    );
}

fn palette_entry_group(title: &str) -> &'static str {
    if title.starts_with("View:") {
        "VIEW"
    } else if title.starts_with("Filter:") {
        "REQUEST"
    } else if title.starts_with("Console") {
        "CONSOLE"
    } else if title.starts_with("WebSocket") {
        "WS"
    } else if title.starts_with("Scripts:") {
        "SCRIPT"
    } else if title.starts_with("Layout:") {
        "LAYOUT"
    } else if title.starts_with("Sort:") {
        "SORT"
    } else if title.starts_with("Debug:") {
        "DEBUG"
    } else {
        "ACTION"
    }
}

fn palette_entry_name(title: &str) -> String {
    title
        .split_once(':')
        .map(|(_, name)| name.trim().to_string())
        .unwrap_or_else(|| title.to_string())
}

pub(super) fn render_sql_results(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let frame_area = frame.area();
    let width = frame_area.width.saturating_sub(8).clamp(48, 128);
    let height = frame_area.height.saturating_sub(4).clamp(14, 34);
    let area = centered_rect(frame_area, width, height);
    let Some(result) = &app.sql_result else {
        return;
    };

    frame.render_widget(Clear, area);
    if let Some(error) = &result.error {
        let lines = vec![
            Line::from(vec![
                Span::styled("error ", label_style()),
                Span::styled(error.clone(), warning_style()),
            ]),
            Line::raw(""),
            Line::styled("query", label_style()),
            Line::raw(compact_value(&result.query.replace('\n', " "), 180)),
            Line::raw(""),
            Line::from(vec![
                Span::styled("esc", key_style()),
                Span::raw(" close  "),
                Span::styled("p", key_style()),
                Span::raw(" palette"),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .block(panel_block("SQL Results", true))
                .style(Style::default().fg(GB_FG))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);
    let summary = vec![
        Line::from(vec![
            Span::styled("rows ", label_style()),
            Span::raw(result.rows.len().to_string()),
            Span::styled("  columns ", label_style()),
            Span::raw(result.columns.len().to_string()),
            Span::styled("  duration ", label_style()),
            Span::raw(format!("{}ms", result.duration_ms)),
            Span::styled("  row ", label_style()),
            Span::raw(format!(
                "{}",
                app.sql_row_scroll
                    .saturating_add(1)
                    .min(result.rows.len().max(1))
            )),
            Span::styled("  col ", label_style()),
            Span::raw(format!(
                "{}",
                app.sql_col_scroll
                    .saturating_add(1)
                    .min(result.columns.len().max(1))
            )),
        ]),
        Line::from(vec![
            Span::styled("query ", label_style()),
            Span::raw(compact_value(&result.query.replace('\n', " "), 180)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(summary)
            .block(panel_block("SQL Results", true))
            .style(Style::default().fg(GB_FG)),
        chunks[0],
    );

    let visible_columns = visible_sql_columns(result, app.sql_col_scroll, chunks[1].width);
    let widths = visible_columns
        .iter()
        .map(|(_, width)| Constraint::Length(*width))
        .collect::<Vec<_>>();
    let header = Row::new(
        visible_columns
            .iter()
            .map(|(index, _)| {
                Cell::from(
                    result
                        .columns
                        .get(*index)
                        .cloned()
                        .unwrap_or_else(|| "-".to_string()),
                )
            })
            .collect::<Vec<_>>(),
    )
    .style(muted_style().add_modifier(Modifier::BOLD));
    let visible_rows = chunks[1].height.saturating_sub(3).max(1) as usize;
    let rows = result
        .rows
        .iter()
        .skip(app.sql_row_scroll)
        .take(visible_rows)
        .map(|row| {
            Row::new(
                visible_columns
                    .iter()
                    .map(|(index, width)| {
                        Cell::from(
                            row.get(*index)
                                .map(|value| compact_value(value, width.saturating_sub(1) as usize))
                                .unwrap_or_default(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(rows, widths)
            .header(header)
            .block(panel_block("Rows", false))
            .style(Style::default().fg(GB_FG)),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("j/k", key_style()),
            Span::raw(" rows  "),
            Span::styled("h/l", key_style()),
            Span::raw(" columns  "),
            Span::styled("g/G", key_style()),
            Span::raw(" top/bottom  "),
            Span::styled("esc", key_style()),
            Span::raw(" close  "),
            Span::styled("p", key_style()),
            Span::raw(" palette"),
        ]))
        .style(Style::default().fg(GB_FG)),
        chunks[2],
    );
}

fn visible_sql_columns(
    result: &SqlResultsView,
    column_scroll: usize,
    area_width: u16,
) -> Vec<(usize, u16)> {
    if result.columns.is_empty() {
        return vec![(0, area_width.saturating_sub(4).max(8))];
    }

    let mut used_width = 0_u16;
    let mut visible = Vec::new();
    let available_width = area_width.saturating_sub(4).max(8);
    for index in column_scroll.min(result.columns.len())..result.columns.len() {
        let width = sql_column_width(result, index);
        if !visible.is_empty() && used_width.saturating_add(width) > available_width {
            break;
        }
        used_width = used_width.saturating_add(width);
        visible.push((index, width));
    }
    if visible.is_empty() {
        let index = column_scroll.min(result.columns.len().saturating_sub(1));
        visible.push((index, sql_column_width(result, index).min(available_width)));
    }
    visible
}

fn sql_column_width(result: &SqlResultsView, index: usize) -> u16 {
    let header_width = result
        .columns
        .get(index)
        .map(|column| column.len())
        .unwrap_or_default();
    let value_width = result
        .rows
        .iter()
        .filter_map(|row| row.get(index))
        .take(80)
        .map(|value| value.len())
        .max()
        .unwrap_or_default();
    header_width.max(value_width).clamp(8, 34) as u16
}

fn active_filter_text(app: &WorkbenchState) -> String {
    match app.view {
        WorkbenchView::Console if !app.console_filter.is_empty() => app.console_filter.clone(),
        WorkbenchView::Console => "all".to_string(),
        WorkbenchView::WebSockets if !app.websocket_filter.is_empty() => {
            app.websocket_filter.clone()
        }
        WorkbenchView::WebSockets => "all".to_string(),
        _ if !app.request_filter.is_empty() => app.request_filter.clone(),
        _ => "all".to_string(),
    }
}

fn centered_rect(parent: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(parent.width.saturating_sub(2)).max(1);
    let height = height.min(parent.height.saturating_sub(2)).max(1);
    Rect {
        x: parent.x + parent.width.saturating_sub(width) / 2,
        y: parent.y + parent.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::SessionView;
    use faro_core::Session;

    #[test]
    fn session_title_prefers_domain_over_generic_cdp_name() {
        let entry = SessionView {
            session: Session {
                id: "session".to_string(),
                created_at: 0,
                name: Some("CDP session".to_string()),
                root_url: Some("https://api.example.test/path".to_string()),
            },
            request_count: 0,
            console_error_count: 0,
            replay_count: 0,
            websocket_count: 0,
            storage_count: 0,
            cookie_count: 0,
        };

        let title = session_title(&entry);

        assert!(title.starts_with("api.example.test"));
        assert!(title.contains("https://api.example.test/path"));
        assert!(!title.starts_with("CDP session"));
    }

    #[test]
    fn host_for_url_extracts_domain() {
        assert_eq!(
            host_for_url("https://example.test/path?q=1"),
            "example.test"
        );
        assert_eq!(host_for_url("localhost:5173/app"), "localhost:5173");
    }
}
