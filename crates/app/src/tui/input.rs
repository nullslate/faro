use super::state::{InputMode, WorkbenchState};
use crossterm::event::{KeyCode, KeyEvent};

mod modes;
mod normal;
mod palette;

use modes::{
    handle_body_search_key, handle_filtering_key, handle_sessions_key, handle_sql_result_key,
};
use normal::handle_normal_key;
use palette::handle_palette_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputOutcome {
    Continue,
    Quit,
    CopyCurl,
    CopyBody,
    CopyShareBundle,
    SaveExchange,
    OpenBrowser,
    ToggleMaximize,
    OpenEditor,
    EditConsole,
    ClearConsole,
    ClearRequests,
    Replay,
    EditReplay,
    DiffReplay,
    SaveLayout,
    RefreshPage,
    SqlQuery,
    BodySearch,
    CreateScript,
    EditScript,
    RunScript,
    DuplicateScript,
    RenameScript,
    DeleteScript,
    ResetScriptTemplates,
    OpenSessions,
    SwitchSession,
    DeleteSession,
    TogglePerf,
}

pub(crate) fn handle_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
    if app.input_mode == InputMode::BodySearch {
        return handle_body_search_key(app, key);
    }

    if app.input_mode == InputMode::Filtering {
        return handle_filtering_key(app, key);
    }

    if app.input_mode == InputMode::Palette {
        handle_palette_key(app, key)
    } else if app.show_sessions {
        handle_sessions_key(app, key)
    } else if app.sql_result.is_some() {
        handle_sql_result_key(app, key)
    } else if app.show_theme_preview {
        if key.code == KeyCode::Esc {
            app.toggle_theme_preview();
        }
        InputOutcome::Continue
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
