use crate::{EventEnvelope, EventKind, Id};
use serde_json::json;

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
