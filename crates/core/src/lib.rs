use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::env;
use std::path::PathBuf;
use std::string::FromUtf8Error;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub type Id = String;
pub type UnixMillis = i64;

pub fn new_id() -> Id {
    Uuid::new_v4().to_string()
}

pub fn now_ms() -> UnixMillis {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_millis() as UnixMillis,
        Err(_) => 0,
    }
}

pub fn config_dir(app_dir: &str) -> Option<PathBuf> {
    if let Some(config_home) = env_var_nonempty("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join(app_dir));
    }

    platform_config_dir(app_dir)
}

fn env_var_nonempty(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        Ok(_) | Err(_) => None,
    }
}

#[cfg(target_os = "windows")]
fn platform_config_dir(app_dir: &str) -> Option<PathBuf> {
    env_var_nonempty("APPDATA")
        .or_else(|| env_var_nonempty("LOCALAPPDATA"))
        .map(PathBuf::from)
        .map(|path| path.join(app_dir))
        .or_else(|| {
            env_var_nonempty("USERPROFILE")
                .map(PathBuf::from)
                .map(|path| path.join("AppData").join("Roaming").join(app_dir))
        })
}

#[cfg(target_os = "macos")]
fn platform_config_dir(app_dir: &str) -> Option<PathBuf> {
    env_var_nonempty("HOME").map(PathBuf::from).map(|path| {
        path.join("Library")
            .join("Application Support")
            .join(app_dir)
    })
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn platform_config_dir(app_dir: &str) -> Option<PathBuf> {
    env_var_nonempty("HOME")
        .map(PathBuf::from)
        .map(|path| path.join(".config").join(app_dir))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub id: Id,
    pub created_at: UnixMillis,
    pub name: Option<String>,
    pub root_url: Option<String>,
}

impl Session {
    pub fn new(name: Option<String>, root_url: Option<String>) -> Self {
        Self {
            id: new_id(),
            created_at: now_ms(),
            name,
            root_url,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tab {
    pub id: Id,
    pub session_id: Id,
    pub created_at: UnixMillis,
    pub current_url: Option<String>,
    pub title: Option<String>,
}

impl Tab {
    pub fn new(session_id: Id, current_url: Option<String>) -> Self {
        Self {
            id: new_id(),
            session_id,
            created_at: now_ms(),
            current_url,
            title: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Run {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Id,
    pub started_at: UnixMillis,
    pub ended_at: Option<UnixMillis>,
    pub url: String,
    pub trigger: RunTrigger,
}

impl Run {
    pub fn new(session_id: Id, tab_id: Id, url: String, trigger: RunTrigger) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            started_at: now_ms(),
            ended_at: None,
            url,
            trigger,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    InitialLoad,
    Reload,
    Navigation,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsoleLog {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub ts: UnixMillis,
    pub level: ConsoleLevel,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<i64>,
    pub stack_json: Option<Value>,
}

impl ConsoleLog {
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        level: ConsoleLevel,
        message: String,
        source: Option<String>,
        line: Option<i64>,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            ts: now_ms(),
            level,
            message,
            source,
            line,
            stack_json: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
}

impl ConsoleLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }
}

pub fn console_event(log: &ConsoleLog) -> EventEnvelope {
    EventEnvelope::new(
        log.session_id.clone(),
        log.tab_id.clone(),
        log.run_id.clone(),
        EventKind::ConsoleLogged,
        json!({
            "console_log_id": log.id,
            "level": log.level,
            "message": log.message,
            "source": log.source,
            "line": log.line
        }),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl Header {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub browser_request_id: Option<String>,
    pub started_at: UnixMillis,
    pub completed_at: Option<UnixMillis>,
    pub method: String,
    pub url: String,
    pub resource_type: Option<String>,
    pub initiator: Option<String>,
    pub request_headers: Vec<Header>,
    pub request_body_ref: Option<Id>,
    pub status: RequestStatus,
}

impl RequestRecord {
    pub fn started(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        method: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            browser_request_id: None,
            started_at: now_ms(),
            completed_at: None,
            method: method.into(),
            url: url.into(),
            resource_type: None,
            initiator: None,
            request_headers: Vec::new(),
            request_body_ref: None,
            status: RequestStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Complete,
    Failed,
    Canceled,
}

impl RequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseRecord {
    pub id: Id,
    pub request_id: Id,
    pub received_at: UnixMillis,
    pub status_code: Option<i64>,
    pub status_text: Option<String>,
    pub mime_type: Option<String>,
    pub response_headers: Vec<Header>,
    pub body_ref: Option<Id>,
    pub body_size: Option<i64>,
    pub body_truncated: bool,
}

impl ResponseRecord {
    pub fn received(request_id: Id) -> Self {
        Self {
            id: new_id(),
            request_id,
            received_at: now_ms(),
            status_code: None,
            status_text: None,
            mime_type: None,
            response_headers: Vec::new(),
            body_ref: None,
            body_size: None,
            body_truncated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BodyRecord {
    pub id: Id,
    pub content_type: Option<String>,
    pub encoding: String,
    pub size: i64,
    pub sha256: String,
    pub storage_kind: String,
    pub data: Vec<u8>,
}

impl BodyRecord {
    pub fn inline_text(content_type: Option<String>, text: String, sha256: String) -> Self {
        let data = text.into_bytes();
        Self {
            id: new_id(),
            content_type,
            encoding: "utf-8".to_string(),
            size: data.len() as i64,
            sha256,
            storage_kind: "inline".to_string(),
            data,
        }
    }

    pub fn as_text(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.data.clone())
    }
}

pub fn request_started_event(request: &RequestRecord) -> EventEnvelope {
    EventEnvelope::new(
        request.session_id.clone(),
        request.tab_id.clone(),
        request.run_id.clone(),
        EventKind::RequestStarted,
        json!({
            "request_id": request.id,
            "browser_request_id": request.browser_request_id,
            "method": request.method,
            "url": request.url,
            "resource_type": request.resource_type,
            "initiator": request.initiator
        }),
    )
}

pub fn response_received_event(
    session_id: Id,
    tab_id: Option<Id>,
    run_id: Option<Id>,
    response: &ResponseRecord,
) -> EventEnvelope {
    EventEnvelope::new(
        session_id,
        tab_id,
        run_id,
        EventKind::ResponseReceived,
        json!({
            "response_id": response.id,
            "request_id": response.request_id,
            "status_code": response.status_code,
            "status_text": response.status_text,
            "mime_type": response.mime_type,
            "body_size": response.body_size,
            "body_truncated": response.body_truncated
        }),
    )
}

pub fn request_completed_event(request: &RequestRecord) -> EventEnvelope {
    EventEnvelope::new(
        request.session_id.clone(),
        request.tab_id.clone(),
        request.run_id.clone(),
        EventKind::RequestCompleted,
        json!({
            "request_id": request.id,
            "completed_at": request.completed_at,
            "status": request.status
        }),
    )
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageEventRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub ts: UnixMillis,
    pub origin: String,
    pub storage_type: String,
    pub operation: String,
    pub key: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub stack_json: Option<Value>,
}

impl StorageEventRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        origin: String,
        storage_type: String,
        operation: String,
        key: Option<String>,
        old_value: Option<String>,
        new_value: Option<String>,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            ts: now_ms(),
            origin,
            storage_type,
            operation,
            key,
            old_value,
            new_value,
            stack_json: None,
        }
    }
}

pub fn storage_changed_event(event: &StorageEventRecord) -> EventEnvelope {
    EventEnvelope::new(
        event.session_id.clone(),
        event.tab_id.clone(),
        event.run_id.clone(),
        EventKind::StorageChanged,
        json!({
            "storage_event_id": event.id,
            "origin": event.origin,
            "storage_type": event.storage_type,
            "operation": event.operation,
            "key": event.key,
            "old_value": event.old_value,
            "new_value": event.new_value
        }),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageSnapshotRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub ts: UnixMillis,
    pub origin: String,
    pub storage_type: String,
    pub entries: Vec<StorageEntry>,
    pub sha256: String,
}

impl StorageSnapshotRecord {
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        origin: String,
        storage_type: String,
        entries: Vec<StorageEntry>,
        sha256: String,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            ts: now_ms(),
            origin,
            storage_type,
            entries,
            sha256,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
}

impl StorageEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

pub fn storage_snapshot_created_event(snapshot: &StorageSnapshotRecord) -> EventEnvelope {
    EventEnvelope::new(
        snapshot.session_id.clone(),
        snapshot.tab_id.clone(),
        snapshot.run_id.clone(),
        EventKind::StorageSnapshotCreated,
        json!({
            "storage_snapshot_id": snapshot.id,
            "origin": snapshot.origin,
            "storage_type": snapshot.storage_type,
            "entry_count": snapshot.entries.len(),
            "sha256": snapshot.sha256
        }),
    )
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebSocketFrameRecord {
    pub id: Id,
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub browser_request_id: String,
    pub ts: UnixMillis,
    pub direction: WebSocketFrameDirection,
    pub opcode: i64,
    pub mask: bool,
    pub payload: String,
}

impl WebSocketFrameRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: Id,
        tab_id: Option<Id>,
        run_id: Option<Id>,
        browser_request_id: String,
        direction: WebSocketFrameDirection,
        opcode: i64,
        mask: bool,
        payload: String,
    ) -> Self {
        Self {
            id: new_id(),
            session_id,
            tab_id,
            run_id,
            browser_request_id,
            ts: now_ms(),
            direction,
            opcode,
            mask,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebSocketFrameDirection {
    Sent,
    Received,
}

impl WebSocketFrameDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Received => "received",
        }
    }
}

pub fn websocket_frame_event(frame: &WebSocketFrameRecord) -> EventEnvelope {
    EventEnvelope::new(
        frame.session_id.clone(),
        frame.tab_id.clone(),
        frame.run_id.clone(),
        EventKind::WebSocketFrame,
        json!({
            "websocket_frame_id": frame.id,
            "browser_request_id": frame.browser_request_id,
            "direction": frame.direction,
            "opcode": frame.opcode,
            "mask": frame.mask,
            "payload_size": frame.payload.len()
        }),
    )
}

pub fn page_route_changed_event(
    session_id: Id,
    tab_id: Option<Id>,
    run_id: Option<Id>,
    url: String,
    operation: String,
) -> EventEnvelope {
    EventEnvelope::new(
        session_id,
        tab_id,
        run_id,
        EventKind::PageRouteChanged,
        json!({
            "url": url,
            "operation": operation
        }),
    )
}

pub fn page_error_event(
    session_id: Id,
    tab_id: Option<Id>,
    run_id: Option<Id>,
    message: String,
    source: Option<String>,
    line: Option<i64>,
    kind: String,
) -> EventEnvelope {
    EventEnvelope::new(
        session_id,
        tab_id,
        run_id,
        EventKind::PageError,
        json!({
            "message": message,
            "source": source,
            "line": line,
            "kind": kind
        }),
    )
}
