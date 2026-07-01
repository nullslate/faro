use crate::tui::state::{WorkbenchState, WorkbenchView};
use ratatui::layout::Rect;

mod help;
mod palette;
mod sessions;
mod sql;

pub(super) use help::{render_help, render_theme_preview};
pub(super) use palette::render_palette;
pub(super) use sessions::render_sessions;
pub(super) use sql::render_sql_results;

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
