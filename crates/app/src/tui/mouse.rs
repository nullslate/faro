use super::layout;
use super::state::{self, WorkbenchState};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(super) fn handle_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) -> bool {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            app.scroll_down();
            true
        }
        MouseEventKind::ScrollUp => {
            app.scroll_up();
            true
        }
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_click(app, mouse, area);
            true
        }
        _ => false,
    }
}

fn handle_mouse_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    if app.input_mode != state::InputMode::Normal || app.show_help {
        return;
    }
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(14),
            Constraint::Length(2),
        ])
        .split(area);
    let content = root[1];
    if !rect_contains(content, mouse.column, mouse.row) {
        return;
    }

    if app.layout_mode == layout::LayoutMode::Focused {
        handle_mouse_click_focused(app, mouse, content);
        return;
    }

    if content.width >= 108 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(7), Constraint::Min(40)])
            .split(content);
        if rect_contains(columns[0], mouse.column, mouse.row) {
            handle_rail_click(app, mouse, columns[0]);
            return;
        }
        handle_content_click(app, mouse, columns[1]);
    } else {
        handle_content_click(app, mouse, content);
    }
}

fn handle_mouse_click_focused(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    match app.focus {
        state::FocusPane::Requests => select_request_from_mouse(app, mouse, area),
        state::FocusPane::Detail => app.set_focus(state::FocusPane::Detail),
        state::FocusPane::Body => app.set_focus(state::FocusPane::Body),
        state::FocusPane::Console => app.set_focus(state::FocusPane::Console),
        state::FocusPane::WebSockets => select_websocket_from_mouse(app, mouse, area),
        state::FocusPane::Scripts => app.set_focus(state::FocusPane::Scripts),
        state::FocusPane::Storage => select_storage_from_mouse(app, mouse, area),
        state::FocusPane::Cookies => select_cookie_from_mouse(app, mouse, area),
    }
}

fn handle_content_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    match app.view {
        state::WorkbenchView::Network => handle_network_click(app, mouse, area),
        state::WorkbenchView::Console => app.set_focus(state::FocusPane::Console),
        state::WorkbenchView::WebSockets => select_websocket_from_mouse(app, mouse, area),
        state::WorkbenchView::Scripts => app.set_focus(state::FocusPane::Scripts),
        state::WorkbenchView::Storage => select_storage_from_mouse(app, mouse, area),
        state::WorkbenchView::Cookies => select_cookie_from_mouse(app, mouse, area),
    }
}

fn handle_rail_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let row = mouse.row.saturating_sub(area.y);
    match row {
        0 => app.set_view(state::WorkbenchView::Network),
        1 => app.set_view(state::WorkbenchView::Console),
        2 => app.set_view(state::WorkbenchView::WebSockets),
        3 => app.set_view(state::WorkbenchView::Scripts),
        4 => app.set_view(state::WorkbenchView::Storage),
        5 => app.set_view(state::WorkbenchView::Cookies),
        _ => {}
    }
}

fn handle_network_click(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(12),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.requests_percent),
            Constraint::Percentage(100 - app.requests_percent),
        ])
        .split(root[2]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.detail_percent),
            Constraint::Percentage(100 - app.detail_percent),
        ])
        .split(body[1]);

    if rect_contains(body[0], mouse.column, mouse.row) {
        app.set_focus(state::FocusPane::Requests);
        select_request_from_mouse(app, mouse, body[0]);
    } else if rect_contains(right[0], mouse.column, mouse.row) {
        app.set_focus(state::FocusPane::Detail);
    } else if rect_contains(right[1], mouse.column, mouse.row) {
        app.set_focus(state::FocusPane::Body);
    }
}

fn select_request_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    let visible_rows = area.height.saturating_sub(3).max(1) as usize;
    let row = mouse.row.saturating_sub(area.y.saturating_add(2)) as usize;
    if row >= visible_rows {
        return;
    }
    let selected = app.table_state.selected().unwrap_or(0);
    let start = selected_window_start(selected, visible_rows, app.filtered_request_rows.len());
    app.select_request_position(start + row);
}

fn select_storage_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    app.set_focus(state::FocusPane::Storage);
    let chunks = horizontal_two_pane(area);
    if !rect_contains(chunks[0], mouse.column, mouse.row) {
        return;
    }
    let total = app.current_storage_entries().len();
    let visible_rows = chunks[0].height.saturating_sub(2).max(1) as usize;
    let row = mouse.row.saturating_sub(chunks[0].y.saturating_add(1)) as usize;
    if row >= visible_rows {
        return;
    }
    let start = selected_window_start(app.storage_selected, visible_rows, total);
    app.select_storage_position(start + row);
}

fn select_websocket_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    app.set_focus(state::FocusPane::WebSockets);
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(area);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(root[1]);
    if !rect_contains(chunks[0], mouse.column, mouse.row) {
        return;
    }
    let visible_rows = chunks[0].height.saturating_sub(2).max(1) as usize;
    let row = mouse.row.saturating_sub(chunks[0].y.saturating_add(1)) as usize;
    if row >= visible_rows || app.filtered_websocket_indices.is_empty() {
        return;
    }
    let selected = app.websocket_state.selected().unwrap_or(0);
    let start = selected_window_start(selected, visible_rows, app.filtered_websocket_indices.len());
    app.websocket_state.select(Some(
        (start + row).min(app.filtered_websocket_indices.len() - 1),
    ));
}

fn select_cookie_from_mouse(app: &mut WorkbenchState, mouse: MouseEvent, area: Rect) {
    app.set_focus(state::FocusPane::Cookies);
    let chunks = horizontal_two_pane(area);
    if !rect_contains(chunks[0], mouse.column, mouse.row) {
        return;
    }
    let total = app.current_cookie_entries().len();
    let visible_rows = chunks[0].height.saturating_sub(2).max(1) as usize;
    let row = mouse.row.saturating_sub(chunks[0].y.saturating_add(1)) as usize;
    if row >= visible_rows {
        return;
    }
    let start = selected_window_start(app.cookie_selected, visible_rows, total);
    app.select_cookie_position(start + row);
}

fn horizontal_two_pane(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area)
}

fn selected_window_start(selected: usize, visible_rows: usize, total: usize) -> usize {
    if total <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows / 2)
        .min(total.saturating_sub(visible_rows))
}

fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}
