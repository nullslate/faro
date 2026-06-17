#![allow(clippy::items_after_test_module)]

mod modals;
mod perf;

use super::layout::{DensityMode, LayoutMode};
use super::state::{
    BodyTreeItem, CurrentCookieEntry, CurrentStorageEntry, DetailTab, FocusPane, InputMode,
    RequestTreeMeta, RequestView, WorkbenchState, WorkbenchView, domain_for_url,
    formatted_request_body, formatted_response_body, path_for_url, websocket_opcode_label,
};
use crate::config::Theme;
use faro_core::{ConsoleLevel, ConsoleLog, WebSocketFrameDirection, WebSocketFrameRecord};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, TableState, Wrap,
};

const GB_FG: Color = Color::Rgb(235, 219, 178);
const GB_MUTED: Color = Color::Rgb(146, 131, 116);
const GB_BG2: Color = Color::Rgb(58, 58, 54);
const GB_RED: Color = Color::Rgb(234, 105, 98);
const GB_GREEN: Color = Color::Rgb(169, 182, 101);
const GB_YELLOW: Color = Color::Rgb(216, 166, 87);
const GB_BLUE: Color = Color::Rgb(125, 174, 163);
const GB_PURPLE: Color = Color::Rgb(211, 134, 155);
const GB_AQUA: Color = Color::Rgb(137, 180, 130);
const GB_ORANGE: Color = Color::Rgb(231, 138, 78);
const GB_FAINT: Color = Color::Rgb(102, 92, 84);

pub(crate) fn render(frame: &mut ratatui::Frame, app: &mut WorkbenchState) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(14),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, root[0], app);
    match app.layout_mode {
        LayoutMode::Normal => render_normal_layout(frame, root[1], app),
        LayoutMode::Focused => render_focused_layout(frame, root[1], app),
    }
    render_status(frame, root[2], app);
    if app.sql_result.is_some() {
        modals::render_sql_results(frame, app);
    }
    if app.input_mode == InputMode::Palette {
        modals::render_palette(frame, app);
    }
    if app.show_help {
        modals::render_help(frame, app);
    }
    if app.show_theme_preview {
        modals::render_theme_preview(frame, app);
    }
    if app.show_perf {
        perf::render(frame, app);
    }
}

fn render_normal_layout(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    render_normal_content(frame, area, app);
}

fn render_normal_content(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    match app.view {
        WorkbenchView::Network => render_network_view(frame, area, app),
        WorkbenchView::Console => render_console(frame, area, app),
        WorkbenchView::WebSockets => render_websockets(frame, area, app),
        WorkbenchView::Scripts => render_scripts(frame, area, app),
        WorkbenchView::Storage => render_storage(frame, area, app),
        WorkbenchView::Cookies => render_cookies(frame, area, app),
    }
}

fn render_network_view(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let root = match app.density_mode {
        DensityMode::Compact => Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(12)])
            .split(area),
        DensityMode::Comfortable => Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(if app.active_route_summary().is_some() {
                    4
                } else {
                    3
                }),
                Constraint::Min(12),
            ])
            .split(area),
    };

    let content_area = match app.density_mode {
        DensityMode::Compact => {
            render_network_compact_bar(frame, root[0], app);
            root[1]
        }
        DensityMode::Comfortable => {
            render_network_bar(frame, root[0], app);
            render_stats_panel(frame, root[1], app);
            root[2]
        }
    };

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.requests_percent),
            Constraint::Percentage(100 - app.requests_percent),
        ])
        .split(content_area);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.detail_percent),
            Constraint::Percentage(100 - app.detail_percent),
        ])
        .split(body[1]);

    render_requests(frame, body[0], app);
    render_detail(frame, right[0], app);
    render_body(frame, right[1], app);
}

fn render_focused_layout(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    match app.focus {
        FocusPane::Requests => render_requests(frame, area, app),
        FocusPane::Detail => render_detail(frame, area, app),
        FocusPane::Body => render_body(frame, area, app),
        FocusPane::Console => render_console(frame, area, app),
        FocusPane::WebSockets => render_websockets(frame, area, app),
        FocusPane::Scripts => render_scripts(frame, area, app),
        FocusPane::Storage => render_storage(frame, area, app),
        FocusPane::Cookies => render_cookies(frame, area, app),
    }
}

fn render_header(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let header_bg = Color::Rgb(29, 32, 33);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    let mut title_spans = vec![
        Span::styled(
            " faro ",
            Style::default()
                .fg(header_bg)
                .bg(app.config.theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    title_spans.extend(favicon_spans(app));
    title_spans.extend(if area.width < 90 {
        compact_header_spans(app)
    } else {
        full_header_spans(app)
    });
    let title = Line::from(title_spans);
    let block = Paragraph::new(title).style(Style::default().bg(header_bg));
    frame.render_widget(block, rows[0]);
    frame.render_widget(
        Paragraph::new(view_tabs_line(app)).style(Style::default().fg(GB_FG).bg(header_bg)),
        rows[1],
    );
}

fn view_tabs_line(app: &WorkbenchState) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, tab) in [
        (
            "1",
            format!("Net {}", app.requests.len()),
            app.view == WorkbenchView::Network,
        ),
        (
            "2",
            format!("Console {}", console_error_badge(app)),
            app.view == WorkbenchView::Console,
        ),
        (
            "3",
            format!("WS {}", app.websocket_frames.len()),
            app.view == WorkbenchView::WebSockets,
        ),
        (
            "4",
            format!("Scripts {}", app.scripts.len()),
            app.view == WorkbenchView::Scripts,
        ),
        (
            "5",
            format!("Storage {}", app.storage_events.len()),
            app.view == WorkbenchView::Storage,
        ),
        (
            "6",
            format!("Cookies {}", cookie_count(app)),
            app.view == WorkbenchView::Cookies,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.extend(view_tab_spans(tab.0, tab.1, tab.2, &app.config.theme));
    }
    Line::from(spans)
}

fn view_tab_spans(
    key: &'static str,
    label: String,
    active: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let header_bg = Color::Rgb(29, 32, 33);
    if active {
        vec![
            Span::styled(
                format!(" {key} "),
                Style::default()
                    .fg(header_bg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(theme.text)
                    .bg(GB_BG2)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![Span::styled(
            format!(" {key} {label} "),
            Style::default().fg(theme.muted).bg(header_bg),
        )]
    }
}

fn console_error_badge(app: &WorkbenchState) -> String {
    let errors = console_error_count(app);
    if errors == 0 {
        app.console_logs.len().to_string()
    } else {
        format!("{errors}!")
    }
}

fn console_error_count(app: &WorkbenchState) -> usize {
    app.console_logs
        .iter()
        .filter(|log| matches!(log.level, ConsoleLevel::Error | ConsoleLevel::Fatal))
        .count()
}

fn compact_header_spans(app: &WorkbenchState) -> Vec<Span<'static>> {
    vec![
        Span::styled(site_domain(app), Style::default().fg(GB_FG)),
        Span::styled(format!(" · {}", transient_status(app)), muted_style()),
    ]
}

fn full_header_spans(app: &WorkbenchState) -> Vec<Span<'static>> {
    vec![
        Span::styled(site_domain(app), Style::default().fg(GB_FG)),
        Span::raw("  "),
        Span::styled(transient_status(app), Style::default().fg(GB_BLUE)),
    ]
}

fn transient_status(app: &WorkbenchState) -> String {
    if app.status_updated_at.elapsed() > std::time::Duration::from_secs(5) {
        if app.cdp_websocket_url.is_some() {
            "live".to_string()
        } else {
            "idle".to_string()
        }
    } else if app.status.is_empty() {
        "idle".to_string()
    } else {
        compact_value(&app.status, 80)
    }
}

fn site_domain(app: &WorkbenchState) -> String {
    app.selected_request()
        .map(|request| domain_for_url(&request.request.url))
        .unwrap_or_else(|| domain_for_url(&app.target_url))
}

fn favicon_spans(app: &WorkbenchState) -> Vec<Span<'static>> {
    let Some((mime, data)) = captured_favicon(app) else {
        return vec![Span::styled("[icon]", muted_style()), Span::raw("  ")];
    };
    match terminal_image_protocol() {
        Some(ImageProtocol::Kitty) => vec![Span::raw(kitty_favicon_escape(data)), Span::raw(" ")],
        Some(ImageProtocol::ITerm) => vec![Span::raw(iterm_favicon_escape(data)), Span::raw(" ")],
        None => vec![
            Span::styled("[favicon]", Style::default().fg(GB_GREEN)),
            Span::raw(format!(" {mime} ")),
        ],
    }
}

fn captured_favicon(app: &WorkbenchState) -> Option<(&str, &str)> {
    app.requests.iter().find_map(|request| {
        let mime = request
            .response
            .as_ref()
            .and_then(|response| response.mime_type.as_deref())?;
        if !mime.starts_with("image/") {
            return None;
        }
        let path = path_for_url(&request.request.url).to_lowercase();
        if !(path.contains("favicon")
            || path.contains("apple-touch-icon")
            || path.ends_with(".ico"))
        {
            return None;
        }
        let body = request.response_body.as_deref()?;
        parse_image_data_url(body)
    })
}

fn render_network_bar(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let line = Line::from(vec![
        Span::styled("filter", label_style()),
        Span::raw(" "),
        Span::raw(if app.request_filter.is_empty() {
            "all".to_string()
        } else {
            app.request_filter.clone()
        }),
        Span::styled("   preset ", muted_style()),
        Span::raw(app.active_filter_preset_label().unwrap_or("-")),
        Span::styled("   split ", muted_style()),
        Span::raw(format!(
            "{}:{} / {}:{}",
            app.requests_percent,
            100 - app.requests_percent,
            app.detail_percent,
            100 - app.detail_percent
        )),
        Span::styled("   density ", muted_style()),
        Span::raw(app.density_mode.label()),
        Span::styled("   sql ", muted_style()),
        Span::raw(
            app.sql_request_filter_ids
                .as_ref()
                .map(|ids| ids.len().to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::styled("   route ", muted_style()),
        Span::raw(
            app.active_request_route_breadcrumb()
                .map(|route| compact_value(&route, 56))
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::raw("   "),
        route_summary_span(app, 64),
    ]);
    frame.render_widget(Paragraph::new(line).style(Style::default().fg(GB_FG)), area);
}

fn render_network_compact_bar(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let stats = RequestStats::from(app);
    let mut traffic_line = status_meter_line(&stats);
    let mut spans = vec![
        Span::styled("filter ", label_style()),
        Span::raw(if app.request_filter.is_empty() {
            "all".to_string()
        } else {
            compact_value(&app.request_filter, 36)
        }),
        Span::styled("  preset ", muted_style()),
        Span::raw(app.active_filter_preset_label().unwrap_or("-")),
        Span::styled("  sql ", muted_style()),
        Span::raw(
            app.sql_request_filter_ids
                .as_ref()
                .map(|ids| ids.len().to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::styled("  route ", muted_style()),
        Span::raw(
            app.active_request_route_breadcrumb()
                .map(|route| compact_value(&route, 36))
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::raw("  "),
        route_summary_span(app, 42),
        Span::raw("  "),
    ];
    spans.append(&mut traffic_line.spans);
    spans.extend([
        Span::raw("  "),
        Span::styled("lat ", muted_style()),
        Span::raw(
            stats
                .avg_duration_ms
                .map(|value| format!("{value}ms"))
                .unwrap_or_else(|| "-".to_string()),
        ),
        Span::styled("  slow ", muted_style()),
        Span::raw(stats.slow.to_string()),
        Span::styled("  bytes ", muted_style()),
        Span::raw(format_bytes(stats.total_size)),
        Span::styled("  split ", muted_style()),
        Span::raw(format!(
            "{}:{}",
            app.requests_percent,
            100 - app.requests_percent
        )),
    ]);
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().fg(GB_FG)),
        area,
    );
}

fn render_stats_panel(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let stats = RequestStats::from(app);
    let mut traffic_line = status_meter_line(&stats);
    traffic_line.spans.extend([
        Span::raw("  "),
        Span::styled("pending ", muted_style()),
        Span::raw(stats.pending.to_string()),
        Span::raw("  "),
        Span::styled("replayed ", muted_style()),
        Span::raw(stats.replayed.to_string()),
    ]);
    let lines = vec![
        traffic_line,
        Line::from(vec![
            Span::styled("latency ", muted_style()),
            Span::raw(
                stats
                    .avg_duration_ms
                    .map(|value| format!("avg {value}ms"))
                    .unwrap_or_else(|| "avg -".to_string()),
            ),
            Span::raw("  "),
            Span::raw(
                stats
                    .max_duration_ms
                    .map(|value| format!("max {value}ms"))
                    .unwrap_or_else(|| "max -".to_string()),
            ),
            Span::raw("  "),
            Span::styled("slow ", muted_style()),
            Span::raw(stats.slow.to_string()),
            Span::raw("  "),
            Span::styled("bytes ", muted_style()),
            Span::raw(format_bytes(stats.total_size)),
        ]),
    ];
    let lines = if app.active_route_summary().is_some() {
        let mut lines = lines;
        lines.push(Line::from(vec![route_summary_span(app, 120)]));
        lines
    } else {
        lines
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block("Signal", false))
            .style(Style::default().fg(GB_FG)),
        area,
    );
}

fn render_requests(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
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
                let resource_type = request
                    .request
                    .resource_type
                    .clone()
                    .unwrap_or_else(|| "-".to_string());
                let resource_label = resource_label(&resource_type);
                let tree_meta = app.request_tree_meta(*index);
                let can_drill_down = app.request_can_drill_down(*index);
                let domain = domain_for_url(&request.request.url);
                let path = app
                    .request_route_remainder(*index)
                    .unwrap_or_else(|| path_for_url(&request.request.url));
                Some(
                    Row::new([
                        Cell::from(request_tree_marker(
                            row_index,
                            total,
                            tree_meta.as_ref(),
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
                        Cell::from(resource_type_line(
                            request,
                            resource_label,
                            &highlight_terms,
                            fade,
                            theme,
                        ))
                        .style(resource_style(
                            &resource_type,
                            fade,
                            theme,
                        )),
                        Cell::from(highlight_text(&domain, &highlight_terms))
                            .style(fade.secondary_style(theme)),
                        Cell::from(highlight_text(&path, &highlight_terms)).style(base_style),
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
}

fn visible_request_rows(area: Rect) -> usize {
    // Border top/bottom plus a one-line header.
    area.height.saturating_sub(3).max(1) as usize
}

fn request_window_start(selected: usize, visible_rows: usize, total: usize) -> usize {
    if total <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows / 2)
        .min(total.saturating_sub(visible_rows))
}

fn visible_request_table_state(app: &WorkbenchState, visible_rows: usize) -> TableState {
    let total = app.filtered_request_rows.len();
    let selected = app
        .table_state
        .selected()
        .map(|selected| selected.min(total.saturating_sub(1)));
    let visible_selected = selected.map(|selected| {
        selected.saturating_sub(request_window_start(selected, visible_rows, total))
    });
    TableState::default().with_selected(visible_selected)
}

fn render_detail(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let lines = detail_lines(app);

    let paragraph = Paragraph::new(lines)
        .block(themed_panel_block(
            detail_title(app),
            Some('D'),
            app.focus == FocusPane::Detail,
            &app.config.theme,
        ))
        .style(Style::default().fg(app.config.theme.text))
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_body(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let lines = response_body_panel_lines(app);
    let paragraph = Paragraph::new(lines)
        .block(themed_panel_block(
            response_body_title(app),
            Some('B'),
            app.focus == FocusPane::Body,
            &app.config.theme,
        ))
        .style(Style::default().fg(app.config.theme.text))
        .scroll((app.body_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_console(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    if app.console_logs.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::styled("No console logs captured yet.", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("o ", key_style()),
                Span::raw("open browser and attach CDP"),
            ]),
            Line::from(vec![
                Span::styled("e ", key_style()),
                Span::raw("open editor, evaluate JavaScript in the page"),
                Span::raw("  "),
                Span::styled("c ", key_style()),
                Span::raw("clear"),
            ]),
        ])
        .block(panel_block("Console", app.focus == FocusPane::Console))
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    } else {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(8)])
            .split(area);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(root[1]);

        frame.render_widget(
            Paragraph::new(console_summary_lines(app)).style(Style::default().fg(GB_FG)),
            root[0],
        );
        render_console_stream(frame, body[0], app);
        render_console_detail(frame, body[1], app);
    }
}

fn render_console_stream(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let items = app
        .filtered_console_indices
        .iter()
        .filter_map(|index| app.console_logs.get(*index))
        .map(console_stream_item)
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel_block(
            console_stream_title(app),
            app.focus == FocusPane::Console,
        ))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut app.console_state);
}

fn render_console_detail(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let selected = app.selected_console_log();

    let lines = selected
        .map(console_detail_lines)
        .unwrap_or_else(|| vec![Line::styled("No console entry selected.", muted_style())]);
    let title = selected
        .map(console_detail_title)
        .unwrap_or_else(|| "Console Detail".to_string());
    let paragraph = Paragraph::new(lines)
        .block(panel_block(title, app.focus == FocusPane::Console))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn console_summary_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let errors = app
        .console_logs
        .iter()
        .filter(|log| matches!(log.level, ConsoleLevel::Error | ConsoleLevel::Fatal))
        .count();
    let warnings = app
        .console_logs
        .iter()
        .filter(|log| matches!(log.level, ConsoleLevel::Warning))
        .count();
    let evals = app
        .console_logs
        .iter()
        .filter(|log| log.source.as_deref() == Some("faro-console"))
        .count();

    vec![
        Line::from(vec![
            Span::styled("console ", label_style()),
            Span::raw(format!(
                "{}/{} events",
                app.filtered_console_indices.len(),
                app.console_logs.len()
            )),
            Span::styled("  errors ", label_style()),
            Span::styled(
                errors.to_string(),
                if errors == 0 {
                    Style::default().fg(GB_FG)
                } else {
                    console_style(&ConsoleLevel::Error)
                },
            ),
            Span::styled("  warnings ", label_style()),
            Span::styled(
                warnings.to_string(),
                if warnings == 0 {
                    Style::default().fg(GB_FG)
                } else {
                    console_style(&ConsoleLevel::Warning)
                },
            ),
            Span::styled("  evals ", label_style()),
            Span::raw(evals.to_string()),
        ]),
        Line::from(vec![
            Span::styled("e ", key_style()),
            Span::raw("evaluate JS  "),
            Span::styled("c ", key_style()),
            Span::raw("clear visible console  "),
            Span::styled("j/k ", key_style()),
            Span::raw("select  "),
            Span::styled("/", key_style()),
            Span::raw("filter"),
        ]),
    ]
}

fn console_stream_title(app: &WorkbenchState) -> String {
    if app.console_filter.is_empty() {
        "Console Stream".to_string()
    } else {
        format!(
            "Console Stream /{} ({}/{})",
            app.console_filter,
            app.filtered_console_indices.len(),
            app.console_logs.len()
        )
    }
}

fn console_stream_item(log: &ConsoleLog) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(
            format!("{:<5}", console_level_label(&log.level)),
            console_style(&log.level),
        ),
        Span::raw(" "),
        Span::styled(
            console_stream_message(log),
            console_message_style(&log.level),
        ),
        Span::styled(
            log.source
                .as_deref()
                .map(|source| format!("  {source}"))
                .unwrap_or_default(),
            muted_style(),
        ),
    ]))
}

fn console_stream_message(log: &ConsoleLog) -> String {
    if let Some((expression, _)) = console_eval_parts(log) {
        return format!("eval {}", compact_value(expression, 88));
    }

    compact_value(log.message.lines().next().unwrap_or_default(), 96)
}

fn console_detail_title(log: &ConsoleLog) -> String {
    format!("Console Detail {}", console_level_label(&log.level))
}

fn console_detail_lines(log: &ConsoleLog) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("level ", label_style()),
            Span::styled(console_level_label(&log.level), console_style(&log.level)),
            Span::styled("  source ", label_style()),
            Span::raw(log.source.clone().unwrap_or_else(|| "-".to_string())),
        ]),
        Line::from(vec![
            Span::styled("time ", label_style()),
            Span::raw(log.ts.to_string()),
            Span::styled("  line ", label_style()),
            Span::raw(
                log.line
                    .map(|line| line.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]),
        Line::raw(""),
    ];

    lines.extend(console_log_lines(log));

    if let Some(stack) = &log.stack_json {
        lines.push(Line::raw(""));
        lines.push(Line::styled("stack", label_style()));
        lines.extend(console_stack_lines(stack));
    }

    lines
}

fn console_stack_lines(stack: &serde_json::Value) -> Vec<Line<'static>> {
    let Some(frames) = stack_frames(stack) else {
        return syntax_body_lines(
            serde_json::to_string_pretty(stack).unwrap_or_else(|_| stack.to_string()),
        );
    };
    if frames.is_empty() {
        return vec![Line::styled("  no frames", muted_style())];
    }

    frames
        .iter()
        .take(24)
        .enumerate()
        .map(|(index, frame)| {
            let function = json_string_field(frame, "functionName")
                .or_else(|| json_string_field(frame, "function"))
                .unwrap_or("(anonymous)");
            let url = json_string_field(frame, "url")
                .or_else(|| json_string_field(frame, "scriptId"))
                .unwrap_or("-");
            let line = json_i64_field(frame, "lineNumber")
                .or_else(|| json_i64_field(frame, "line"))
                .map(|value| value + 1);
            let column = json_i64_field(frame, "columnNumber")
                .or_else(|| json_i64_field(frame, "column"))
                .map(|value| value + 1);
            Line::from(vec![
                Span::styled(format!("{:>2} ", index + 1), muted_style()),
                Span::styled(compact_value(function, 36), Style::default().fg(GB_AQUA)),
                Span::styled("  at ", muted_style()),
                Span::styled(compact_value(url, 72), Style::default().fg(GB_FG)),
                Span::styled(
                    match (line, column) {
                        (Some(line), Some(column)) => format!(":{line}:{column}"),
                        (Some(line), None) => format!(":{line}"),
                        _ => String::new(),
                    },
                    muted_style(),
                ),
            ])
        })
        .collect()
}

fn stack_frames(stack: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    stack
        .get("callFrames")
        .and_then(serde_json::Value::as_array)
        .or_else(|| {
            stack
                .get("stack")
                .and_then(|value| value.get("callFrames"))
                .and_then(serde_json::Value::as_array)
        })
        .or_else(|| stack.as_array())
}

fn json_string_field<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(serde_json::Value::as_str)
}

fn json_i64_field(value: &serde_json::Value, field: &str) -> Option<i64> {
    value.get(field).and_then(serde_json::Value::as_i64)
}

fn render_websockets(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    if app.websocket_frames.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::styled("No WebSocket frames captured yet.", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("o ", key_style()),
                Span::raw("open browser and attach CDP"),
            ]),
            Line::from(vec![
                Span::styled("/", key_style()),
                Span::raw("filter frames  "),
                Span::styled("j/k ", key_style()),
                Span::raw("select"),
            ]),
        ])
        .block(panel_block(
            "WebSockets",
            app.focus == FocusPane::WebSockets,
        ))
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
        return;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(root[1]);

    frame.render_widget(
        Paragraph::new(websocket_summary_lines(app)).style(Style::default().fg(GB_FG)),
        root[0],
    );
    render_websocket_stream(frame, body[0], app);
    render_websocket_detail(frame, body[1], app);
}

fn render_websocket_stream(frame: &mut ratatui::Frame, area: Rect, app: &mut WorkbenchState) {
    let items = app
        .filtered_websocket_indices
        .iter()
        .filter_map(|index| app.websocket_frames.get(*index))
        .map(websocket_stream_item)
        .collect::<Vec<_>>();
    let title = if app.websocket_filter.is_empty() {
        format!(
            "WebSocket Stream {}/{}",
            app.filtered_websocket_indices.len(),
            app.websocket_frames.len()
        )
    } else {
        format!(
            "WebSocket Stream /{} ({}/{})",
            app.websocket_filter,
            app.filtered_websocket_indices.len(),
            app.websocket_frames.len()
        )
    };
    let list = List::new(items)
        .block(panel_block(title, app.focus == FocusPane::WebSockets))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut app.websocket_state);
}

fn render_websocket_detail(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let selected = app.selected_websocket_frame();
    let lines = selected
        .map(websocket_detail_lines)
        .unwrap_or_else(|| vec![Line::styled("No frame selected.", muted_style())]);
    let title = selected
        .map(|frame| {
            format!(
                "Frame {} {}",
                direction_label(frame),
                websocket_opcode_label(frame.opcode)
            )
        })
        .unwrap_or_else(|| "Frame Detail".to_string());
    frame.render_widget(
        Paragraph::new(faded_lines(
            lines,
            app.websocket_detail_scroll,
            area,
            &app.config.theme,
            app.config.ui.bottom_fade_rows,
        ))
        .block(panel_block(title, app.focus == FocusPane::WebSockets))
        .scroll((app.websocket_detail_scroll, 0))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn websocket_summary_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let sent = app
        .websocket_frames
        .iter()
        .filter(|frame| matches!(frame.direction, WebSocketFrameDirection::Sent))
        .count();
    let received = app.websocket_frames.len().saturating_sub(sent);
    let bytes = app
        .websocket_frames
        .iter()
        .map(|frame| frame.payload.len())
        .sum::<usize>();
    let connections = app
        .websocket_frames
        .iter()
        .map(|frame| frame.browser_request_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();

    vec![
        Line::from(vec![
            Span::styled("frames ", label_style()),
            Span::raw(format!(
                "{}/{}",
                app.filtered_websocket_indices.len(),
                app.websocket_frames.len()
            )),
            Span::styled("  conns ", label_style()),
            Span::raw(connections.to_string()),
            Span::styled("  in ", label_style()),
            Span::styled(received.to_string(), Style::default().fg(GB_BLUE)),
            Span::styled("  out ", label_style()),
            Span::styled(sent.to_string(), Style::default().fg(GB_GREEN)),
            Span::styled("  payload ", label_style()),
            Span::raw(format_bytes(bytes as i64)),
        ]),
        Line::from(vec![
            Span::styled("j/k ", key_style()),
            Span::raw("select  "),
            Span::styled("u/d ", key_style()),
            Span::raw("scroll payload  "),
            Span::styled("/", key_style()),
            Span::raw("filter  "),
            Span::styled("g/G ", key_style()),
            Span::raw("top/bottom"),
        ]),
    ]
}

fn websocket_stream_item(frame: &WebSocketFrameRecord) -> ListItem<'static> {
    let direction_style = match frame.direction {
        WebSocketFrameDirection::Sent => Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD),
        WebSocketFrameDirection::Received => {
            Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD)
        }
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{:<3}", direction_label(frame)), direction_style),
        Span::raw(" "),
        Span::styled(
            format!("{:<6}", websocket_opcode_label(frame.opcode)),
            Style::default().fg(GB_YELLOW),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:>7}", format_bytes(frame.payload.len() as i64)),
            muted_style(),
        ),
        Span::raw(" "),
        Span::raw(compact_value(&frame.payload, 80)),
    ]))
}

fn websocket_detail_lines(frame: &WebSocketFrameRecord) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("direction ", label_style()),
            Span::raw(direction_label(frame).to_string()),
            Span::styled("  opcode ", label_style()),
            Span::raw(format!(
                "{} ({})",
                frame.opcode,
                websocket_opcode_label(frame.opcode)
            )),
            Span::styled("  size ", label_style()),
            Span::raw(format_bytes(frame.payload.len() as i64)),
        ]),
        Line::from(vec![
            Span::styled("time ", label_style()),
            Span::raw(frame.ts.to_string()),
            Span::styled("  request ", label_style()),
            Span::raw(frame.browser_request_id.clone()),
            Span::styled("  mask ", label_style()),
            Span::raw(if frame.mask { "yes" } else { "no" }),
        ]),
        Line::raw(""),
        Line::styled("payload", label_style()),
    ];
    lines.extend(format_websocket_payload(frame));
    lines
}

fn format_websocket_payload(frame: &WebSocketFrameRecord) -> Vec<Line<'static>> {
    let payload = frame.payload.clone();
    if websocket_opcode_label(frame.opcode) == "text"
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload)
    {
        return syntax_body_lines(
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| payload.clone()),
        );
    }
    payload
        .lines()
        .map(|line| Line::raw(line.to_string()))
        .collect::<Vec<_>>()
}

fn direction_label(frame: &WebSocketFrameRecord) -> &'static str {
    match frame.direction {
        WebSocketFrameDirection::Sent => "out",
        WebSocketFrameDirection::Received => "in",
    }
}

fn render_scripts(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    frame.render_widget(
        Paragraph::new(script_list_lines(app))
            .block(panel_block(
                format!("Scripts {}", app.scripts.len()),
                app.focus == FocusPane::Scripts,
            ))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(script_output_lines(app))
            .block(panel_block("Output", app.focus == FocusPane::Scripts))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn script_list_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    if app.scripts.is_empty() {
        return vec![
            Line::styled("no scripts yet", muted_style()),
            Line::raw(""),
            Line::from(vec![
                Span::styled("n", key_style()),
                Span::raw(" new  "),
                Span::styled("p", key_style()),
                Span::raw(" palette"),
            ]),
        ];
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("n", key_style()),
        Span::raw(" new  "),
        Span::styled("e", key_style()),
        Span::raw(" edit  "),
        Span::styled("r", key_style()),
        Span::raw(" run  "),
        Span::styled("R", key_style()),
        Span::raw(" rename  "),
        Span::styled("D", key_style()),
        Span::raw(" dup  "),
        Span::styled("x", key_style()),
        Span::raw(" delete"),
    ])];
    lines.push(Line::raw(""));
    lines.extend(app.scripts.iter().enumerate().map(|(index, script)| {
        let selected = app.script_state.selected() == Some(index);
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(GB_YELLOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(GB_FG)
        };
        Line::from(vec![
            Span::styled(if selected { "> " } else { "  " }, muted_style()),
            Span::styled(script.name.clone(), style),
            Span::styled("  ", muted_style()),
            Span::styled(
                script
                    .last_run_at
                    .map(|ts| format!("last {ts}"))
                    .unwrap_or_else(|| "never run".to_string()),
                muted_style(),
            ),
        ])
    }));
    lines
}

fn script_output_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(script) = app.selected_script() {
        lines.push(Line::from(vec![
            Span::styled("script ", label_style()),
            Span::raw(script.name.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("updated ", label_style()),
            Span::raw(script.updated_at.to_string()),
            Span::styled("  chars ", label_style()),
            Span::raw(script.body.len().to_string()),
        ]));
    }
    if let Some(status) = &app.script_status {
        lines.push(Line::from(vec![
            Span::styled("status ", label_style()),
            Span::raw(status.clone()),
        ]));
    }
    if let Some(duration) = app.script_duration_ms {
        lines.push(Line::from(vec![
            Span::styled("duration ", label_style()),
            Span::raw(format!("{duration}ms")),
        ]));
    }
    if !lines.is_empty() {
        lines.push(Line::raw(""));
    }
    if let Some(script) = app.selected_script() {
        lines.push(Line::from(vec![
            Span::styled("source ", label_style()),
            Span::styled("e to edit", key_style()),
        ]));
        lines.extend(script_source_lines(&script.body, 80));
        lines.push(Line::raw(""));
    }
    if app.script_output.is_empty() {
        lines.push(Line::styled("no output", muted_style()));
    } else {
        lines.push(Line::styled("output", label_style()));
        lines.extend(app.script_output.iter().map(|line| Line::raw(line.clone())));
    }
    lines
}

fn script_source_lines(source: &str, max_lines: usize) -> Vec<Line<'static>> {
    source
        .lines()
        .take(max_lines)
        .map(highlight_javascript_line)
        .collect()
}

fn render_storage(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
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

fn render_cookies(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
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

fn render_status(frame: &mut ratatui::Frame, area: Rect, app: &WorkbenchState) {
    let keys = match app.input_mode {
        InputMode::Filtering => filter_help_line(),
        InputMode::BodySearch => body_search_help_line(app),
        _ => compact_help_line(app),
    };
    let mut status_spans = vec![
        Span::styled("status ", label_style()),
        Span::raw(transient_status(app)),
    ];

    if app.view == WorkbenchView::Network {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("sort ", label_style()),
            Span::raw(format!(
                "{}{}",
                app.sort_mode.label(),
                if app.sort_descending { " desc" } else { " asc" }
            )),
        ]);
    }

    let active_filters = active_filter_count(app);
    if active_filters > 0 {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("filters ", label_style()),
            Span::raw(active_filters.to_string()),
        ]);
    }
    if app.input_mode != InputMode::Normal {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("mode ", label_style()),
            Span::raw(app.input_mode.label()),
        ]);
    }
    if app.layout_mode != LayoutMode::Normal {
        status_spans.extend([
            Span::raw("  "),
            Span::styled("layout ", label_style()),
            Span::raw(app.layout_mode.label()),
        ]);
    }

    let status = Line::from(status_spans);
    frame.render_widget(
        Paragraph::new(vec![keys, status]).style(Style::default().fg(GB_FG)),
        area,
    );
}

fn active_filter_count(app: &WorkbenchState) -> usize {
    usize::from(!app.request_filter.is_empty())
        + usize::from(!app.console_filter.is_empty())
        + usize::from(!app.websocket_filter.is_empty())
        + usize::from(app.sql_request_filter_ids.is_some())
        + usize::from(app.active_request_route_breadcrumb().is_some())
}

fn filter_help_line() -> Line<'static> {
    Line::from(vec![
        Span::styled("type", key_style()),
        Span::raw(" live filter (substring or regex)  "),
        Span::styled("enter", key_style()),
        Span::raw(" done  "),
        Span::styled("esc", key_style()),
        Span::raw(" done  "),
        Span::styled("backspace", key_style()),
        Span::raw(" delete"),
    ])
}

fn body_search_help_line(app: &WorkbenchState) -> Line<'static> {
    Line::from(vec![
        Span::styled("body search ", label_style()),
        Span::raw(if app.body_search_query.is_empty() {
            "type to find in response body".to_string()
        } else {
            app.body_search_query.clone()
        }),
        Span::raw("  "),
        Span::styled("enter", key_style()),
        Span::raw(" done  "),
        Span::styled("esc", key_style()),
        Span::raw(" close  "),
        Span::styled("backspace", key_style()),
        Span::raw(" delete"),
    ])
}

fn compact_help_line(app: &WorkbenchState) -> Line<'static> {
    // Show the keys that matter for the pane the user is actually in.
    match app.focus {
        FocusPane::Console => key_hints(&[
            ("e", "eval"),
            ("c", "clear"),
            ("j/k", "select"),
            ("u/d", "scroll"),
            ("/", "filter"),
            ("?", "keys"),
        ]),
        FocusPane::WebSockets => key_hints(&[
            ("j/k", "select"),
            ("u/d", "payload"),
            ("g/G", "ends"),
            ("/", "filter"),
            ("?", "keys"),
        ]),
        FocusPane::Scripts => key_hints(&[
            ("n", "new"),
            ("e", "edit"),
            ("r", "run"),
            ("D", "dup"),
            ("x", "delete"),
            ("?", "keys"),
        ]),
        FocusPane::Storage | FocusPane::Cookies => key_hints(&[
            ("e", "edit"),
            ("x", "delete"),
            ("tab", "origin"),
            ("j/k", "select"),
            ("/", "filter"),
            ("?", "keys"),
        ]),
        FocusPane::Detail | FocusPane::Body => key_hints(&[
            ("h/l", "tabs"),
            ("u/d", "scroll"),
            ("g/G", "ends"),
            ("e", "editor"),
            ("Y", "copy"),
            ("/", "find"),
            ("y", "curl"),
            ("?", "keys"),
        ]),
        FocusPane::Requests => key_hints(&[
            ("j/k", "move"),
            ("enter", "route"),
            ("backspace", "up"),
            ("h/l", "tabs"),
            ("/", "filter"),
            ("r", "replay"),
            ("p", "palette"),
            ("?", "keys"),
        ]),
    }
}

fn key_hints(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(hints.len() * 3);
    for (index, (key, label)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(key.to_string(), key_style()));
        spans.push(Span::raw(format!(" {label}")));
    }
    Line::from(spans)
}

fn latency_bar(ms: i64, theme: &Theme) -> Vec<Span<'static>> {
    const WIDTH: usize = 3;
    let (filled, color) = match ms {
        ..=50 => (1, theme.ok),
        51..=100 => (1, theme.ok),
        101..=200 => (1, theme.redirect),
        201..=400 => (2, theme.client_error),
        401..=800 => (2, theme.client_error),
        801..=1_500 => (3, GB_ORANGE),
        _ => (3, theme.server_error),
    };
    vec![
        Span::styled(
            "━".repeat(filled),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─".repeat(WIDTH - filled), Style::default().fg(GB_FAINT)),
    ]
}

pub(super) fn label_style() -> Style {
    Style::default().fg(GB_YELLOW)
}

pub(super) fn key_style() -> Style {
    Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
}

pub(super) fn modal_section_style() -> Style {
    Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD)
}

pub(super) fn modal_selection_style(selected: bool) -> Style {
    if selected {
        Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
    } else {
        muted_style()
    }
}

pub(super) fn muted_style() -> Style {
    Style::default().fg(GB_MUTED)
}

fn active_border(active: bool) -> Style {
    if active {
        Style::default().fg(GB_GREEN)
    } else {
        Style::default().fg(GB_BG2)
    }
}

pub(super) fn panel_block(title: impl Into<String>, active: bool) -> Block<'static> {
    let title = title.into();
    Block::default()
        .title(Line::from(Span::styled(
            format!(" {title} "),
            panel_title_style(active),
        )))
        .borders(Borders::ALL)
        .border_style(active_border(active))
}

pub(super) fn themed_panel_block(
    title: impl Into<String>,
    hotkey: Option<char>,
    active: bool,
    theme: &Theme,
) -> Block<'static> {
    let title = title.into();
    let title_color = if active {
        theme.accent
    } else {
        theme.panel_title
    };
    let border_color = if active {
        theme.active_border
    } else {
        theme.panel_border
    };
    Block::default()
        .title(themed_title_line(&title, hotkey, title_color, theme))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
}

fn themed_title_line(
    title: &str,
    hotkey: Option<char>,
    title_color: Color,
    theme: &Theme,
) -> Line<'static> {
    let base_style = Style::default()
        .fg(title_color)
        .add_modifier(Modifier::BOLD);
    let Some(hotkey) = hotkey.map(|value| value.to_ascii_lowercase()) else {
        return Line::from(Span::styled(title.to_string(), base_style));
    };
    let Some((start, character)) = title
        .char_indices()
        .find(|(_, character)| character.to_ascii_lowercase() == hotkey)
    else {
        return Line::from(Span::styled(title.to_string(), base_style));
    };
    let end = start + character.len_utf8();
    let key_color = if title_color == theme.accent {
        theme.text
    } else {
        theme.accent
    };
    let key_style = Style::default().fg(key_color).add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(title[..start].to_string(), base_style),
        Span::styled(title[start..end].to_string(), key_style),
        Span::styled(title[end..].to_string(), base_style),
    ])
}

pub(super) fn panel_title_style(active: bool) -> Style {
    if active {
        Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD)
    }
}

struct RequestStats {
    ok: usize,
    redirect: usize,
    client: usize,
    server: usize,
    pending: usize,
    replayed: usize,
    slow: usize,
    total_size: i64,
    avg_duration_ms: Option<i64>,
    max_duration_ms: Option<i64>,
}

impl RequestStats {
    fn from(app: &WorkbenchState) -> Self {
        let mut stats = Self {
            ok: 0,
            redirect: 0,
            client: 0,
            server: 0,
            pending: 0,
            replayed: 0,
            slow: 0,
            total_size: 0,
            avg_duration_ms: None,
            max_duration_ms: None,
        };
        let mut duration_total = 0_i64;
        let mut duration_count = 0_i64;

        for request in &app.requests {
            match request.status_code() {
                Some(200..=299) => stats.ok += 1,
                Some(300..=399) => stats.redirect += 1,
                Some(400..=499) => stats.client += 1,
                Some(500..=599) => stats.server += 1,
                None => stats.pending += 1,
                Some(_) => {}
            }
            if !request.replays.is_empty() {
                stats.replayed += 1;
            }
            if let Some(size) = request
                .response
                .as_ref()
                .and_then(|response| response.body_size)
            {
                stats.total_size += size;
            }
            if let Some(duration) = request.duration_ms() {
                duration_total += duration;
                duration_count += 1;
                stats.max_duration_ms =
                    Some(stats.max_duration_ms.unwrap_or(duration).max(duration));
                if duration >= 500 {
                    stats.slow += 1;
                }
            }
        }
        if duration_count > 0 {
            stats.avg_duration_ms = Some(duration_total / duration_count);
        }

        stats
    }
}

fn status_meter_line(stats: &RequestStats) -> Line<'static> {
    let total = stats.ok + stats.redirect + stats.client + stats.server + stats.pending;
    if total == 0 {
        return Line::from(vec![
            Span::styled("traffic ", muted_style()),
            Span::styled("no traffic", Style::default().fg(GB_BG2)),
        ]);
    }

    let mut spans = vec![Span::styled("traffic ", muted_style())];
    for (index, (label, count, color)) in [
        ("2xx", stats.ok, GB_GREEN),
        ("3xx", stats.redirect, GB_BLUE),
        ("4xx", stats.client, GB_YELLOW),
        ("5xx", stats.server, GB_RED),
        ("...", stats.pending, GB_MUTED),
    ]
    .into_iter()
    .filter(|(_, count, _)| *count > 0)
    .enumerate()
    {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(format!("{label} "), muted_style()));
        spans.extend(segment_bar(count, total, 8, color));
    }
    Line::from(spans)
}

fn segment_bar(count: usize, total: usize, width: usize, color: Color) -> Vec<Span<'static>> {
    let filled = (count * width).div_ceil(total);
    vec![
        Span::styled("━".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "─".repeat(width.saturating_sub(filled)),
            Style::default().fg(GB_BG2),
        ),
    ]
}

fn requests_title(app: &WorkbenchState) -> String {
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

fn route_summary_span(app: &WorkbenchState, max_width: usize) -> Span<'static> {
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

fn filter_highlight_terms(filter: &str) -> Vec<String> {
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

fn highlight_text(value: &str, terms: &[String]) -> Line<'static> {
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

fn console_level_label(level: &ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Trace => "trace",
        ConsoleLevel::Debug => "debug",
        ConsoleLevel::Info => "info",
        ConsoleLevel::Warning => "warn",
        ConsoleLevel::Error => "error",
        ConsoleLevel::Fatal => "fatal",
    }
}

fn console_style(level: &ConsoleLevel) -> Style {
    match level {
        ConsoleLevel::Warning => Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD),
        ConsoleLevel::Error | ConsoleLevel::Fatal => {
            Style::default().fg(GB_RED).add_modifier(Modifier::BOLD)
        }
        ConsoleLevel::Debug | ConsoleLevel::Trace => muted_style(),
        ConsoleLevel::Info => Style::default().fg(GB_GREEN),
    }
}

fn console_log_lines(log: &ConsoleLog) -> Vec<Line<'static>> {
    if let Some(lines) = console_eval_lines(log) {
        return lines;
    }

    let mut message_lines = log.message.lines();
    let first_message = message_lines.next().unwrap_or_default();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{:<7}", console_level_label(&log.level)),
            console_style(&log.level),
        ),
        Span::raw(" "),
        Span::styled(first_message.to_string(), console_message_style(&log.level)),
        Span::styled(
            log.source
                .as_deref()
                .map(|source| format!("  {source}"))
                .unwrap_or_default(),
            muted_style(),
        ),
    ])];

    for line in message_lines {
        lines.push(Line::from(vec![
            Span::styled("        ", muted_style()),
            Span::styled(line.to_string(), console_message_style(&log.level)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::styled("(empty)", muted_style()));
    }

    lines
}

fn console_eval_lines(log: &ConsoleLog) -> Option<Vec<Line<'static>>> {
    let (expression, result) = console_eval_parts(log)?;

    let mut lines = vec![Line::from(vec![
        Span::styled("eval   ", console_style(&log.level)),
        Span::styled(" faro-console", muted_style()),
    ])];

    for (index, line) in expression.lines().enumerate() {
        let prompt = if index == 0 { "> " } else { "| " };
        lines.push(prefixed_line(
            Span::styled(prompt, key_style()),
            highlight_javascript_line(line),
        ));
    }

    if result.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("< ", muted_style()),
            Span::styled("undefined", muted_style()),
        ]));
        return Some(lines);
    }

    for line in console_result_lines(result, &log.level) {
        lines.push(prefixed_line(Span::styled("< ", muted_style()), line));
    }

    Some(lines)
}

fn console_eval_parts(log: &ConsoleLog) -> Option<(&str, &str)> {
    if log.source.as_deref() != Some("faro-console") || !log.message.starts_with("> ") {
        return None;
    }

    Some(
        log.message
            .split_once('\n')
            .map(|(expression, result)| (expression.trim_start_matches("> "), result))
            .unwrap_or_else(|| (log.message.trim_start_matches("> "), "")),
    )
}

fn console_result_lines(result: &str, level: &ConsoleLevel) -> Vec<Line<'static>> {
    if serde_json::from_str::<serde_json::Value>(result).is_ok() {
        return syntax_body_lines(result.to_string());
    }

    result
        .lines()
        .map(|line| {
            if matches!(level, ConsoleLevel::Error | ConsoleLevel::Fatal)
                || line.starts_with("Error:")
                || line.starts_with("TypeError:")
                || line.starts_with("ReferenceError:")
                || line.starts_with("SyntaxError:")
            {
                Line::styled(line.to_string(), console_style(level))
            } else {
                highlight_javascript_line(line)
            }
        })
        .collect()
}

fn prefixed_line(prefix: Span<'static>, mut line: Line<'static>) -> Line<'static> {
    let mut spans = vec![prefix];
    spans.append(&mut line.spans);
    Line::from(spans)
}

fn console_message_style(level: &ConsoleLevel) -> Style {
    match level {
        ConsoleLevel::Error | ConsoleLevel::Fatal => Style::default().fg(GB_RED),
        ConsoleLevel::Warning => Style::default().fg(GB_YELLOW),
        ConsoleLevel::Debug | ConsoleLevel::Trace => muted_style(),
        ConsoleLevel::Info => Style::default().fg(GB_FG),
    }
}

#[derive(Clone, Copy)]
enum RowFade {
    Full,
    Soft,
    Dim,
    Ghost,
}

impl RowFade {
    fn base_style(self, theme: &Theme) -> Style {
        self.fg(theme.text)
    }

    fn secondary_style(self, theme: &Theme) -> Style {
        self.fg(theme.muted)
    }

    fn accent_style(self, color: Color) -> Style {
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

fn bottom_overlay_fade(
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

fn status_style(status: Option<i64>, fade: RowFade, theme: &Theme) -> Style {
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

fn request_tree_marker(
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

fn method_style(method: &str, fade: RowFade, theme: &Theme) -> Style {
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

fn resource_style(resource_type: &str, fade: RowFade, theme: &Theme) -> Style {
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

fn resource_label(resource_type: &str) -> &'static str {
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

fn resource_type_line(
    request: &RequestView,
    resource_label: &str,
    highlight_terms: &[String],
    fade: RowFade,
    theme: &Theme,
) -> Line<'static> {
    let mut line = highlight_text(resource_label, highlight_terms);
    let mut badges = Vec::new();
    if request.request.request_body_ref.is_some()
        || request
            .response
            .as_ref()
            .and_then(|response| response.body_size)
            .is_some_and(|size| size > 0)
    {
        badges.push(("b", theme.resource_image));
    }
    if !request.replays.is_empty() {
        badges.push(("r", theme.method_post));
    }
    if request
        .response
        .as_ref()
        .is_some_and(|response| response.body_truncated)
    {
        badges.push(("!", theme.client_error));
    }
    if matches!(request.status_code(), Some(300..=399)) {
        badges.push(("→", theme.redirect));
    }
    if !badges.is_empty() {
        line.spans.push(Span::raw(" "));
        for (badge, color) in badges {
            line.spans.push(Span::styled(
                badge.to_string(),
                fade.accent_style(color).add_modifier(Modifier::BOLD),
            ));
        }
    }
    line
}

fn duration_style(duration: Option<i64>, fade: RowFade, theme: &Theme) -> Style {
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

fn status_text(request: &RequestView) -> String {
    request
        .status_code()
        .map(|status| status.to_string())
        .unwrap_or_else(|| "---".to_string())
}

fn format_bytes(bytes: i64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}mb", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1}kb", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}b")
    }
}

fn detail_title(app: &WorkbenchState) -> String {
    let Some(request) = app.selected_request() else {
        return format!(" Request Detail [{}] ", app.detail_tab.label());
    };
    let mode = if is_image_request(request) {
        "image"
    } else if is_sse_request(request) {
        "sse"
    } else {
        app.detail_tab.label()
    };
    format!(
        " Detail [{mode}] {} {} ",
        request.request.method,
        compact_value(&path_for_url(&request.request.url), 48)
    )
}

fn response_body_title(app: &WorkbenchState) -> String {
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

fn cookie_count(app: &WorkbenchState) -> usize {
    app.cookie_events.len()
        + app
            .cookie_snapshots
            .last()
            .map(|snapshot| snapshot.cookies.len())
            .unwrap_or(0)
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

fn faded_lines(
    lines: Vec<Line<'static>>,
    scroll: u16,
    area: Rect,
    theme: &Theme,
    fade_rows: usize,
) -> Vec<Line<'static>> {
    let visible_rows = pane_visible_rows(area);
    let offset = usize::from(scroll);
    let has_more_below = offset + visible_rows < lines.len();
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let fade = bottom_overlay_fade(index, offset, visible_rows, has_more_below, fade_rows);
            if matches!(fade, RowFade::Full) {
                line
            } else {
                line.patch_style(fade.base_style(theme))
            }
        })
        .collect()
}

fn pane_visible_rows(area: Rect) -> usize {
    area.height.saturating_sub(2).max(1) as usize
}

fn selected_window_start(selected: usize, visible_rows: usize, total: usize) -> usize {
    if total <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows / 2)
        .min(total.saturating_sub(visible_rows))
}

fn plain_value_lines(value: &str) -> Vec<Line<'static>> {
    value
        .lines()
        .map(|line| Line::styled(line.to_string(), Style::default().fg(GB_FG)))
        .collect()
}

pub(super) fn compact_value(value: &str, max_chars: usize) -> String {
    let normalized = value.replace(['\n', '\r', '\t'], " ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    if max_chars <= 3 {
        return normalized.chars().take(max_chars).collect();
    }

    let mut compact = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    compact.push_str("...");
    compact
}

#[derive(Clone, Copy)]
enum BodySyntax {
    Json,
    Html,
    Css,
    JavaScript,
    Xml,
    Text,
}

fn syntax_body_lines(body: String) -> Vec<Line<'static>> {
    syntax_body_lines_with(body, BodySyntax::Json)
}

fn syntax_body_lines_for_request(request: &RequestView, body: String) -> Vec<Line<'static>> {
    syntax_body_lines_with(body, body_syntax_for_request(request))
}

fn syntax_body_lines_with(body: String, syntax: BodySyntax) -> Vec<Line<'static>> {
    match syntax {
        BodySyntax::Json if serde_json::from_str::<serde_json::Value>(&body).is_ok() => {
            body.lines().map(highlight_json_line).collect()
        }
        BodySyntax::Html => body.lines().map(highlight_html_line).collect(),
        BodySyntax::Css => body.lines().map(highlight_css_line).collect(),
        BodySyntax::JavaScript => body.lines().map(highlight_javascript_line).collect(),
        BodySyntax::Xml => body.lines().map(highlight_html_line).collect(),
        BodySyntax::Json | BodySyntax::Text => body
            .lines()
            .map(|line| Line::styled(line.to_string(), Style::default().fg(GB_FG)))
            .collect(),
    }
}

fn body_syntax_for_request(request: &RequestView) -> BodySyntax {
    let mime = request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let resource = request
        .request
        .resource_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = path_for_url(&request.request.url).to_ascii_lowercase();

    if mime.contains("json") || path.ends_with(".json") {
        BodySyntax::Json
    } else if mime.contains("html") || resource == "document" || path.ends_with(".html") {
        BodySyntax::Html
    } else if mime.contains("css") || resource == "stylesheet" || path.ends_with(".css") {
        BodySyntax::Css
    } else if mime.contains("javascript")
        || mime.contains("ecmascript")
        || resource == "script"
        || path.ends_with(".js")
        || path.ends_with(".mjs")
    {
        BodySyntax::JavaScript
    } else if mime.contains("xml") || path.ends_with(".xml") || path.ends_with(".svg") {
        BodySyntax::Xml
    } else {
        BodySyntax::Text
    }
}

fn highlight_html_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .map(|offset| index + offset + 3)
                .unwrap_or(line.len());
            spans.push(Span::styled(
                line[index..end].to_string(),
                js_comment_style(),
            ));
            index = end;
            continue;
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        if ch == '<' {
            spans.push(Span::styled("<".to_string(), json_punctuation_style()));
            index += ch.len_utf8();
            if line[index..].starts_with('/') {
                spans.push(Span::styled("/".to_string(), json_punctuation_style()));
                index += 1;
            }
            let name_start = index;
            while index < line.len() {
                let Some(next) = line[index..].chars().next() else {
                    break;
                };
                if !(next.is_ascii_alphanumeric() || matches!(next, '-' | ':' | '_' | '!')) {
                    break;
                }
                index += next.len_utf8();
            }
            if index > name_start {
                spans.push(Span::styled(
                    line[name_start..index].to_string(),
                    Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD),
                ));
            }
            continue;
        }
        if ch == '>' || ch == '/' || ch == '=' {
            spans.push(Span::styled(ch.to_string(), json_punctuation_style()));
            index += ch.len_utf8();
            continue;
        }
        if ch == '"' || ch == '\'' {
            let end = quoted_string_end(line, index, ch);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_string_style(),
            ));
            index = end;
            continue;
        }
        if is_js_ident_start(ch) {
            let end = consume_html_identifier(line, index);
            let token = &line[index..end];
            let style = if line[end..].trim_start().starts_with('=') {
                Style::default().fg(GB_AQUA)
            } else {
                Style::default().fg(GB_FG)
            };
            spans.push(Span::styled(token.to_string(), style));
            index = end;
            continue;
        }
        spans.push(Span::raw(ch.to_string()));
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn consume_html_identifier(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | ':' | '_' | '.')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn highlight_css_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("/*") {
            let end = rest
                .find("*/")
                .map(|offset| index + offset + 2)
                .unwrap_or(line.len());
            spans.push(Span::styled(
                line[index..end].to_string(),
                js_comment_style(),
            ));
            index = end;
            continue;
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        if ch == '"' || ch == '\'' {
            let end = quoted_string_end(line, index, ch);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_string_style(),
            ));
            index = end;
            continue;
        }
        if ch == '#'
            && rest
                .chars()
                .nth(1)
                .is_some_and(|next| next.is_ascii_hexdigit())
        {
            let end = consume_css_hash(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                Style::default().fg(GB_PURPLE),
            ));
            index = end;
            continue;
        }
        if ch == '@' {
            let end = consume_css_identifier(line, index + ch.len_utf8());
            spans.push(Span::styled(
                line[index..end].to_string(),
                Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD),
            ));
            index = end;
            continue;
        }
        if ch.is_ascii_digit() {
            let end = consume_css_number(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_number_style(),
            ));
            index = end;
            continue;
        }
        if is_js_ident_start(ch) || ch == '-' {
            let end = consume_css_identifier(line, index);
            let token = &line[index..end];
            let style = if line[end..].trim_start().starts_with(':') {
                Style::default().fg(GB_AQUA)
            } else {
                Style::default().fg(GB_FG)
            };
            spans.push(Span::styled(token.to_string(), style));
            index = end;
            continue;
        }
        if matches!(ch, '{' | '}' | ':' | ';' | ',' | '(' | ')' | '[' | ']') {
            spans.push(Span::styled(ch.to_string(), json_punctuation_style()));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn consume_css_identifier(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn consume_css_hash(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch == '#' || ch.is_ascii_hexdigit()) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn consume_css_number(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '%' | '-' | '+')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn highlight_javascript_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if rest.starts_with("//") {
            spans.push(Span::styled(rest.to_string(), js_comment_style()));
            break;
        }

        if ch == '"' || ch == '\'' || ch == '`' {
            let end = quoted_string_end(line, index, ch);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_string_style(),
            ));
            index = end;
            continue;
        }

        if ch.is_ascii_digit() {
            let end = consume_js_number(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_number_style(),
            ));
            index = end;
            continue;
        }

        if is_js_ident_start(ch) {
            let end = consume_js_identifier(line, index);
            let token = &line[index..end];
            spans.push(Span::styled(token.to_string(), js_identifier_style(token)));
            index = end;
            continue;
        }

        if matches!(
            ch,
            '{' | '}'
                | '['
                | ']'
                | '('
                | ')'
                | ':'
                | ','
                | '.'
                | ';'
                | '='
                | '+'
                | '-'
                | '*'
                | '/'
                | '!'
                | '?'
                | '<'
                | '>'
                | '|'
                | '&'
        ) {
            spans.push(Span::styled(ch.to_string(), json_punctuation_style()));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn quoted_string_end(line: &str, start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (offset, ch) in line[start + quote.len_utf8()..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return start + quote.len_utf8() + offset + ch.len_utf8();
        }
    }
    line.len()
}

fn consume_js_number(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_digit() || matches!(ch, '.' | '_' | 'x' | 'X' | 'a'..='f' | 'A'..='F')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn consume_js_identifier(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !is_js_ident_continue(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn is_js_ident_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_js_ident_continue(ch: char) -> bool {
    is_js_ident_start(ch) || ch.is_ascii_digit()
}

fn js_identifier_style(token: &str) -> Style {
    if js_keyword(token) {
        Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD)
    } else if js_global(token) {
        Style::default().fg(GB_AQUA)
    } else {
        Style::default().fg(GB_FG)
    }
}

fn js_keyword(token: &str) -> bool {
    matches!(
        token,
        "async"
            | "await"
            | "break"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "else"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "let"
            | "new"
            | "null"
            | "return"
            | "throw"
            | "true"
            | "try"
            | "undefined"
            | "var"
            | "while"
    )
}

fn js_global(token: &str) -> bool {
    matches!(
        token,
        "console"
            | "document"
            | "fetch"
            | "history"
            | "localStorage"
            | "location"
            | "navigator"
            | "sessionStorage"
            | "window"
    )
}

fn js_comment_style() -> Style {
    muted_style()
}

fn highlight_json_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if ch == '"' {
            let end = json_string_end(line, index);
            let token = &line[index..end];
            let is_key = line[end..].trim_start().starts_with(':');
            spans.push(Span::styled(
                token.to_string(),
                if is_key {
                    json_key_style()
                } else {
                    json_string_style()
                },
            ));
            index = end;
            continue;
        }

        if ch == '-' || ch.is_ascii_digit() {
            let end = consume_json_number(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_number_style(),
            ));
            index = end;
            continue;
        }

        if let Some((literal, style)) = json_literal(rest) {
            spans.push(Span::styled(literal.to_string(), style));
            index += literal.len();
            continue;
        }

        if matches!(ch, '{' | '}' | '[' | ']' | ':' | ',') {
            spans.push(Span::styled(ch.to_string(), json_punctuation_style()));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn json_string_end(line: &str, start: usize) -> usize {
    let mut escaped = false;
    for (offset, ch) in line[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return start + 1 + offset + ch.len_utf8();
        }
    }
    line.len()
}

fn consume_json_number(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn json_literal(input: &str) -> Option<(&'static str, Style)> {
    if input.starts_with("true") {
        Some(("true", json_literal_style()))
    } else if input.starts_with("false") {
        Some(("false", json_literal_style()))
    } else if input.starts_with("null") {
        Some(("null", json_null_style()))
    } else {
        None
    }
}

fn json_key_style() -> Style {
    Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD)
}

fn json_string_style() -> Style {
    Style::default().fg(GB_GREEN)
}

fn json_number_style() -> Style {
    Style::default().fg(GB_PURPLE)
}

fn json_literal_style() -> Style {
    Style::default().fg(GB_YELLOW)
}

fn json_null_style() -> Style {
    muted_style()
}

fn json_punctuation_style() -> Style {
    muted_style()
}

fn response_body_panel_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
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

fn body_tree_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::tui::state::{InputMode, SortMode};
    use faro_core::{
        CookieEventRecord, CookieRecord, CookieSnapshotRecord, RequestRecord, ResponseRecord,
        StorageEntry, StorageEventRecord, StorageSnapshotRecord,
    };
    use faro_store::ScriptRecord;
    use ratatui::widgets::{ListState, TableState};
    use std::path::PathBuf;

    fn state_with_storage(
        storage_snapshots: Vec<StorageSnapshotRecord>,
        storage_events: Vec<StorageEventRecord>,
    ) -> WorkbenchState {
        WorkbenchState {
            config: AppConfig::default(),
            db_path: PathBuf::from("/tmp/faro-test.db"),
            target_url: "http://localhost:5173".to_string(),
            active_session_id: None,
            requests: Vec::new(),
            request_tree_metas: Vec::new(),
            filtered_request_indices: Vec::new(),
            filtered_request_rows: Vec::new(),
            filtered_route_descendant_counts: std::collections::HashMap::new(),
            collapsed_request_groups: std::collections::HashSet::new(),
            active_request_route_group: None,
            sql_request_filter_ids: None,
            sql_request_filter_query: None,
            requests_hidden_before: None,
            console_logs: Vec::new(),
            filtered_console_indices: Vec::new(),
            console_hidden_before: None,
            websocket_frames: Vec::new(),
            filtered_websocket_indices: Vec::new(),
            websocket_state: ListState::default(),
            websocket_detail_scroll: 0,
            storage_events,
            storage_snapshots,
            storage_selected: 0,
            cookie_events: Vec::new(),
            cookie_snapshots: Vec::new(),
            cookie_selected: 0,
            scripts: Vec::new(),
            script_state: ListState::default(),
            script_output: Vec::new(),
            script_status: None,
            script_duration_ms: None,
            table_state: TableState::default(),
            console_state: ListState::default(),
            view: WorkbenchView::Network,
            focus: FocusPane::Requests,
            detail_tab: DetailTab::Overview,
            sort_mode: SortMode::Started,
            sort_descending: false,
            detail_scroll: 0,
            body_scroll: 0,
            body_tree_selected: 0,
            body_tree_selected_key: None,
            collapsed_body_nodes: std::collections::HashSet::new(),
            storage_scroll: 0,
            cookie_scroll: 0,
            input_mode: InputMode::Normal,
            layout_mode: LayoutMode::Normal,
            density_mode: DensityMode::Compact,
            requests_percent: 48,
            detail_percent: 38,
            palette_query: String::new(),
            palette_selected: 0,
            body_search_query: String::new(),
            show_help: false,
            show_theme_preview: false,
            show_perf: false,
            perf: Default::default(),
            sql_result: None,
            sql_row_scroll: 0,
            sql_col_scroll: 0,
            last_sql_query: String::new(),
            request_filter: String::new(),
            console_filter: String::new(),
            websocket_filter: String::new(),
            cdp_websocket_url: None,
            status: String::new(),
            status_updated_at: std::time::Instant::now(),
        }
    }

    fn response_request(mime: &str, resource_type: &str, url: &str) -> RequestView {
        let mut request = RequestRecord::started(
            "session".to_string(),
            Some("tab".to_string()),
            Some("run".to_string()),
            "GET",
            url,
        );
        request.resource_type = Some(resource_type.to_string());
        let mut response = ResponseRecord::received(request.id.clone());
        response.status_code = Some(200);
        response.mime_type = Some(mime.to_string());
        RequestView {
            request,
            response: Some(response),
            request_body: None,
            response_body: None,
            replays: Vec::new(),
            details_loaded: true,
        }
    }

    fn state_with_cookies(
        cookie_snapshots: Vec<CookieSnapshotRecord>,
        cookie_events: Vec<CookieEventRecord>,
    ) -> WorkbenchState {
        WorkbenchState {
            cookie_events,
            cookie_snapshots,
            ..state_with_storage(Vec::new(), Vec::new())
        }
    }

    #[test]
    fn derives_current_storage_from_snapshot_and_live_events() {
        let session_id = "session".to_string();
        let tab_id = Some("tab".to_string());
        let run_id = Some("run".to_string());
        let snapshot = StorageSnapshotRecord::new(
            session_id.clone(),
            tab_id.clone(),
            run_id.clone(),
            "http://localhost:5173".to_string(),
            "localStorage".to_string(),
            vec![
                StorageEntry::new("stale", "old"),
                StorageEntry::new("keep", "value"),
            ],
            "hash".to_string(),
        );
        let events = vec![
            StorageEventRecord::new(
                session_id.clone(),
                tab_id.clone(),
                run_id.clone(),
                "http://localhost:5173".to_string(),
                "localStorage".to_string(),
                "update".to_string(),
                Some("stale".to_string()),
                Some("old".to_string()),
                Some("new".to_string()),
            ),
            StorageEventRecord::new(
                session_id.clone(),
                tab_id.clone(),
                run_id.clone(),
                "http://localhost:5173".to_string(),
                "localStorage".to_string(),
                "remove".to_string(),
                Some("keep".to_string()),
                Some("value".to_string()),
                None,
            ),
            StorageEventRecord::new(
                session_id,
                tab_id,
                run_id,
                "http://localhost:5173".to_string(),
                "sessionStorage".to_string(),
                "set".to_string(),
                Some("token".to_string()),
                None,
                Some("abc".to_string()),
            ),
        ];

        let app = state_with_storage(vec![snapshot], events);
        let entries = app.current_storage_entries();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| {
            entry.storage_type == "localStorage" && entry.key == "stale" && entry.value == "new"
        }));
        assert!(entries.iter().any(|entry| {
            entry.storage_type == "sessionStorage" && entry.key == "token" && entry.value == "abc"
        }));
    }

    #[test]
    fn storage_clear_only_affects_matching_origin_and_type() {
        let session_id = "session".to_string();
        let tab_id = Some("tab".to_string());
        let run_id = Some("run".to_string());
        let snapshot = StorageSnapshotRecord::new(
            session_id.clone(),
            tab_id.clone(),
            run_id.clone(),
            "http://localhost:5173".to_string(),
            "localStorage".to_string(),
            vec![StorageEntry::new("gone", "1")],
            "hash".to_string(),
        );
        let other_snapshot = StorageSnapshotRecord::new(
            session_id.clone(),
            tab_id.clone(),
            run_id.clone(),
            "http://localhost:5173".to_string(),
            "sessionStorage".to_string(),
            vec![StorageEntry::new("kept", "2")],
            "hash".to_string(),
        );
        let clear = StorageEventRecord::new(
            session_id,
            tab_id,
            run_id,
            "http://localhost:5173".to_string(),
            "localStorage".to_string(),
            "clear".to_string(),
            None,
            None,
            None,
        );

        let app = state_with_storage(vec![snapshot, other_snapshot], vec![clear]);
        let entries = app.current_storage_entries();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].storage_type, "sessionStorage");
        assert_eq!(entries[0].key, "kept");
    }

    #[test]
    fn derives_current_cookies_from_snapshot_and_events() {
        let session_id = "session".to_string();
        let tab_id = Some("tab".to_string());
        let run_id = Some("run".to_string());
        let snapshot = CookieSnapshotRecord::new(
            session_id.clone(),
            tab_id.clone(),
            run_id.clone(),
            Some("http://localhost:5173".to_string()),
            vec![CookieRecord {
                name: "theme".to_string(),
                value: "light".to_string(),
                domain: "localhost".to_string(),
                path: "/".to_string(),
                expires: None,
                http_only: false,
                secure: false,
                same_site: Some("Lax".to_string()),
            }],
        );
        let event = CookieEventRecord::new(
            session_id,
            tab_id,
            run_id,
            "document.cookie",
            Some("theme".to_string()),
            Some("localhost".to_string()),
            Some("/".to_string()),
            Some("dark".to_string()),
            Some(serde_json::json!({"sameSite": "Strict"})),
        );

        let app = state_with_cookies(vec![snapshot], vec![event]);
        let cookies = app.current_cookie_entries();

        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "theme");
        assert_eq!(cookies[0].value, "dark");
        assert!(cookies[0].flags.contains("sameSite"));
    }

    #[test]
    fn syntax_body_lines_highlights_json() -> anyhow::Result<()> {
        let lines = syntax_body_lines(serde_json::to_string_pretty(&serde_json::json!({
            "name": "faro",
            "count": 3,
            "ok": true,
            "empty": null
        }))?);

        let spans = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();
        assert!(spans.iter().any(|span| span.content.as_ref() == "\"name\""));
        assert!(spans.iter().any(|span| span.content.as_ref() == "\"faro\""));
        assert!(spans.iter().any(|span| span.content.as_ref() == "3"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "true"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "null"));
        Ok(())
    }

    #[test]
    fn syntax_body_lines_leaves_plain_text_plain() {
        let lines = syntax_body_lines("not-json: true".to_string());

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content.as_ref(), "not-json: true");
    }

    #[test]
    fn syntax_body_lines_highlights_html_response() {
        let request = response_request("text/html", "document", "https://example.test/");
        let lines = syntax_body_lines_for_request(
            &request,
            r#"<main class="shell">Hello</main>"#.to_string(),
        );
        let spans = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        assert!(spans.iter().any(|span| span.content.as_ref() == "main"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "class"));
        assert!(
            spans
                .iter()
                .any(|span| span.content.as_ref() == r#""shell""#)
        );
    }

    #[test]
    fn syntax_body_lines_highlights_css_response() {
        let request = response_request("text/css", "stylesheet", "https://example.test/app.css");
        let lines = syntax_body_lines_for_request(
            &request,
            ".shell { color: #d4be98; margin: 12px; }".to_string(),
        );
        let spans = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        assert!(spans.iter().any(|span| span.content.as_ref() == "color"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "#d4be98"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "12px"));
    }

    #[test]
    fn syntax_body_lines_highlights_javascript_response() {
        let request = response_request(
            "application/javascript",
            "script",
            "https://example.test/app.js",
        );
        let lines =
            syntax_body_lines_for_request(&request, "const title = document.title;".to_string());
        let spans = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        assert!(spans.iter().any(|span| span.content.as_ref() == "const"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "document"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "title"));
    }

    #[test]
    fn response_body_content_lines_adds_line_number_gutter_when_active() {
        let mut request = response_request("application/json", "fetch", "https://example.test/api");
        request.response_body = Some("{\n  \"ok\": true\n}".to_string());

        let lines = response_body_content_lines(&request, true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  1 ");
        assert_eq!(lines[0].spans[1].content.as_ref(), "│ ");
        assert!(
            lines[1]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == r#""ok""#)
        );
    }

    #[test]
    fn console_stack_lines_formats_call_frames() {
        let stack = serde_json::json!({
            "callFrames": [{
                "functionName": "loadUser",
                "url": "https://example.test/app.js",
                "lineNumber": 41,
                "columnNumber": 7
            }]
        });

        let text = console_stack_lines(&stack)
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("loadUser"));
        assert!(text.contains("https://example.test/app.js"));
        assert!(text.contains(":42:8"));
    }

    #[test]
    fn script_output_lines_highlight_selected_script_source() {
        let mut app = state_with_storage(Vec::new(), Vec::new());
        app.scripts.push(ScriptRecord::new(
            "Example",
            "let failed = faros.requests.filter(#{ status: #{ gte: 400 } });",
        ));
        app.script_state.select(Some(0));
        let lines = script_output_lines(&app);
        let spans = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        assert!(spans.iter().any(|span| span.content.as_ref() == "source "));
        assert!(spans.iter().any(|span| span.content.as_ref() == "let"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "faros"));
        assert!(spans.iter().any(|span| span.content.as_ref() == "400"));
    }

    #[test]
    fn response_body_syntax_applies_when_inactive_too() {
        let request = response_request("text/css", "stylesheet", "https://example.test/app.css");
        let body = ".shell { color: #d4be98; }";
        let mut active_request = request;
        active_request.response_body = Some(body.to_string());

        let active = response_body_content_lines(&active_request, true);
        let inactive = response_body_content_lines(&active_request, false);

        assert!(
            active[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "color")
        );
        assert!(
            inactive[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "color")
        );
        assert_eq!(active[0].spans[0].content.as_ref(), "  1 ");
        assert_ne!(inactive[0].spans[0].content.as_ref(), "  1 ");
    }

    #[test]
    fn view_tabs_include_websockets_with_matching_shortcuts() {
        let app = state_with_storage(Vec::new(), Vec::new());
        let text = view_tabs_line(&app)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        for expected in [
            "1",
            "Net",
            "2 Console",
            "3 WS",
            "4 Scripts",
            "5 Storage",
            "6 Cookies",
        ] {
            assert!(text.contains(expected));
        }
    }

    #[test]
    fn request_tree_marker_shows_dot_only_for_rows_with_children() {
        let theme = Theme::default();
        let parent = RequestTreeMeta {
            depth: 1,
            group_key: None,
            ancestor_keys: Vec::new(),
            has_children: true,
            child_count: 2,
            collapsed: false,
        };
        let leaf = RequestTreeMeta {
            has_children: false,
            ..parent.clone()
        };
        let deep_parent = RequestTreeMeta {
            depth: 7,
            has_children: true,
            ..parent.clone()
        };

        let parent_text = request_tree_marker(0, 2, Some(&parent), true, RowFade::Full, &theme)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        let leaf_text = request_tree_marker(1, 2, Some(&leaf), false, RowFade::Full, &theme)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        let deep_parent_text =
            request_tree_marker(1, 2, Some(&deep_parent), true, RowFade::Full, &theme)
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
        let non_drillable_parent_text =
            request_tree_marker(1, 2, Some(&deep_parent), false, RowFade::Full, &theme)
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();

        assert_eq!(parent_text, "›");
        assert_eq!(leaf_text, "");
        assert!(deep_parent_text.starts_with("›"));
        assert!(!non_drillable_parent_text.contains('›'));
    }

    #[test]
    fn parse_sse_events_groups_fields() {
        let events = parse_sse_events(
            "id: 1\nevent: patch\ndata: {\"ok\":true}\n\nretry: 5000\ndata: heartbeat\n\n",
        );

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id.as_deref(), Some("1"));
        assert_eq!(events[0].event.as_deref(), Some("patch"));
        assert_eq!(events[0].data, vec![r#"{"ok":true}"#]);
        assert_eq!(events[1].retry.as_deref(), Some("5000"));
        assert_eq!(events[1].data, vec!["heartbeat"]);
    }

    #[test]
    fn console_eval_lines_render_prompt_and_result_separately() {
        let log = ConsoleLog::new(
            "session".to_string(),
            None,
            None,
            ConsoleLevel::Info,
            "> const value = await fetch('/api')\n{\"ok\":true}".to_string(),
            Some("faro-console".to_string()),
            None,
        );

        let lines = console_log_lines(&log);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert!(rendered.iter().any(|line| line.starts_with("> ")));
        assert!(rendered.iter().any(|line| line.starts_with("< ")));
        assert!(rendered.iter().any(|line| line.contains("\"ok\"")));
    }

    #[test]
    fn console_log_lines_preserve_multiline_messages() {
        let log = ConsoleLog::new(
            "session".to_string(),
            None,
            None,
            ConsoleLevel::Error,
            "first line\nsecond line".to_string(),
            Some("page".to_string()),
            None,
        );

        let lines = console_log_lines(&log);

        assert_eq!(lines.len(), 2);
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "first line")
        );
        assert!(
            lines[1]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "second line")
        );
    }
}

fn detail_lines(app: &WorkbenchState) -> Vec<Line<'static>> {
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
        DetailTab::ResponseBody
            if app.focus == FocusPane::Detail && !app.body_tree_items().is_empty() =>
        {
            body_tree_lines(app)
        }
        DetailTab::ResponseBody => response_body_lines(request, app.focus == FocusPane::Detail),
        DetailTab::Timing => timing_lines(request),
        DetailTab::Replay if !request.details_loaded => detail_not_loaded_lines(),
        DetailTab::Replay => replay_lines(request),
    }
}

fn detail_not_loaded_lines() -> Vec<Line<'static>> {
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

fn is_image_request(request: &RequestView) -> bool {
    request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .map(|mime| mime.starts_with("image/"))
        .unwrap_or(false)
}

fn image_preview_lines(request: &RequestView) -> Vec<Line<'static>> {
    let mime = request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .unwrap_or("image/*");
    let size = request
        .response
        .as_ref()
        .and_then(|response| response.body_size)
        .map(format_bytes)
        .unwrap_or_else(|| "-".to_string());
    let mut lines = vec![
        Line::styled("image preview", label_style()),
        Line::from(vec![
            Span::styled("mime ", label_style()),
            Span::raw(mime.to_string()),
            Span::styled("  size ", label_style()),
            Span::raw(size),
        ]),
        Line::raw(""),
    ];

    let Some(body) = request.response_body.as_deref() else {
        lines.push(Line::styled(
            "No image body captured yet. Refresh while capture is active.",
            warning_style(),
        ));
        return lines;
    };
    let Some((data_mime, base64_data)) = parse_image_data_url(body) else {
        lines.push(Line::styled(
            "Image body is metadata-only; no inline preview payload was stored.",
            warning_style(),
        ));
        return lines;
    };

    match terminal_image_protocol() {
        Some(ImageProtocol::Kitty) => {
            lines.push(Line::raw(kitty_image_escape(base64_data)));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Rendered with Kitty graphics protocol.",
                muted_style(),
            ));
        }
        Some(ImageProtocol::ITerm) => {
            lines.push(Line::raw(iterm_image_escape(base64_data)));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Rendered with iTerm inline image protocol.",
                muted_style(),
            ));
        }
        None => {
            lines.push(Line::styled(
                "Inline preview requires Kitty or iTerm image protocol support.",
                warning_style(),
            ));
            lines.push(Line::from(vec![
                Span::styled("captured ", label_style()),
                Span::raw(format!(
                    "{} base64 chars for {data_mime}",
                    base64_data.len()
                )),
            ]));
        }
    }
    lines
}

fn parse_image_data_url(body: &str) -> Option<(&str, &str)> {
    let rest = body.strip_prefix("data:")?;
    let (metadata, data) = rest.split_once(',')?;
    let (mime, encoding) = metadata.split_once(';')?;
    (mime.starts_with("image/") && encoding == "base64").then_some((mime, data))
}

#[derive(Debug, Clone, Copy)]
enum ImageProtocol {
    Kitty,
    ITerm,
}

fn terminal_image_protocol() -> Option<ImageProtocol> {
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    if term.contains("kitty") {
        return Some(ImageProtocol::Kitty);
    }
    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_lowercase();
    if term_program.contains("iterm") {
        return Some(ImageProtocol::ITerm);
    }
    None
}

fn kitty_image_escape(base64_data: &str) -> String {
    format!("\x1b_Ga=T,f=100;{base64_data}\x1b\\")
}

fn iterm_image_escape(base64_data: &str) -> String {
    format!("\x1b]1337;File=inline=1;width=auto;height=12;preserveAspectRatio=1:{base64_data}\x07")
}

fn kitty_favicon_escape(base64_data: &str) -> String {
    format!("\x1b_Ga=T,f=100,c=2,r=1;{base64_data}\x1b\\")
}

fn iterm_favicon_escape(base64_data: &str) -> String {
    format!("\x1b]1337;File=inline=1;width=2;height=1;preserveAspectRatio=1:{base64_data}\x07")
}

fn is_sse_request(request: &RequestView) -> bool {
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

fn sse_body_lines(request: &RequestView) -> Vec<Line<'static>> {
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
struct SseEvent {
    event: Option<String>,
    id: Option<String>,
    retry: Option<String>,
    data: Vec<String>,
}

fn parse_sse_events(body: &str) -> Vec<SseEvent> {
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

fn body_lines(title: &'static str, body: String) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(title, label_style()), Line::raw("")];
    lines.extend(syntax_body_lines(body));
    lines
}

fn body_preview_lines(title: &'static str, body: String) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(title, label_style()), Line::raw("")];
    lines.extend(
        body.lines()
            .take(80)
            .map(|line| Line::styled(line.to_string(), Style::default().fg(GB_FG))),
    );
    lines
}

fn response_body_lines(request: &RequestView, active: bool) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled("response body", label_style()), Line::raw("")];
    lines.extend(response_body_content_lines(request, active));
    lines
}

fn response_body_content_lines(request: &RequestView, active: bool) -> Vec<Line<'static>> {
    let body = formatted_response_body(request);
    let lines = syntax_body_lines_for_request(request, body);
    if active {
        numbered_lines(lines)
    } else {
        lines.into_iter().take(80).collect()
    }
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

fn replay_lines(request: &RequestView) -> Vec<Line<'static>> {
    let Some(replay) = request.replays.last() else {
        return vec![Line::raw("No replay captured for this request.")];
    };

    let mut lines = vec![
        labeled_line("replay id", replay.record.id.clone()),
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
        labeled_line("command", replay.record.command.clone()),
        Line::raw(""),
        Line::styled("response body", label_style()),
        Line::raw(""),
    ];

    if let Some(body) = replay.body.as_deref() {
        lines.extend(syntax_body_lines(body.to_string()).into_iter().take(80));
    } else {
        lines.push(Line::raw("(none)"));
    }

    lines
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

fn labeled_line(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), label_style()),
        Span::raw(value),
    ])
}

fn empty_state_lines(title: &'static str, hint: &'static str) -> Vec<Line<'static>> {
    vec![
        Line::styled(title, label_style()),
        Line::styled(hint, muted_style()),
    ]
}

fn query_params_for_url(url: &str) -> Vec<(String, String)> {
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

pub(super) fn warning_style() -> Style {
    Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD)
}
