use super::editor::{run_editor, write_temp_file};
use super::script_templates;
use super::state::{WorkbenchState, WorkbenchView};
use anyhow::Context;
use faro_store::{ScriptRecord, Store};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::collections::HashSet;
use std::fs;
use std::io::Stdout;
use std::path::Path;

mod runtime;

use runtime::execute;

pub(crate) fn create(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    app.set_view(WorkbenchView::Scripts);
    let body = script_templates::default_body();
    let path = write_temp_file("faro-script-new", "rs", &body).context("write script file")?;
    app.status = run_editor(terminal, &path).context("run editor for new script")?;
    let body = read_script_body(&path)?;
    if body.trim().is_empty() {
        app.status = "script create skipped: empty file".to_string();
        return Ok(());
    }
    let name = script_name_from_body(&body).unwrap_or_else(|| next_script_name(app));
    let script = ScriptRecord::new(name, body);
    let script_id = script.id.clone();
    save_script_record(app, &script).context("save new script")?;
    app.reload().context("reload after script create")?;
    app.select_script_by_id(&script_id);
    app.status = "created script".to_string();
    Ok(())
}

pub(crate) fn reset_templates(app: &mut WorkbenchState) {
    match seed_templates(app, true).and_then(|added| {
        app.reload()?;
        Ok(added)
    }) {
        Ok(0) => app.status = "script templates already installed".to_string(),
        Ok(added) => app.status = format!("installed {added} script templates"),
        Err(error) => app.status = format!("script template install failed: {error}"),
    }
}

pub(crate) fn seed_templates(app: &WorkbenchState, force: bool) -> anyhow::Result<usize> {
    if !force && !app.scripts.is_empty() {
        return Ok(0);
    }
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let existing = store
        .scripts()
        .context("load existing scripts")?
        .into_iter()
        .map(|script| script.name)
        .collect::<HashSet<_>>();
    let mut added = 0;
    for template in script_templates::TEMPLATES {
        if existing.contains(template.name) {
            continue;
        }
        let script = ScriptRecord::new(template.name, template.body);
        store.save_script(&script).context("save script template")?;
        added += 1;
    }
    Ok(added)
}

pub(crate) fn edit(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(mut script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("faro-script-edit", "rs", &script.body)
        .context("write script edit file")?;
    app.status = run_editor(terminal, &path).context("run editor for script edit")?;
    let body = read_script_body(&path)?;
    if body.trim().is_empty() {
        app.status = "script edit skipped: empty file".to_string();
        return Ok(());
    }
    script.body = body;
    script.updated_at = faro_core::now_ms();
    save_script_record(app, &script).context("save edited script")?;
    app.reload().context("reload after script edit")?;
    app.select_script_by_id(&script.id);
    app.status = format!("updated script {}", script.name);
    Ok(())
}

pub(crate) fn run_selected(app: &mut WorkbenchState) {
    let Some(script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return;
    };

    match execute(app, &script.body) {
        Ok(result) => {
            app.script_output = result.output;
            app.script_duration_ms = Some(result.duration_ms);
            app.script_status = Some(if result.success {
                "success".to_string()
            } else {
                result
                    .error
                    .map(|error| format!("failed: {error}"))
                    .unwrap_or_else(|| "failed".to_string())
            });
            if result.success {
                let ran_at = faro_core::now_ms();
                let mark_result = Store::open(&app.db_path)
                    .with_context(|| format!("open database {}", app.db_path.display()))
                    .and_then(|store| {
                        store
                            .mark_script_run(&script.id, ran_at)
                            .context("mark script run")
                    });
                match mark_result {
                    Ok(()) => {
                        if let Some(current) = app
                            .scripts
                            .iter_mut()
                            .find(|candidate| candidate.id == script.id)
                        {
                            current.last_run_at = Some(ran_at);
                        }
                    }
                    Err(error) => {
                        app.status = format!("script ran; last-run save failed: {error}");
                        return;
                    }
                }
            }
            app.status = format!("script {} in {}ms", script.name, result.duration_ms);
        }
        Err(error) => {
            app.script_output = vec![format!("error: {error}")];
            app.script_duration_ms = None;
            app.script_status = Some("failed".to_string());
            app.status = format!("script failed: {error}");
        }
    }
}

pub(crate) fn duplicate(app: &mut WorkbenchState) {
    let Some(script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return;
    };
    let duplicate = ScriptRecord::new(format!("{} copy", script.name), script.body);
    let script_id = duplicate.id.clone();
    match save_script_record(app, &duplicate).and_then(|()| app.reload()) {
        Ok(()) => {
            app.select_script_by_id(&script_id);
            app.status = format!("duplicated script {}", script.name);
        }
        Err(error) => app.status = format!("duplicate script failed: {error}"),
    }
}

pub(crate) fn rename(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let Some(mut script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("faro-script-rename", "txt", &format!("{}\n", script.name))
        .context("write script rename file")?;
    app.status = run_editor(terminal, &path).context("run editor for script rename")?;
    let name = fs::read_to_string(&path)
        .with_context(|| format!("read script rename file {}", path.display()))?
        .lines()
        .find_map(|line| {
            let name = line.trim();
            (!name.is_empty()).then(|| name.to_string())
        });
    let Some(name) = name else {
        app.status = "script rename skipped: empty name".to_string();
        return Ok(());
    };
    script.name = name;
    script.updated_at = faro_core::now_ms();
    save_script_record(app, &script).context("save renamed script")?;
    app.reload().context("reload after script rename")?;
    app.select_script_by_id(&script.id);
    app.status = format!("renamed script {}", script.name);
    Ok(())
}

pub(crate) fn delete_selected(app: &mut WorkbenchState) {
    let Some(script) = app.selected_script().cloned() else {
        app.status = "no script selected".to_string();
        return;
    };
    match Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))
        .and_then(|store| store.delete_script(&script.id).context("delete script"))
        .and_then(|()| app.reload())
    {
        Ok(()) => app.status = format!("deleted script {}", script.name),
        Err(error) => app.status = format!("delete script failed: {error}"),
    }
}

fn save_script_record(app: &WorkbenchState, script: &ScriptRecord) -> anyhow::Result<()> {
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    store.save_script(script).context("save script")?;
    Ok(())
}

fn read_script_body(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| format!("read script file {}", path.display()))
}

fn script_name_from_body(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let line = line.trim();
        let name = line.strip_prefix("// name:")?.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

fn next_script_name(app: &WorkbenchState) -> String {
    let mut number = app.scripts.len() + 1;
    loop {
        let name = format!("Script {number}");
        if app.scripts.iter().all(|script| script.name != name) {
            return name;
        }
        number += 1;
    }
}
