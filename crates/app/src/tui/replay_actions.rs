use super::editor::{
    resume_terminal_after_editor, run_editor, suspend_terminal_for_editor, write_temp_bytes,
    write_temp_file,
};
use super::state::WorkbenchState;
use super::util::{append_audit_event, command_exists};
use crate::services::{parse_http_status, split_http_body};
use anyhow::Context;
use faro_core::{ReplayRecord, request_replayed_event};
use faro_store::{Store, inline_text_body};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::fs;
use std::io::Stdout;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

struct ReplayTask {
    db_path: PathBuf,
    args: Vec<String>,
    session_id: String,
    tab_id: Option<String>,
    run_id: Option<String>,
    request_id: String,
    command: String,
}

pub(super) struct ReplayCompletion {
    pub(super) request_id: String,
    pub(super) status: String,
}

pub(super) fn replay_selected_request(
    app: &mut WorkbenchState,
    replay_tx: &mpsc::Sender<ReplayCompletion>,
) {
    app.hydrate_selected_request();
    let Some(args) = app.replay_curl_args() else {
        app.status = "no request selected".to_string();
        return;
    };
    let Some((session_id, tab_id, run_id, request_id, command)) = app.selected_replay_context()
    else {
        app.status = "no request selected".to_string();
        return;
    };

    if !command_exists("curl") {
        app.status = "cannot replay: curl not found".to_string();
        return;
    }

    start_replay_with_curl(
        app,
        replay_tx,
        ReplayTask {
            db_path: app.db_path.clone(),
            args,
            session_id,
            tab_id,
            run_id,
            request_id,
            command,
        },
    );
    append_audit_event(
        "tui.replay_request",
        serde_json::json!({ "target_url": app.target_url }),
    );
}

pub(super) fn edit_and_replay_selected_request(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
    replay_tx: &mpsc::Sender<ReplayCompletion>,
) -> anyhow::Result<()> {
    app.hydrate_selected_request();
    let Some(editable) = app.selected_editable_request() else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let Some((session_id, tab_id, run_id, request_id, _command)) = app.selected_replay_context()
    else {
        app.status = "no request selected".to_string();
        return Ok(());
    };
    let path = write_temp_file("faro-edit-replay", "http", &editable)
        .context("write editable replay request")?;
    app.status = run_editor(terminal, &path).context("run editor for replay request")?;

    let edited = fs::read_to_string(&path)
        .with_context(|| format!("read edited replay request {}", path.display()))?;
    let Some(args) = parse_edited_request(&edited) else {
        app.status = format!("edited replay parse failed: {}", path.display());
        return Ok(());
    };
    let command = format!(
        "curl {}",
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    );
    start_replay_with_curl(
        app,
        replay_tx,
        ReplayTask {
            db_path: app.db_path.clone(),
            args,
            session_id,
            tab_id,
            run_id,
            request_id,
            command,
        },
    );
    Ok(())
}

fn start_replay_with_curl(
    app: &mut WorkbenchState,
    replay_tx: &mpsc::Sender<ReplayCompletion>,
    task: ReplayTask,
) {
    if !command_exists("curl") {
        app.status = "cannot replay: curl not found".to_string();
        return;
    }

    let tx = replay_tx.clone();
    app.status = "replaying request...".to_string();
    thread::spawn(move || {
        let completion = run_replay_task(task);
        let _ = tx.send(completion);
    });
}

fn run_replay_task(task: ReplayTask) -> ReplayCompletion {
    let mut replay = ReplayRecord::new(
        task.session_id,
        task.tab_id,
        task.run_id,
        task.request_id,
        task.command,
    );
    let request_id = replay.source_request_id.clone();
    match Command::new("curl").args(&task.args).output() {
        Ok(output) => {
            replay.exit_code = output.status.code().map(i64::from);
            replay.status_code = parse_http_status(&output.stdout);
            let mut response_output = Vec::new();
            response_output.extend_from_slice(&output.stdout);
            if !output.stderr.is_empty() {
                response_output.extend_from_slice(b"\n\n--- stderr ---\n");
                response_output.extend_from_slice(&output.stderr);
            }
            match write_temp_bytes("faro-replay", "http", &response_output) {
                Ok(path) => {
                    replay.output_path = Some(path.display().to_string());
                    let body_text = split_http_body(&output.stdout);
                    if !body_text.is_empty() {
                        let body = inline_text_body(None, body_text);
                        replay.response_body_ref = Some(body.id.clone());
                        if let Err(error) =
                            persist_replay_body_and_record_path(&task.db_path, &body, &replay)
                        {
                            return ReplayCompletion {
                                request_id,
                                status: format!("replay persisted failed: {error}"),
                            };
                        }
                    } else if let Err(error) = persist_replay_record_path(&task.db_path, &replay) {
                        return ReplayCompletion {
                            request_id,
                            status: format!("replay persisted failed: {error}"),
                        };
                    }
                    ReplayCompletion {
                        request_id,
                        status: format!(
                            "replayed request -> {} status {} ({})",
                            path.display(),
                            replay
                                .status_code
                                .map(|status| status.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            output.status
                        ),
                    }
                }
                Err(error) => ReplayCompletion {
                    request_id,
                    status: format!("replay ran but writing output failed: {error}"),
                },
            }
        }
        Err(error) => {
            replay.error = Some(error.to_string());
            match persist_replay_record_path(&task.db_path, &replay) {
                Ok(()) => ReplayCompletion {
                    request_id,
                    status: format!("replay failed: {error}"),
                },
                Err(store_error) => ReplayCompletion {
                    request_id,
                    status: format!("replay failed: {error}; persist failed: {store_error}"),
                },
            }
        }
    }
}

pub(super) fn diff_selected_replay(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    app.hydrate_selected_request();
    let Some((original, replay)) = app.selected_replay_diff_bodies() else {
        app.status = "no replay body to diff".to_string();
        return Ok(());
    };
    let original_path =
        write_temp_file("faro-original", "txt", &original).context("write original body")?;
    let replay_path =
        write_temp_file("faro-replay-body", "txt", &replay).context("write replay body")?;

    if command_exists("nvim") {
        suspend_terminal_for_editor(terminal).context("suspend terminal before nvim diff")?;
        let status = Command::new("nvim")
            .args([
                "-d",
                original_path.to_str().unwrap_or(""),
                replay_path.to_str().unwrap_or(""),
            ])
            .status();
        resume_terminal_after_editor(terminal).context("restore terminal after nvim diff")?;
        match status {
            Ok(status) if status.success() => {
                app.status = format!("diff viewed in nvim ({status})");
                return Ok(());
            }
            Ok(status) => {
                app.status = format!("nvim diff exited {status}; writing unified diff");
            }
            Err(error) => {
                app.status = format!("nvim diff failed: {error}; writing unified diff");
            }
        }
    }

    let diff = if command_exists("diff") {
        Command::new("diff")
            .args([
                "-u",
                original_path.to_str().unwrap_or(""),
                replay_path.to_str().unwrap_or(""),
            ])
            .output()
            .map(|output| output.stdout)
            .unwrap_or_else(|_| b"diff failed".to_vec())
    } else {
        b"diff unavailable; original/replay files written".to_vec()
    };
    let diff_path = write_temp_bytes("faro-diff", "diff", &diff).context("write diff file")?;
    app.status = format!("wrote diff {}", diff_path.display());
    Ok(())
}

fn persist_replay_body_and_record_path(
    db_path: &Path,
    body: &faro_core::BodyRecord,
    replay: &ReplayRecord,
) -> anyhow::Result<()> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
    store
        .insert_body(body)
        .context("insert replay response body")?;
    store
        .insert_replay(replay)
        .context("insert replay record")?;
    store
        .append_event(&request_replayed_event(replay))
        .context("append replay event")?;
    Ok(())
}

fn persist_replay_record_path(db_path: &Path, replay: &ReplayRecord) -> anyhow::Result<()> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
    store
        .insert_replay(replay)
        .context("insert replay record")?;
    store
        .append_event(&request_replayed_event(replay))
        .context("append replay event")?;
    Ok(())
}

pub(super) fn drain_replay_updates(
    app: &mut WorkbenchState,
    updates: &mpsc::Receiver<ReplayCompletion>,
) {
    while let Ok(update) = updates.try_recv() {
        app.perf.replay_completed = app.perf.replay_completed.saturating_add(1);
        match app.refresh_replays_for_request(&update.request_id) {
            Ok(()) => app.status = update.status,
            Err(error) => app.status = format!("{}; replay refresh failed: {error}", update.status),
        }
    }
}

fn parse_edited_request(text: &str) -> Option<Vec<String>> {
    let normalized = text.replace("\r\n", "\n");
    let (head, body) = normalized.split_once("\n\n").unwrap_or((&normalized, ""));
    let mut lines = head.lines().filter(|line| !line.trim().is_empty());
    let first = lines.next()?;
    let (method, url) = first.split_once(' ')?;

    let mut args = vec![
        "-sS".to_string(),
        "-i".to_string(),
        "--compressed".to_string(),
        "-X".to_string(),
        method.trim().to_string(),
        url.trim().to_string(),
    ];

    for line in lines {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.contains(':') {
            args.push("-H".to_string());
            args.push(line.trim().to_string());
        }
    }

    if !body.trim().is_empty() {
        args.push("--data-raw".to_string());
        args.push(body.to_string());
    }

    Some(args)
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
