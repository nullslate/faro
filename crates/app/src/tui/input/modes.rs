use super::InputOutcome;
use crate::tui::state::{InputMode, WorkbenchState};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_body_search_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    match key.code {
        KeyCode::Esc => app.close_body_search(),
        KeyCode::Enter => {
            app.apply_body_search();
            app.input_mode = InputMode::Normal;
        }
        KeyCode::Backspace => app.pop_body_search_char(),
        KeyCode::Char(character) => app.push_body_search_char(character),
        _ => {}
    }
    InputOutcome::Continue
}

pub(super) fn handle_filtering_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.input_mode = InputMode::Normal,
        KeyCode::Backspace => app.pop_filter_char(),
        KeyCode::Char(character) => app.push_filter_char(character),
        _ => {}
    }
    InputOutcome::Continue
}

pub(super) fn handle_sessions_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    match key.code {
        KeyCode::Esc | KeyCode::Char('S') => app.close_sessions(),
        KeyCode::Char('j') | KeyCode::Down => app.next_session(),
        KeyCode::Char('k') | KeyCode::Up => app.previous_session(),
        KeyCode::Enter => return InputOutcome::SwitchSession,
        KeyCode::Char('x') | KeyCode::Delete => return InputOutcome::DeleteSession,
        KeyCode::Char('p') => {
            app.close_sessions();
            app.open_palette();
        }
        KeyCode::Char('q') => return InputOutcome::Quit,
        _ => {}
    }
    InputOutcome::Continue
}

pub(super) fn handle_sql_result_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
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
