use crate::{Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};

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
