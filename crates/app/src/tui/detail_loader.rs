use super::layout::LayoutMode;
use super::state::{self, FocusPane, ReplayView, WorkbenchState, WorkbenchView};
use anyhow::Context;
use faro_store::Store;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

struct DetailLoadTask {
    db_path: PathBuf,
    request_id: String,
    request_body_ref: Option<String>,
    response_body_ref: Option<String>,
}

struct DetailLoadResult {
    request_body: Option<String>,
    response_body: Option<String>,
    replays: Vec<ReplayView>,
}

pub(super) struct DetailLoadCompletion {
    request_id: String,
    result: anyhow::Result<DetailLoadResult>,
}

pub(super) fn maybe_start_selected_detail_load(
    app: &mut WorkbenchState,
    tx: &mpsc::Sender<DetailLoadCompletion>,
    inflight: &mut HashSet<String>,
    pending: &mut Option<(String, Instant)>,
) {
    let Some(task) = selected_detail_load_task(app) else {
        *pending = None;
        return;
    };
    if inflight.contains(&task.request_id) {
        return;
    }

    let now = Instant::now();
    match pending {
        Some((request_id, since)) if request_id == &task.request_id => {
            if now.duration_since(*since) < Duration::from_millis(120) {
                return;
            }
        }
        _ => {
            *pending = Some((task.request_id.clone(), now));
            return;
        }
    }

    inflight.insert(task.request_id.clone());
    app.perf.detail_load_started = app.perf.detail_load_started.saturating_add(1);
    *pending = None;
    let tx = tx.clone();
    thread::spawn(move || {
        let request_id = task.request_id.clone();
        let result = run_detail_load_task(task);
        let _ = tx.send(DetailLoadCompletion { request_id, result });
    });
}

fn selected_detail_load_task(app: &WorkbenchState) -> Option<DetailLoadTask> {
    if !selected_details_are_visible(app) {
        return None;
    }
    let request = app.selected_request()?;
    if request.details_loaded {
        return None;
    }
    Some(DetailLoadTask {
        db_path: app.db_path.clone(),
        request_id: request.request.id.clone(),
        request_body_ref: request.request.request_body_ref.clone(),
        response_body_ref: request
            .response
            .as_ref()
            .and_then(|response| response.body_ref.clone()),
    })
}

fn selected_details_are_visible(app: &WorkbenchState) -> bool {
    if app.view != WorkbenchView::Network {
        return false;
    }
    match app.layout_mode {
        LayoutMode::Normal => true,
        LayoutMode::Focused => matches!(app.focus, FocusPane::Detail | FocusPane::Body),
    }
}

fn run_detail_load_task(task: DetailLoadTask) -> anyhow::Result<DetailLoadResult> {
    let store = Store::open(&task.db_path)
        .with_context(|| format!("open database {}", task.db_path.display()))?;
    let request_body = state::body_text_for_ref(&store, task.request_body_ref.as_deref())?;
    let response_body = state::body_text_for_ref(&store, task.response_body_ref.as_deref())?;
    let replays = store
        .replays_for_request(&task.request_id)?
        .into_iter()
        .map(|record| state::replay_view_for_record(&store, record))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(DetailLoadResult {
        request_body,
        response_body,
        replays,
    })
}

pub(super) fn drain_detail_updates(
    app: &mut WorkbenchState,
    updates: &mpsc::Receiver<DetailLoadCompletion>,
    inflight: &mut HashSet<String>,
) {
    while let Ok(update) = updates.try_recv() {
        inflight.remove(&update.request_id);
        app.perf.detail_load_completed = app.perf.detail_load_completed.saturating_add(1);
        match update.result {
            Ok(details) => app.apply_request_details(
                &update.request_id,
                details.request_body,
                details.response_body,
                details.replays,
            ),
            Err(error) => {
                app.status = format!("request detail load failed: {error}");
                app.note_status_changed();
            }
        }
    }
}
