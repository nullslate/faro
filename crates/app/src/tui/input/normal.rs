use super::InputOutcome;
use crate::tui::state::{DetailTab, FocusPane, InputMode, WorkbenchState, WorkbenchView};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn handle_normal_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    match key.code {
        KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.resize_requests_split(-4);
            InputOutcome::SaveLayout
        }
        KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.resize_requests_split(4);
            InputOutcome::SaveLayout
        }
        KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.resize_detail_split(-4);
            InputOutcome::SaveLayout
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.resize_detail_split(4);
            InputOutcome::SaveLayout
        }
        KeyCode::Char('q') | KeyCode::Esc => InputOutcome::Quit,
        KeyCode::F(5) => InputOutcome::RefreshPage,
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            InputOutcome::RefreshPage
        }
        KeyCode::Tab => {
            app.next_focus();
            InputOutcome::Continue
        }
        KeyCode::Char('1') => {
            app.set_view(WorkbenchView::Network);
            InputOutcome::Continue
        }
        KeyCode::Char('2') => {
            app.set_view(WorkbenchView::Console);
            InputOutcome::Continue
        }
        KeyCode::Char('3') => {
            app.set_view(WorkbenchView::WebSockets);
            InputOutcome::Continue
        }
        KeyCode::Char('4') => {
            app.set_view(WorkbenchView::Scripts);
            InputOutcome::Continue
        }
        KeyCode::Char('5') => {
            app.set_view(WorkbenchView::Storage);
            InputOutcome::Continue
        }
        KeyCode::Char('6') => {
            app.set_view(WorkbenchView::Cookies);
            InputOutcome::Continue
        }
        KeyCode::Char('/')
            if app.view == WorkbenchView::Network && app.focus == FocusPane::Body =>
        {
            app.open_body_search();
            InputOutcome::Continue
        }
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Filtering;
            InputOutcome::Continue
        }
        KeyCode::Char('c') if app.view == WorkbenchView::Console => InputOutcome::ClearConsole,
        KeyCode::Char('c') if app.view == WorkbenchView::Network => InputOutcome::ClearRequests,
        KeyCode::Char('c') => {
            app.clear_filter();
            InputOutcome::Continue
        }
        KeyCode::Char('f') => {
            app.cycle_filter_preset();
            InputOutcome::Continue
        }
        KeyCode::Char('?') => {
            app.toggle_help();
            InputOutcome::Continue
        }
        KeyCode::Char('S') => InputOutcome::OpenSessions,
        KeyCode::Char('~') => InputOutcome::TogglePerf,
        KeyCode::Char('p') => {
            app.open_palette();
            InputOutcome::Continue
        }
        KeyCode::Char('R') if app.view == WorkbenchView::Network => {
            app.set_focus(FocusPane::Requests);
            InputOutcome::Continue
        }
        KeyCode::Char('D') if app.view == WorkbenchView::Network => {
            app.set_focus(FocusPane::Detail);
            InputOutcome::Continue
        }
        KeyCode::Char('B') if app.view == WorkbenchView::Network => {
            app.set_focus(FocusPane::Body);
            InputOutcome::Continue
        }
        KeyCode::Enter if app.view == WorkbenchView::Network && app.focus == FocusPane::Body => {
            app.toggle_selected_body_tree_node();
            InputOutcome::Continue
        }
        KeyCode::Enter if app.view == WorkbenchView::Network => {
            app.enter_selected_request_group();
            InputOutcome::Continue
        }
        KeyCode::Backspace if app.view == WorkbenchView::Network => {
            app.leave_request_route_group();
            InputOutcome::Continue
        }
        KeyCode::Char(' ')
            if app.view == WorkbenchView::Network && app.focus == FocusPane::Body =>
        {
            app.toggle_selected_body_tree_node();
            InputOutcome::Continue
        }
        KeyCode::Char(' ') if app.view == WorkbenchView::Network => {
            app.toggle_selected_request_group();
            InputOutcome::Continue
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.next();
            InputOutcome::Continue
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.previous();
            InputOutcome::Continue
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.previous_tab();
            InputOutcome::Continue
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.next_tab();
            InputOutcome::Continue
        }
        KeyCode::Char('Y') if app.view == WorkbenchView::Network => InputOutcome::CopyBody,
        KeyCode::Char('y') => InputOutcome::CopyCurl,
        KeyCode::Char('w') => InputOutcome::SaveExchange,
        KeyCode::Char('o') => InputOutcome::OpenBrowser,
        KeyCode::Char('m') => InputOutcome::ToggleMaximize,
        KeyCode::Char('z') => {
            app.toggle_density_mode();
            InputOutcome::SaveLayout
        }
        KeyCode::Char('n') if app.view == WorkbenchView::Scripts => InputOutcome::CreateScript,
        KeyCode::Char('e') if app.view == WorkbenchView::Scripts => InputOutcome::EditScript,
        KeyCode::Char('e') if app.view == WorkbenchView::Console => InputOutcome::EditConsole,
        KeyCode::Char('e') => InputOutcome::OpenEditor,
        KeyCode::Char('r') if app.view == WorkbenchView::Scripts => InputOutcome::RunScript,
        KeyCode::Char('r') => InputOutcome::Replay,
        KeyCode::Char('R') if app.view == WorkbenchView::Scripts => InputOutcome::RenameScript,
        KeyCode::Char('R') => InputOutcome::EditReplay,
        KeyCode::Char('D') if app.view == WorkbenchView::Scripts => InputOutcome::DuplicateScript,
        KeyCode::Char('D') => InputOutcome::DiffReplay,
        KeyCode::Char('d')
            if app.view == WorkbenchView::Network
                && app.focus == FocusPane::Detail
                && app.detail_tab == DetailTab::Replay =>
        {
            InputOutcome::DiffReplay
        }
        KeyCode::Char('x') if app.view == WorkbenchView::Scripts => InputOutcome::DeleteScript,
        KeyCode::Char('s') => {
            app.next_sort_mode();
            InputOutcome::Continue
        }
        KeyCode::Char('u') | KeyCode::PageUp => {
            app.scroll_up();
            InputOutcome::Continue
        }
        KeyCode::Char('d') | KeyCode::PageDown => {
            app.scroll_down();
            InputOutcome::Continue
        }
        KeyCode::Char('g') | KeyCode::Home => {
            app.scroll_top();
            InputOutcome::Continue
        }
        KeyCode::Char('G') | KeyCode::End => {
            app.scroll_bottom();
            InputOutcome::Continue
        }
        _ => InputOutcome::Continue,
    }
}
