use faro_core::{
    ConsoleLevel, ConsoleLog, EventEnvelope, Header, Id, RequestRecord, RequestStatus,
    ResponseRecord, StorageEventRecord, console_event, now_ms, page_error_event,
    page_route_changed_event, request_completed_event, request_started_event,
    response_received_event, storage_changed_event,
};
use faro_store::{Store, StoreError, inline_text_body};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("unknown browser request id: {0}")]
    UnknownBrowserRequest(String),
}

pub type Result<T> = std::result::Result<T, AdapterError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserEvent {
    ConsoleLogged(ConsoleLogged),
    RequestStarted(RequestStarted),
    ResponseReceived(ResponseReceived),
    RequestCompleted(RequestCompleted),
    StorageChanged(StorageChanged),
    PageRouteChanged(PageRouteChanged),
    PageError(PageError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsoleLogged {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub level: ConsoleLevel,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestStarted {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub browser_request_id: String,
    pub method: String,
    pub url: String,
    pub resource_type: Option<String>,
    pub initiator: Option<String>,
    pub headers: Vec<Header>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseReceived {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub browser_request_id: String,
    pub status_code: Option<i64>,
    pub status_text: Option<String>,
    pub mime_type: Option<String>,
    pub headers: Vec<Header>,
    pub body_size: Option<i64>,
    pub body_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestCompleted {
    pub browser_request_id: String,
    pub status: RequestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageChanged {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub origin: String,
    pub storage_type: String,
    pub operation: String,
    pub key: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageRouteChanged {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub url: String,
    pub operation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageError {
    pub session_id: Id,
    pub tab_id: Option<Id>,
    pub run_id: Option<Id>,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<i64>,
    pub kind: String,
}

#[derive(Debug, Default)]
pub struct EventIngestor {
    request_ids: HashMap<String, RequestRecord>,
}

impl EventIngestor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest(&mut self, store: &Store, event: BrowserEvent) -> Result<Vec<EventEnvelope>> {
        match event {
            BrowserEvent::ConsoleLogged(event) => self.ingest_console(store, event),
            BrowserEvent::RequestStarted(event) => self.ingest_request_started(store, event),
            BrowserEvent::ResponseReceived(event) => self.ingest_response_received(store, event),
            BrowserEvent::RequestCompleted(event) => self.ingest_request_completed(store, event),
            BrowserEvent::StorageChanged(event) => self.ingest_storage_changed(store, event),
            BrowserEvent::PageRouteChanged(event) => self.ingest_page_route_changed(store, event),
            BrowserEvent::PageError(event) => self.ingest_page_error(store, event),
        }
    }

    fn ingest_console(
        &mut self,
        store: &Store,
        event: ConsoleLogged,
    ) -> Result<Vec<EventEnvelope>> {
        let log = ConsoleLog::new(
            event.session_id,
            event.tab_id,
            event.run_id,
            event.level,
            event.message,
            event.source,
            event.line,
        );
        let envelope = console_event(&log);
        store.insert_console_log(&log)?;
        store.append_event(&envelope)?;
        Ok(vec![envelope])
    }

    fn ingest_request_started(
        &mut self,
        store: &Store,
        event: RequestStarted,
    ) -> Result<Vec<EventEnvelope>> {
        let mut request = RequestRecord::started(
            event.session_id,
            event.tab_id,
            event.run_id,
            event.method,
            event.url,
        );
        request.browser_request_id = Some(event.browser_request_id.clone());
        request.resource_type = event.resource_type;
        request.initiator = event.initiator;
        request.request_headers = event.headers;
        if let Some(body_text) = event.body {
            let content_type = request
                .request_headers
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case("content-type"))
                .map(|header| header.value.clone());
            let body = inline_text_body(content_type, body_text);
            request.request_body_ref = Some(body.id.clone());
            store.insert_body(&body)?;
        }

        let envelope = request_started_event(&request);
        store.insert_request(&request)?;
        store.append_event(&envelope)?;
        self.request_ids.insert(event.browser_request_id, request);

        Ok(vec![envelope])
    }

    fn ingest_response_received(
        &mut self,
        store: &Store,
        event: ResponseReceived,
    ) -> Result<Vec<EventEnvelope>> {
        let request = self
            .request_ids
            .get(&event.browser_request_id)
            .ok_or_else(|| AdapterError::UnknownBrowserRequest(event.browser_request_id.clone()))?;

        let mut response = ResponseRecord::received(request.id.clone());
        response.status_code = event.status_code;
        response.status_text = event.status_text;
        response.mime_type = event.mime_type;
        response.response_headers = event.headers;
        response.body_size = event.body_size;
        response.body_truncated = event.body_truncated;

        let envelope =
            response_received_event(event.session_id, event.tab_id, event.run_id, &response);
        store.insert_response(&response)?;
        store.append_event(&envelope)?;

        Ok(vec![envelope])
    }

    fn ingest_request_completed(
        &mut self,
        store: &Store,
        event: RequestCompleted,
    ) -> Result<Vec<EventEnvelope>> {
        let mut request = self
            .request_ids
            .remove(&event.browser_request_id)
            .ok_or_else(|| AdapterError::UnknownBrowserRequest(event.browser_request_id.clone()))?;
        request.completed_at = Some(now_ms());
        request.status = event.status;

        let envelope = request_completed_event(&request);
        store.complete_request(&request)?;
        store.append_event(&envelope)?;

        Ok(vec![envelope])
    }

    fn ingest_storage_changed(
        &mut self,
        store: &Store,
        event: StorageChanged,
    ) -> Result<Vec<EventEnvelope>> {
        let storage_event = StorageEventRecord::new(
            event.session_id,
            event.tab_id,
            event.run_id,
            event.origin,
            event.storage_type,
            event.operation,
            event.key,
            event.old_value,
            event.new_value,
        );
        let envelope = storage_changed_event(&storage_event);
        store.insert_storage_event(&storage_event)?;
        store.append_event(&envelope)?;
        Ok(vec![envelope])
    }

    fn ingest_page_route_changed(
        &mut self,
        store: &Store,
        event: PageRouteChanged,
    ) -> Result<Vec<EventEnvelope>> {
        let envelope = page_route_changed_event(
            event.session_id,
            event.tab_id,
            event.run_id,
            event.url,
            event.operation,
        );
        store.append_event(&envelope)?;
        Ok(vec![envelope])
    }

    fn ingest_page_error(&mut self, store: &Store, event: PageError) -> Result<Vec<EventEnvelope>> {
        let envelope = page_error_event(
            event.session_id,
            event.tab_id,
            event.run_id,
            event.message,
            event.source,
            event.line,
            event.kind,
        );
        store.append_event(&envelope)?;
        Ok(vec![envelope])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::{Run, RunTrigger, Session, Tab};

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn ingests_console_and_network_observations() -> TestResult {
        let store = Store::open_memory()?;
        let session = Session::new(None, Some("http://localhost:3000".to_string()));
        let tab = Tab::new(session.id.clone(), session.root_url.clone());
        let run = Run::new(
            session.id.clone(),
            tab.id.clone(),
            "http://localhost:3000".to_string(),
            RunTrigger::InitialLoad,
        );
        store.insert_session(&session)?;
        store.insert_tab(&tab)?;
        store.insert_run(&run)?;

        let mut ingestor = EventIngestor::new();
        ingestor.ingest(
            &store,
            BrowserEvent::ConsoleLogged(ConsoleLogged {
                session_id: session.id.clone(),
                tab_id: Some(tab.id.clone()),
                run_id: Some(run.id.clone()),
                level: ConsoleLevel::Info,
                message: "ready".to_string(),
                source: Some("http://localhost:3000/main.js".to_string()),
                line: Some(7),
            }),
        )?;
        ingestor.ingest(
            &store,
            BrowserEvent::RequestStarted(RequestStarted {
                session_id: session.id.clone(),
                tab_id: Some(tab.id.clone()),
                run_id: Some(run.id.clone()),
                browser_request_id: "cef-1".to_string(),
                method: "POST".to_string(),
                url: "http://localhost:3000/api/todos".to_string(),
                resource_type: Some("fetch".to_string()),
                initiator: None,
                headers: vec![
                    Header::new("accept", "application/json"),
                    Header::new("content-type", "application/json"),
                ],
                body: Some(r#"{"title":"ship"}"#.to_string()),
            }),
        )?;
        ingestor.ingest(
            &store,
            BrowserEvent::ResponseReceived(ResponseReceived {
                session_id: session.id.clone(),
                tab_id: Some(tab.id.clone()),
                run_id: Some(run.id.clone()),
                browser_request_id: "cef-1".to_string(),
                status_code: Some(200),
                status_text: Some("OK".to_string()),
                mime_type: Some("application/json".to_string()),
                headers: vec![Header::new("content-type", "application/json")],
                body_size: Some(2),
                body_truncated: false,
            }),
        )?;
        ingestor.ingest(
            &store,
            BrowserEvent::RequestCompleted(RequestCompleted {
                browser_request_id: "cef-1".to_string(),
                status: RequestStatus::Complete,
            }),
        )?;

        assert_eq!(store.event_count()?, 4);
        assert_eq!(store.console_logs_for_session(&session.id)?.len(), 1);
        let requests = store.requests_for_session(&session.id)?;
        assert_eq!(requests.len(), 1);
        let Some(body_id) = requests[0].request_body_ref.as_deref() else {
            panic!("missing request body ref");
        };
        let Some(body) = store.response_body(body_id)? else {
            panic!("missing request body");
        };
        let text = body.as_text()?;
        assert_eq!(text, r#"{"title":"ship"}"#);
        Ok(())
    }
}
