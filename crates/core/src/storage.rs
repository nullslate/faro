use crate::{EventEnvelope, EventKind, Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
