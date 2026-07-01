use crate::{EventEnvelope, EventKind, Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub source_request_id: Id,
    pub ts: UnixMillis,
    pub command: String,
    pub exit_code: Option<i64>,
    pub status_code: Option<i64>,
    pub response_body_ref: Option<Id>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

impl ReplayRecord {
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        source_request_id: Id,
        command: String,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            source_request_id,
            ts: now_ms(),
            command,
            exit_code: None,
            status_code: None,
            response_body_ref: None,
            output_path: None,
            error: None,
        }
    }
}

pub fn request_replayed_event(replay: &ReplayRecord) -> EventEnvelope {
    EventEnvelope::new(
        replay.session_id.clone(),
        replay.tab_id.clone(),
        replay.run_id.clone(),
        EventKind::RequestReplayed,
        json!({
            "replay_id": replay.id,
            "source_request_id": replay.source_request_id,
            "exit_code": replay.exit_code,
            "status_code": replay.status_code,
            "response_body_ref": replay.response_body_ref,
            "output_path": replay.output_path,
            "error": replay.error
        }),
    )
}
