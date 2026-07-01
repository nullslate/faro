#![allow(clippy::items_after_test_module)]

mod chrome;
mod console;
mod console_lines;
mod data;
mod detail;
mod modals;
mod perf;
mod request_format;
mod requests;
mod scripts;
mod style;
mod websockets;
mod window;
mod workspace;

use super::layout::{DensityMode, LayoutMode};
use super::state::{
    BodyTreeItem, CapturedFavicon, CurrentCookieEntry, CurrentStorageEntry, DetailTab, FocusPane,
    InputMode, ReplayView, RequestStats, RequestTreeMeta, RequestView, WorkbenchState,
    WorkbenchView, domain_for_url, formatted_request_body, formatted_response_body, path_for_url,
    websocket_opcode_label,
};
use crate::config::Theme;
#[cfg(test)]
use chrome::view_tabs_line;
#[cfg(test)]
use console::console_stack_lines;
use console_lines::*;
use detail::{
    ImageProtocol, highlight_javascript_line, is_image_request, is_sse_request,
    iterm_favicon_escape, kitty_favicon_escape, labeled_line, parse_image_data_url,
    syntax_body_lines, terminal_image_protocol,
};
#[cfg(test)]
use detail::{
    detail_tab_lines, line_width, parse_sse_events, replay_lines, response_body_content_lines,
    syntax_body_lines_for_request,
};
use faro_core::{ConsoleLevel, ConsoleLog, WebSocketFrameDirection, WebSocketFrameRecord};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    Wrap,
};
use request_format::*;
#[cfg(test)]
use scripts::script_output_lines;
use style::*;
use window::*;

pub(crate) fn render(frame: &mut ratatui::Frame, app: &mut WorkbenchState) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(14),
            Constraint::Length(2),
        ])
        .split(frame.area());

    chrome::render_header(frame, root[0], app);
    match app.layout_mode {
        LayoutMode::Normal => workspace::render_normal_layout(frame, root[1], app),
        LayoutMode::Focused => workspace::render_focused_layout(frame, root[1], app),
    }
    chrome::render_status(frame, root[2], app);
    if app.sql_result.is_some() {
        modals::render_sql_results(frame, app);
    }
    if app.input_mode == InputMode::Palette {
        modals::render_palette(frame, app);
    }
    if app.show_sessions {
        modals::render_sessions(frame, app);
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

#[cfg(test)]
mod tests;
