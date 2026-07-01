use crate::query::RequestSort;
use faro_core::{ReplayRecord, RequestRecord, ResponseRecord, Session};

#[derive(Debug, Clone)]
pub(crate) struct SessionView {
    pub(crate) session: Session,
    pub(crate) request_count: usize,
    pub(crate) console_error_count: usize,
    pub(crate) replay_count: usize,
    pub(crate) websocket_count: usize,
    pub(crate) storage_count: usize,
    pub(crate) cookie_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusPane {
    Requests,
    Detail,
    Body,
    Console,
    WebSockets,
    Scripts,
    Storage,
    Cookies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkbenchView {
    Network,
    Console,
    WebSockets,
    Scripts,
    Storage,
    Cookies,
}

impl WorkbenchView {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Console => "console",
            Self::WebSockets => "websockets",
            Self::Scripts => "scripts",
            Self::Storage => "storage",
            Self::Cookies => "cookies",
        }
    }
}

impl FocusPane {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::Detail => "detail",
            Self::Body => "body",
            Self::Console => "console",
            Self::WebSockets => "websockets",
            Self::Scripts => "scripts",
            Self::Storage => "storage",
            Self::Cookies => "cookies",
        }
    }

    pub(crate) fn parse(value: &str) -> Self {
        match value.trim() {
            "detail" => Self::Detail,
            "body" => Self::Body,
            "console" => Self::Console,
            "websockets" => Self::WebSockets,
            "scripts" => Self::Scripts,
            "storage" => Self::Storage,
            "cookies" => Self::Cookies,
            _ => Self::Requests,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailTab {
    Overview,
    RequestHeaders,
    RequestBody,
    ResponseHeaders,
    ResponseBody,
    Timing,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortMode {
    Started,
    Status,
    Duration,
    Size,
    Method,
}

impl SortMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Status => "status",
            Self::Duration => "duration",
            Self::Size => "size",
            Self::Method => "method",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Started => Self::Status,
            Self::Status => Self::Duration,
            Self::Duration => Self::Size,
            Self::Size => Self::Method,
            Self::Method => Self::Started,
        }
    }
}

impl From<SortMode> for RequestSort {
    fn from(value: SortMode) -> Self {
        match value {
            SortMode::Started => Self::Started,
            SortMode::Status => Self::Status,
            SortMode::Duration => Self::Duration,
            SortMode::Size => Self::Size,
            SortMode::Method => Self::Method,
        }
    }
}

impl DetailTab {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Overview => Self::RequestHeaders,
            Self::RequestHeaders => Self::RequestBody,
            Self::RequestBody => Self::ResponseHeaders,
            Self::ResponseHeaders => Self::ResponseBody,
            Self::ResponseBody => Self::Timing,
            Self::Timing => Self::Replay,
            Self::Replay => Self::Overview,
        }
    }

    pub(crate) fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Replay,
            Self::RequestHeaders => Self::Overview,
            Self::RequestBody => Self::RequestHeaders,
            Self::ResponseHeaders => Self::RequestBody,
            Self::ResponseBody => Self::ResponseHeaders,
            Self::Timing => Self::ResponseBody,
            Self::Replay => Self::Timing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputMode {
    Normal,
    Filtering,
    Palette,
    BodySearch,
}

impl InputMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Filtering => "filter",
            Self::Palette => "palette",
            Self::BodySearch => "body-search",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutPreset {
    CompactNetwork,
    BodyHeavy,
    ConsoleHeavy,
    WebSocketHeavy,
}

#[derive(Debug, Clone)]
pub(crate) struct SqlResultsView {
    pub(crate) query: String,
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) duration_ms: u128,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RouteSummary {
    pub(crate) count: usize,
    pub(crate) errors: usize,
    pub(crate) pending: usize,
    pub(crate) slow: usize,
    pub(crate) total_size: i64,
    pub(crate) max_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PerfStats {
    pub(crate) frame_count: u64,
    pub(crate) last_frame_ms: u128,
    pub(crate) max_frame_ms: u128,
    pub(crate) last_tick_ms: u128,
    pub(crate) max_tick_ms: u128,
    pub(crate) last_poll_ms: u128,
    pub(crate) max_poll_ms: u128,
    pub(crate) last_capture_drain_ms: u128,
    pub(crate) last_replay_drain_ms: u128,
    pub(crate) last_detail_drain_ms: u128,
    pub(crate) detail_load_started: u64,
    pub(crate) detail_load_completed: u64,
    pub(crate) replay_completed: u64,
}

pub(crate) struct RequestView {
    pub(crate) request: RequestRecord,
    pub(crate) response: Option<ResponseRecord>,
    pub(crate) request_body: Option<String>,
    pub(crate) response_body: Option<String>,
    pub(crate) replays: Vec<ReplayView>,
    pub(crate) details_loaded: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentStorageEntry {
    pub(crate) storage_type: String,
    pub(crate) origin: String,
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CurrentCookieEntry {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) expires: Option<f64>,
    pub(crate) http_only: bool,
    pub(crate) secure: bool,
    pub(crate) same_site: Option<String>,
    pub(crate) flags: String,
}

impl CurrentCookieEntry {
    pub(crate) fn to_cookie_record(&self) -> faro_core::CookieRecord {
        faro_core::CookieRecord {
            name: self.name.clone(),
            value: self.value.clone(),
            domain: self.domain.clone(),
            path: self.path.clone(),
            expires: self.expires,
            http_only: self.http_only,
            secure: self.secure,
            same_site: self.same_site.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestTreeMeta {
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) depth: usize,
    pub(crate) group_key: Option<String>,
    pub(crate) ancestor_keys: Vec<String>,
    pub(crate) has_children: bool,
    pub(crate) child_count: usize,
    pub(crate) collapsed: bool,
}

#[derive(Clone)]
pub(crate) struct BodyTreeItem {
    pub(crate) key: String,
    pub(crate) depth: usize,
    pub(crate) label: String,
    pub(crate) value: Option<String>,
    pub(crate) expandable: bool,
    pub(crate) collapsed: bool,
}

pub(crate) struct ReplayView {
    pub(crate) record: ReplayRecord,
    pub(crate) body: Option<String>,
}

impl RequestView {
    pub(crate) fn status_code(&self) -> Option<i64> {
        self.response
            .as_ref()
            .and_then(|response| response.status_code)
    }

    pub(crate) fn duration_ms(&self) -> Option<i64> {
        Some(self.request.completed_at? - self.request.started_at)
    }
}
