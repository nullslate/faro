use faro_core::{new_id, now_ms};

#[derive(Debug, Clone, Copy, Default)]
pub struct SessionSummaryCounts {
    pub requests: usize,
    pub console_errors: usize,
    pub replays: usize,
    pub websocket_frames: usize,
    pub storage_events: usize,
    pub cookie_events: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRowCount {
    pub table: String,
    pub rows: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BodyStorageStats {
    pub bodies: usize,
    pub total_bytes: u64,
    pub inline_bytes: u64,
    pub external_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStorageStats {
    pub id: String,
    pub created_at: i64,
    pub name: Option<String>,
    pub root_url: Option<String>,
    pub requests: usize,
    pub console_errors: usize,
    pub replays: usize,
    pub websocket_frames: usize,
    pub storage_events: usize,
    pub cookie_events: usize,
    pub bodies: usize,
    pub body_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedRequestGroup {
    pub session_id: String,
    pub root_url: Option<String>,
    pub method: String,
    pub resource_type: Option<String>,
    pub url: String,
    pub requests: usize,
    pub error_responses: usize,
    pub body_bytes: u64,
    pub first_started_at: i64,
    pub last_started_at: i64,
}

#[derive(Debug, Clone)]
pub struct ScriptRecord {
    pub id: String,
    pub name: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_run_at: Option<i64>,
}

impl ScriptRecord {
    pub fn new(name: impl Into<String>, body: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            name: name.into(),
            body: body.into(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
        }
    }
}
