use crate::{EventEnvelope, EventKind, Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CookieSnapshotRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub ts: UnixMillis,
    pub url: Option<String>,
    pub cookies: Vec<CookieRecord>,
}

impl CookieSnapshotRecord {
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        url: Option<String>,
        cookies: Vec<CookieRecord>,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            ts: now_ms(),
            url,
            cookies,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CookieRecord {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<f64>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
}

pub fn cookie_observed_event(snapshot: &CookieSnapshotRecord) -> EventEnvelope {
    EventEnvelope::new(
        snapshot.session_id.clone(),
        snapshot.tab_id.clone(),
        snapshot.run_id.clone(),
        EventKind::CookieObserved,
        json!({
            "cookie_snapshot_id": snapshot.id,
            "url": snapshot.url,
            "cookie_count": snapshot.cookies.len()
        }),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CookieEventRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub ts: UnixMillis,
    pub operation: String,
    pub name: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub value: Option<String>,
    pub attributes_json: Option<Value>,
}

impl CookieEventRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        operation: impl Into<String>,
        name: Option<String>,
        domain: Option<String>,
        path: Option<String>,
        value: Option<String>,
        attributes_json: Option<Value>,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            ts: now_ms(),
            operation: operation.into(),
            name,
            domain,
            path,
            value,
            attributes_json,
        }
    }
}

pub fn cookie_event_observed_event(event: &CookieEventRecord) -> EventEnvelope {
    EventEnvelope::new(
        event.session_id.clone(),
        event.tab_id.clone(),
        event.run_id.clone(),
        EventKind::CookieObserved,
        json!({
            "cookie_event_id": event.id,
            "operation": event.operation,
            "name": event.name,
            "domain": event.domain,
            "path": event.path
        }),
    )
}
