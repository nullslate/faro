use super::{GB_FG, label_style, muted_style, panel_block, panel_title_style};
use crate::tui::state::WorkbenchState;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

pub(super) fn render(frame: &mut ratatui::Frame, app: &WorkbenchState) {
    let area = top_right_rect(frame.area(), 54, 19);
    frame.render_widget(Clear, area);
    let selected = app.selected_request();
    let body_state = selected
        .map(|request| {
            let response_bytes = request
                .response_body
                .as_ref()
                .map(|body| body.len().to_string())
                .unwrap_or_else(|| "-".to_string());
            format!(
                "{} body={} replays={}",
                if request.details_loaded {
                    "loaded"
                } else {
                    "pending"
                },
                response_bytes,
                request.replays.len()
            )
        })
        .unwrap_or_else(|| "none".to_string());
    let lines = vec![
        Line::from(vec![
            Span::styled("Perf", panel_title_style(true)),
            Span::styled("  ~ close", muted_style()),
        ]),
        Line::styled(
            "-".repeat(area.width.saturating_sub(4) as usize),
            muted_style(),
        ),
        perf_line("frame", app.perf.last_frame_ms, app.perf.max_frame_ms),
        perf_line("tick", app.perf.last_tick_ms, app.perf.max_tick_ms),
        perf_line("poll", app.perf.last_poll_ms, app.perf.max_poll_ms),
        perf_line(
            "db refresh",
            app.perf.last_db_refresh_ms,
            app.perf.max_db_refresh_ms,
        ),
        perf_line(
            "live merge",
            app.perf.last_live_merge_ms,
            app.perf.max_live_merge_ms,
        ),
        perf_line("filter", app.perf.last_filter_ms, app.perf.max_filter_ms),
        perf_line(
            "tree",
            app.perf.last_tree_build_ms,
            app.perf.max_tree_build_ms,
        ),
        perf_line(
            "req render",
            app.perf.last_request_render_ms,
            app.perf.max_request_render_ms,
        ),
        Line::from(vec![
            Span::styled("drain ", label_style()),
            Span::raw(format!(
                "capture={}ms replay={}ms detail={}ms",
                app.perf.last_capture_drain_ms,
                app.perf.last_replay_drain_ms,
                app.perf.last_detail_drain_ms
            )),
        ]),
        Line::from(vec![
            Span::styled("frames ", label_style()),
            Span::raw(app.perf.frame_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled("requests ", label_style()),
            Span::raw(format!(
                "{}/{}",
                app.filtered_request_indices.len(),
                app.requests.len()
            )),
        ]),
        Line::from(vec![
            Span::styled("console ", label_style()),
            Span::raw(format!(
                "{}/{}",
                app.filtered_console_indices.len(),
                app.console_logs.len()
            )),
        ]),
        Line::from(vec![
            Span::styled("websocket ", label_style()),
            Span::raw(format!(
                "{}/{}",
                app.filtered_websocket_indices.len(),
                app.websocket_frames.len()
            )),
        ]),
        Line::from(vec![
            Span::styled("detail loads ", label_style()),
            Span::raw(format!(
                "started={} done={}",
                app.perf.detail_load_started, app.perf.detail_load_completed
            )),
        ]),
        Line::from(vec![
            Span::styled("replays ", label_style()),
            Span::raw(format!("done={}", app.perf.replay_completed)),
        ]),
        Line::from(vec![
            Span::styled("selected ", label_style()),
            Span::raw(body_state),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block("Debug Perf", true))
            .style(Style::default().fg(GB_FG)),
        area,
    );
}

fn perf_line(label: &'static str, last: u128, max: u128) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label} "), label_style()),
        Span::raw(format!("last={last}ms max={max}ms")),
    ])
}

fn top_right_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y,
        width,
        height,
    }
}
