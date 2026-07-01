use super::state::WorkbenchState;
use anyhow::Context;
use faro_store::Store;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

struct SessionDeleteTask {
    db_path: PathBuf,
    session_id: String,
}

pub(super) struct SessionDeleteCompletion {
    session_id: String,
    result: anyhow::Result<usize>,
}

pub(super) fn switch_selected_session(app: &mut WorkbenchState) -> anyhow::Result<()> {
    let Some(session_id) = app.selected_session_id() else {
        app.status = "no session selected".to_string();
        return Ok(());
    };
    let store = Store::open(&app.db_path)
        .with_context(|| format!("open database {}", app.db_path.display()))?;
    let last_sql_query = app.last_sql_query.clone();
    let view = app.view;
    let focus = app.focus;
    let mut loaded = WorkbenchState::load_for_session(
        &store,
        &app.db_path,
        &app.target_url,
        app.config.clone(),
        Some(&session_id),
    )
    .with_context(|| format!("load session {session_id} from {}", app.db_path.display()))?;
    loaded.last_sql_query = last_sql_query;
    loaded.view = view;
    loaded.focus = focus;
    loaded.show_sessions = false;
    loaded.status = format!("opened session {}", compact_id(&session_id));
    *app = loaded;
    Ok(())
}

pub(super) fn delete_selected_session(
    app: &mut WorkbenchState,
    tx: &mpsc::Sender<SessionDeleteCompletion>,
    inflight: &mut HashSet<String>,
) {
    let Some(session_id) = app.selected_session_id() else {
        app.status = "no session selected".to_string();
        return;
    };
    if inflight.contains(&session_id) {
        app.status = format!("already deleting session {}", compact_id(&session_id));
        return;
    }

    inflight.insert(session_id.clone());
    app.remove_session_optimistic(&session_id);
    app.status = format!("deleting session {}", compact_id(&session_id));
    let task = SessionDeleteTask {
        db_path: app.db_path.clone(),
        session_id,
    };
    let tx = tx.clone();
    thread::spawn(move || {
        let session_id = task.session_id.clone();
        let result = delete_session_task(task);
        let _ = tx.send(SessionDeleteCompletion { session_id, result });
    });
}

fn delete_session_task(task: SessionDeleteTask) -> anyhow::Result<usize> {
    let store = Store::open(&task.db_path)
        .with_context(|| format!("open database {}", task.db_path.display()))?;
    store
        .delete_session(&task.session_id)
        .with_context(|| format!("delete session {}", task.session_id))
}

pub(super) fn drain_session_delete_updates(
    app: &mut WorkbenchState,
    updates: &mpsc::Receiver<SessionDeleteCompletion>,
    inflight: &mut HashSet<String>,
) {
    while let Ok(update) = updates.try_recv() {
        inflight.remove(&update.session_id);
        match update.result {
            Ok(0) => {
                app.status = format!(
                    "session {} was already gone",
                    compact_id(&update.session_id)
                )
            }
            Ok(_) => app.status = format!("deleted session {}", compact_id(&update.session_id)),
            Err(error) => {
                app.status = format!(
                    "delete session {} failed: {error}",
                    compact_id(&update.session_id)
                );
                if let Ok(store) = Store::open(&app.db_path)
                    && let Ok(mut loaded) = WorkbenchState::load_for_session(
                        &store,
                        &app.db_path,
                        &app.target_url,
                        app.config.clone(),
                        app.active_session_id.as_deref(),
                    )
                {
                    loaded.last_sql_query = app.last_sql_query.clone();
                    loaded.view = app.view;
                    loaded.focus = app.focus;
                    loaded.show_sessions = true;
                    loaded.status = app.status.clone();
                    *app = loaded;
                }
            }
        }
        app.open_sessions();
        app.note_status_changed();
    }
}

fn compact_id(id: &str) -> String {
    id.chars().take(8).collect()
}
