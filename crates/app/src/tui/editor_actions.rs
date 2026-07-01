use super::editor::{run_editor, write_temp_file};
use super::state::{self, DetailTab, WorkbenchState, WorkbenchView};
use anyhow::Context;
use faro_core::{
    ConsoleLevel, ConsoleLog, CookieEventRecord, StorageEventRecord, console_event,
    cookie_event_observed_event, storage_changed_event,
};
use faro_store::Store;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::fs;
use std::io::Stdout;

pub(crate) fn edit_console_expression(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "console eval unavailable: open browser with o first".to_string();
        return Ok(());
    };

    let template = [
        "// Faro console scratch",
        "// Return a value or await a promise. This runs in the inspected page.",
        "",
        "document.title",
        "",
    ]
    .join("\n");
    let path =
        write_temp_file("faro-console", "js", &template).context("write console scratch file")?;
    app.status = run_editor(terminal, &path).context("run editor for console scratch")?;
    let expression = fs::read_to_string(&path)
        .with_context(|| format!("read console scratch file {}", path.display()))?;
    let expression = expression
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if expression.is_empty() {
        app.status = "console eval skipped: empty expression".to_string();
        return Ok(());
    }

    match faro_cdp::evaluate_expression_blocking(&websocket_url, &expression) {
        Ok(result) => {
            persist_console_eval(app, ConsoleLevel::Info, format!("> {expression}\n{result}"))
                .context("persist console eval result")?;
            app.reload()
                .context("reload TUI state after console eval")?;
            app.status = "console eval completed".to_string();
        }
        Err(error) => {
            persist_console_eval(app, ConsoleLevel::Error, format!("> {expression}\n{error}"))
                .context("persist failed console eval result")?;
            app.reload()
                .context("reload TUI state after failed console eval")?;
            app.status = format!("console eval failed: {error}");
        }
    }

    Ok(())
}

pub(crate) fn open_selected_item_in_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    match app.view {
        WorkbenchView::Storage => return edit_selected_storage_value(terminal, app),
        WorkbenchView::Cookies => return edit_selected_cookie_value(terminal, app),
        _ => {}
    }

    app.hydrate_selected_request();
    let body = match app.detail_tab {
        DetailTab::RequestBody => app.selected_request_body_for_editor(),
        _ => app.selected_response_body_for_editor(),
    };
    let Some((body, extension)) = body else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("faro-body", &extension, &body).context("write selected body file")?;
    app.status = run_editor(terminal, &path).context("run editor for selected body")?;
    Ok(())
}

fn edit_selected_storage_value(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(entry) = app.selected_storage_entry() else {
        app.status = "no storage value selected".to_string();
        return Ok(());
    };
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "storage edit unavailable: open browser with o first".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("faro-storage", "txt", &entry.value).context("write storage edit file")?;
    app.status = run_editor(terminal, &path).context("run editor for storage value")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read edited storage value {}", path.display()))?;

    match faro_cdp::set_storage_item_blocking(
        &websocket_url,
        &entry.origin,
        &entry.storage_type,
        &entry.key,
        &value,
    ) {
        Ok(()) => {
            persist_storage_edit(app, &entry, &value).context("persist storage edit")?;
            app.reload().context("reload state after storage edit")?;
            app.status = format!("updated {} {}", entry.storage_type, entry.key);
        }
        Err(error) => app.status = format!("storage edit failed: {error}"),
    }
    Ok(())
}

fn edit_selected_cookie_value(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(entry) = app.selected_cookie_entry() else {
        app.status = "no cookie selected".to_string();
        return Ok(());
    };
    let Some(websocket_url) = app.cdp_websocket_url.clone() else {
        app.status = "cookie edit unavailable: open browser with o first".to_string();
        return Ok(());
    };
    let path =
        write_temp_file("faro-cookie", "txt", &entry.value).context("write cookie edit file")?;
    app.status = run_editor(terminal, &path).context("run editor for cookie value")?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("read edited cookie value {}", path.display()))?;
    let mut cookie = entry.to_cookie_record();
    cookie.value = value.clone();

    match faro_cdp::set_cookie_value_blocking(&websocket_url, &cookie, &value) {
        Ok(()) => {
            persist_cookie_edit(app, &entry, &value).context("persist cookie edit")?;
            app.reload().context("reload state after cookie edit")?;
            app.status = format!("updated cookie {}", entry.name);
        }
        Err(error) => app.status = format!("cookie edit failed: {error}"),
    }
    Ok(())
}

fn persist_console_eval(
    app: &WorkbenchState,
    level: ConsoleLevel,
    message: String,
) -> anyhow::Result<()> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let session = store
        .sessions()
        .context("load sessions for console eval persistence")?
        .into_iter()
        .last()
        .ok_or_else(|| anyhow::anyhow!("no active session"))?;
    let log = ConsoleLog::new(
        session.id,
        None,
        None,
        level,
        message,
        Some("faro-console".to_string()),
        None,
    );
    store
        .insert_console_log(&log)
        .context("insert console eval log")?;
    store
        .append_event(&console_event(&log))
        .context("append console eval event")?;
    Ok(())
}

fn persist_storage_edit(
    app: &WorkbenchState,
    entry: &state::CurrentStorageEntry,
    value: &str,
) -> anyhow::Result<()> {
    let Some(session_id) = app.active_session_id.clone() else {
        return Ok(());
    };
    let event = StorageEventRecord::new(
        session_id,
        None,
        None,
        entry.origin.clone(),
        entry.storage_type.clone(),
        "update".to_string(),
        Some(entry.key.clone()),
        Some(entry.value.clone()),
        Some(value.to_string()),
    );
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    store
        .insert_storage_event(&event)
        .context("insert storage edit event")?;
    store
        .append_event(&storage_changed_event(&event))
        .context("append storage edit event")?;
    Ok(())
}

fn persist_cookie_edit(
    app: &WorkbenchState,
    entry: &state::CurrentCookieEntry,
    value: &str,
) -> anyhow::Result<()> {
    let Some(session_id) = app.active_session_id.clone() else {
        return Ok(());
    };
    let event = CookieEventRecord::new(
        session_id,
        None,
        None,
        "update",
        Some(entry.name.clone()),
        Some(entry.domain.clone()),
        Some(entry.path.clone()),
        Some(value.to_string()),
        None,
    );
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    store
        .insert_cookie_event(&event)
        .context("insert cookie edit event")?;
    store
        .append_event(&cookie_event_observed_event(&event))
        .context("append cookie edit event")?;
    Ok(())
}
