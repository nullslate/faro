use anyhow::Context;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::env;
use std::fs;
use std::io::Stdout;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    path: &Path,
) -> anyhow::Result<String> {
    run_editor_with_env(terminal, path, &[])
}

pub(crate) fn run_editor_with_env(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    path: &Path,
    editor_env: &[(&str, String)],
) -> anyhow::Result<String> {
    suspend_terminal_for_editor(terminal).context("suspend terminal before editor")?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    let mut command = Command::new(&editor);
    for (key, value) in editor_env {
        command.env(key, value);
    }
    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }
    let status = command.arg(path).status();

    resume_terminal_after_editor(terminal).context("restore terminal after editor")?;

    Ok(match status {
        Ok(status) if status.success() => format!("opened body in {editor}: {}", path.display()),
        Ok(status) => format!("editor exited with {status}: {}", path.display()),
        Err(error) => format!("failed to open {editor}: {error}; wrote {}", path.display()),
    })
}

pub(crate) fn suspend_terminal_for_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> anyhow::Result<()> {
    execute!(terminal.backend_mut(), DisableMouseCapture)
        .context("disable mouse capture before editor")?;
    disable_raw_mode().context("disable raw mode before editor")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("leave alternate screen before editor")?;
    terminal
        .show_cursor()
        .context("show cursor before editor")?;
    Ok(())
}

pub(crate) fn resume_terminal_after_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> anyhow::Result<()> {
    execute!(terminal.backend_mut(), EnterAlternateScreen)
        .context("re-enter alternate screen after editor")?;
    enable_raw_mode().context("re-enable raw mode after editor")?;
    execute!(terminal.backend_mut(), EnableMouseCapture)
        .context("enable mouse capture after editor")?;
    terminal.hide_cursor().context("hide cursor after editor")?;
    terminal.clear().context("clear terminal after editor")?;
    Ok(())
}

pub(crate) fn write_temp_file(
    prefix: &str,
    extension: &str,
    contents: &str,
) -> anyhow::Result<PathBuf> {
    write_temp_bytes(prefix, extension, contents.as_bytes())
}

pub(crate) fn write_temp_bytes(
    prefix: &str,
    extension: &str,
    contents: &[u8],
) -> anyhow::Result<PathBuf> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let path = env::temp_dir().join(format!("{prefix}-{}-{now}.{extension}", std::process::id()));
    fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}
