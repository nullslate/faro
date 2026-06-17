use super::state::{FocusPane, InputMode, PaletteCommand, WorkbenchState, WorkbenchView};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputOutcome {
    Continue,
    Quit,
    CopyCurl,
    SaveExchange,
    OpenBrowser,
    ToggleMaximize,
    OpenEditor,
    EditConsole,
    ClearConsole,
    Replay,
    EditReplay,
    DiffReplay,
    SaveLayout,
    RefreshPage,
    SqlQuery,
    CreateScript,
    EditScript,
    RunScript,
    DuplicateScript,
    RenameScript,
    DeleteScript,
    ResetScriptTemplates,
    TogglePerf,
}

pub(crate) fn handle_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    if app.input_mode == InputMode::Filtering {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.input_mode = InputMode::Normal,
            KeyCode::Backspace => app.pop_filter_char(),
            KeyCode::Char(character) => app.push_filter_char(character),
            _ => {}
        }
        return InputOutcome::Continue;
    }

    if app.input_mode == InputMode::Palette {
        match key.code {
            KeyCode::Esc => {
                app.close_palette();
                InputOutcome::Continue
            }
            KeyCode::Enter => execute_palette_command(app),
            KeyCode::Backspace => {
                app.pop_palette_char();
                InputOutcome::Continue
            }
            KeyCode::Down => {
                app.next_palette_item();
                InputOutcome::Continue
            }
            KeyCode::Up => {
                app.previous_palette_item();
                InputOutcome::Continue
            }
            KeyCode::Char(character) => {
                app.push_palette_char(character);
                InputOutcome::Continue
            }
            _ => InputOutcome::Continue,
        }
    } else if app.sql_result.is_some() {
        handle_sql_result_key(app, key)
    } else if app.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc => app.toggle_help(),
            _ => {}
        }
        InputOutcome::Continue
    } else {
        handle_normal_key(app, key)
    }
}

fn handle_sql_result_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    match key.code {
        KeyCode::Esc => app.close_sql_result(),
        KeyCode::Char('j') | KeyCode::Down => app.scroll_sql_rows_down(),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_sql_rows_up(),
        KeyCode::Char('h') | KeyCode::Left => app.scroll_sql_columns_left(),
        KeyCode::Char('l') | KeyCode::Right => app.scroll_sql_columns_right(),
        KeyCode::Char('g') | KeyCode::Home => app.scroll_sql_top(),
        KeyCode::Char('G') | KeyCode::End => app.scroll_sql_bottom(),
        KeyCode::Char('p') => app.open_palette(),
        KeyCode::Char('q') => return InputOutcome::Quit,
        _ => {}
    }
    InputOutcome::Continue
}

fn handle_normal_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
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
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Filtering;
            InputOutcome::Continue
        }
        KeyCode::Char('c') if app.view == WorkbenchView::Console => InputOutcome::ClearConsole,
        KeyCode::Char('c') if app.view == WorkbenchView::Network => {
            app.clear_request_filter_and_route();
            InputOutcome::Continue
        }
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
        KeyCode::Char('x') if app.view == WorkbenchView::Scripts => InputOutcome::DeleteScript,
        KeyCode::Char('s') => {
            app.next_sort_mode();
            InputOutcome::Continue
        }
        KeyCode::Char('S') => {
            app.toggle_sort_direction();
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

fn execute_palette_command(app: &mut WorkbenchState) -> InputOutcome {
    let Some(command) = app.selected_palette_command() else {
        app.close_palette();
        return InputOutcome::Continue;
    };
    app.close_palette();
    match command {
        PaletteCommand::View(view) => {
            app.set_view(view);
            InputOutcome::Continue
        }
        PaletteCommand::Filter(query) => {
            app.request_filter = query.to_string();
            app.apply_filter_from_palette();
            InputOutcome::Continue
        }
        PaletteCommand::ConsoleFilter(query) => {
            app.console_filter = query.to_string();
            app.apply_console_filter_from_palette();
            InputOutcome::Continue
        }
        PaletteCommand::WebSocketFilter(query) => {
            app.websocket_filter = query.to_string();
            app.apply_websocket_filter_from_palette();
            InputOutcome::Continue
        }
        PaletteCommand::ClearFilter => {
            app.clear_filter();
            InputOutcome::Continue
        }
        PaletteCommand::SortNext => {
            app.next_sort_mode();
            InputOutcome::Continue
        }
        PaletteCommand::SortDirection => {
            app.toggle_sort_direction();
            InputOutcome::Continue
        }
        PaletteCommand::ToggleLayout => InputOutcome::ToggleMaximize,
        PaletteCommand::ToggleDensity => {
            app.toggle_density_mode();
            InputOutcome::SaveLayout
        }
        PaletteCommand::ToggleHelp => {
            app.toggle_help();
            InputOutcome::Continue
        }
        PaletteCommand::TogglePerf => InputOutcome::TogglePerf,
        PaletteCommand::OpenBrowser => InputOutcome::OpenBrowser,
        PaletteCommand::RefreshPage => InputOutcome::RefreshPage,
        PaletteCommand::CopyCurl => InputOutcome::CopyCurl,
        PaletteCommand::SaveExchange => InputOutcome::SaveExchange,
        PaletteCommand::Replay => InputOutcome::Replay,
        PaletteCommand::EditReplay => InputOutcome::EditReplay,
        PaletteCommand::DiffReplay => InputOutcome::DiffReplay,
        PaletteCommand::OpenEditor => InputOutcome::OpenEditor,
        PaletteCommand::EditConsole => InputOutcome::EditConsole,
        PaletteCommand::SqlQuery => InputOutcome::SqlQuery,
        PaletteCommand::CreateScript => InputOutcome::CreateScript,
        PaletteCommand::EditScript => InputOutcome::EditScript,
        PaletteCommand::RunScript => InputOutcome::RunScript,
        PaletteCommand::DuplicateScript => InputOutcome::DuplicateScript,
        PaletteCommand::RenameScript => InputOutcome::RenameScript,
        PaletteCommand::DeleteScript => InputOutcome::DeleteScript,
        PaletteCommand::ResetScriptTemplates => InputOutcome::ResetScriptTemplates,
    }
}
