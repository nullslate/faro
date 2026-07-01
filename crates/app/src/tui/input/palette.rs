use super::InputOutcome;
use crate::tui::state::{PaletteCommand, WorkbenchState};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_palette_key(app: &mut WorkbenchState, key: KeyEvent) -> InputOutcome {
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
        PaletteCommand::LayoutPreset(preset) => {
            app.apply_layout_preset(preset);
            InputOutcome::SaveLayout
        }
        PaletteCommand::ToggleHelp => {
            app.toggle_help();
            InputOutcome::Continue
        }
        PaletteCommand::ToggleThemePreview => {
            app.toggle_theme_preview();
            InputOutcome::Continue
        }
        PaletteCommand::TogglePerf => InputOutcome::TogglePerf,
        PaletteCommand::OpenSessions => InputOutcome::OpenSessions,
        PaletteCommand::OpenBrowser => InputOutcome::OpenBrowser,
        PaletteCommand::RefreshPage => InputOutcome::RefreshPage,
        PaletteCommand::CopyCurl => InputOutcome::CopyCurl,
        PaletteCommand::CopyShareBundle => InputOutcome::CopyShareBundle,
        PaletteCommand::SaveExchange => InputOutcome::SaveExchange,
        PaletteCommand::Replay => InputOutcome::Replay,
        PaletteCommand::EditReplay => InputOutcome::EditReplay,
        PaletteCommand::DiffReplay => InputOutcome::DiffReplay,
        PaletteCommand::OpenEditor => InputOutcome::OpenEditor,
        PaletteCommand::CopyBody => InputOutcome::CopyBody,
        PaletteCommand::BodySearch => InputOutcome::BodySearch,
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
