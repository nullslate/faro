use crate::config::RetentionConfig;
use anyhow::Context;
use faro_core::{ConsoleLog, RequestRecord, ResponseRecord, WebSocketFrameRecord};
use faro_store::Store;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub(super) struct LiveRefreshRequest {
    pub(super) db_path: PathBuf,
    pub(super) session_id: String,
    pub(super) retention: RetentionConfig,
    pub(super) max_started_at: i64,
    pub(super) max_completed_at: i64,
    pub(super) max_response_at: i64,
    pub(super) max_console_ts: i64,
    pub(super) max_websocket_ts: i64,
    pub(super) prune_retention: bool,
}

#[derive(Debug)]
pub(super) enum LiveRefreshCompletion {
    Loaded(LiveRefreshDelta),
    Failed(String),
}

#[derive(Debug)]
pub(super) struct LiveRefreshDelta {
    pub(super) retained_request_ids: Option<Vec<String>>,
    pub(super) retained_console_log_ids: Option<Vec<String>>,
    pub(super) retained_websocket_frame_ids: Option<Vec<String>>,
    pub(super) changed_requests: Vec<RequestRecord>,
    pub(super) changed_responses: Vec<ResponseRecord>,
    pub(super) new_console_logs: Vec<ConsoleLog>,
    pub(super) new_websocket_frames: Vec<WebSocketFrameRecord>,
    pub(super) retention_prune_ran: bool,
    pub(super) duration_ms: u128,
}

impl LiveRefreshDelta {
    pub(super) fn is_empty(&self) -> bool {
        self.retained_request_ids.is_none()
            && self.retained_console_log_ids.is_none()
            && self.retained_websocket_frame_ids.is_none()
            && self.changed_requests.is_empty()
            && self.changed_responses.is_empty()
            && self.new_console_logs.is_empty()
            && self.new_websocket_frames.is_empty()
            && !self.retention_prune_ran
    }
}

pub(super) fn spawn_live_refresh_worker() -> (
    mpsc::Sender<LiveRefreshRequest>,
    mpsc::Receiver<LiveRefreshCompletion>,
) {
    let (request_tx, request_rx) = mpsc::channel::<LiveRefreshRequest>();
    let (completion_tx, completion_rx) = mpsc::channel::<LiveRefreshCompletion>();
    std::thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let completion = match load_live_refresh_delta_for_state(request) {
                Ok(delta) => LiveRefreshCompletion::Loaded(delta),
                Err(error) => LiveRefreshCompletion::Failed(error.to_string()),
            };
            if completion_tx.send(completion).is_err() {
                break;
            }
        }
    });
    (request_tx, completion_rx)
}

pub(super) fn load_live_refresh_delta_for_state(
    request: LiveRefreshRequest,
) -> anyhow::Result<LiveRefreshDelta> {
    let started = Instant::now();
    let store = Store::open(&request.db_path)
        .with_context(|| format!("open database {}", request.db_path.display()))?;
    let repeated_pruned = if request.prune_retention {
        store
            .prune_repeated_session_requests(
                &request.session_id,
                request.retention.max_repeated_requests_per_url,
            )
            .with_context(|| {
                format!("prune repeated requests for session {}", request.session_id)
            })?
    } else {
        0
    };
    let ring_pruned = if request.prune_retention {
        store
            .prune_session_requests(
                &request.session_id,
                request.retention.max_requests_per_session,
            )
            .with_context(|| format!("prune requests for session {}", request.session_id))?
    } else {
        0
    };
    let retained_request_ids = if repeated_pruned + ring_pruned > 0 {
        Some(
            store
                .request_ids_for_session(&request.session_id)
                .with_context(|| {
                    format!(
                        "load retained request ids for session {}",
                        request.session_id
                    )
                })?,
        )
    } else {
        None
    };
    let console_pruned = if request.prune_retention {
        store
            .prune_session_console_logs(
                &request.session_id,
                request.retention.max_console_logs_per_session,
            )
            .with_context(|| format!("prune console logs for session {}", request.session_id))?
    } else {
        0
    };
    let retained_console_log_ids = if console_pruned > 0 {
        Some(
            store
                .console_log_ids_for_session(&request.session_id)
                .with_context(|| {
                    format!(
                        "load retained console log ids for session {}",
                        request.session_id
                    )
                })?,
        )
    } else {
        None
    };
    let websocket_pruned = if request.prune_retention {
        store
            .prune_session_websocket_frames(
                &request.session_id,
                request.retention.max_websocket_frames_per_session,
            )
            .with_context(|| format!("prune websocket frames for session {}", request.session_id))?
    } else {
        0
    };
    let retained_websocket_frame_ids = if websocket_pruned > 0 {
        Some(
            store
                .websocket_frame_ids_for_session(&request.session_id)
                .with_context(|| {
                    format!(
                        "load retained websocket frame ids for session {}",
                        request.session_id
                    )
                })?,
        )
    } else {
        None
    };
    let changed_requests = store
        .requests_for_session_changed_after(
            &request.session_id,
            request.max_started_at,
            request.max_completed_at,
        )
        .with_context(|| format!("load changed requests for session {}", request.session_id))?;
    let changed_responses = store
        .responses_for_session_after(&request.session_id, request.max_response_at)
        .with_context(|| format!("load changed responses for session {}", request.session_id))?;
    let new_console_logs = store
        .console_logs_for_session_after(&request.session_id, request.max_console_ts)
        .with_context(|| format!("load console logs for session {}", request.session_id))?;
    let new_websocket_frames = store
        .websocket_frames_for_session_after(&request.session_id, request.max_websocket_ts)
        .with_context(|| format!("load websocket frames for session {}", request.session_id))?;

    Ok(LiveRefreshDelta {
        retained_request_ids,
        retained_console_log_ids,
        retained_websocket_frame_ids,
        changed_requests,
        changed_responses,
        new_console_logs,
        new_websocket_frames,
        retention_prune_ran: request.prune_retention,
        duration_ms: started.elapsed().as_millis(),
    })
}
