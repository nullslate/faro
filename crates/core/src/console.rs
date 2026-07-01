use crate::{EventEnvelope, EventKind, Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
