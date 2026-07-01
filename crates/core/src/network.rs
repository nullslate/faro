use crate::{EventEnvelope, EventKind, Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::string::FromUtf8Error;

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
