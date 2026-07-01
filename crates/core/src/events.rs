use crate::{Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub ts: UnixMillis,
    pub kind: EventKind,
    pub payload: Value,
}

impl EventEnvelope {
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        kind: EventKind,
        payload: Value,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            ts: now_ms(),
            kind,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    BrowserCreated,
    NavigationStarted,
    NavigationFinished,
    RequestStarted,
    ResponseReceived,
    RequestCompleted,
    ConsoleLogged,
    StorageChanged,
    PageRouteChanged,
    PageError,
    StorageSnapshotCreated,
    CookieObserved,
    RequestReplayed,
    WebSocketFrame,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BrowserCreated => "browser_created",
            Self::NavigationStarted => "navigation_started",
            Self::NavigationFinished => "navigation_finished",
            Self::RequestStarted => "request_started",
            Self::ResponseReceived => "response_received",
            Self::RequestCompleted => "request_completed",
            Self::ConsoleLogged => "console_logged",
            Self::StorageChanged => "storage_changed",
            Self::PageRouteChanged => "page_route_changed",
            Self::PageError => "page_error",
            Self::StorageSnapshotCreated => "storage_snapshot_created",
            Self::CookieObserved => "cookie_observed",
            Self::RequestReplayed => "request_replayed",
            Self::WebSocketFrame => "websocket_frame",
        }
    }
}
