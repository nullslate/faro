use crate::{EventEnvelope, EventKind, Id, UnixMillis, new_id, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
