#[cfg(test)]
use crate::CdpError;
use crate::Result;
use crate::events::{
    parse_console_api_called, parse_cookie_binding_called, parse_dom_storage_event,
    parse_exception_thrown, parse_request_started, parse_response_received,
    parse_set_cookie_events, parse_websocket_frame,
};
mod ingest;
mod persistence;
mod setup;
mod spawn;
mod wire;

pub use spawn::{spawn_capture, spawn_network_capture};

use faro_capture::{BrowserEvent, EventIngestor, RequestCompleted};
use faro_core::{RequestStatus, websocket_frame_event};
#[cfg(test)]
use faro_core::{Run, RunTrigger, Session, Tab};
use futures_util::StreamExt;
use ingest::ingest_or_ignore_unknown;
use persistence::{
    persist_body_response, persist_cookie_event, persist_cookie_snapshot, persist_storage_snapshot,
};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use setup::{CaptureContext, initialize_capture};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::Message;
use wire::{
    PendingCommand, connect_and_enable_capture, request_response_body, request_state_snapshots,
};

#[derive(Debug, Clone)]
pub enum CaptureUpdate {
    SessionStarted { session_id: String, url: String },
    Attached { url: String, websocket_url: String },
    Status(String),
    StoreChanged,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub db_path: PathBuf,
    pub url: String,
    pub attach_port: Option<u16>,
    pub launch_port: Option<u16>,
    pub max_requests_per_session: usize,
    pub max_repeated_requests_per_url: usize,
    pub prune_interval_requests: usize,
}

impl CaptureOptions {
    pub fn launch(db_path: PathBuf, url: String) -> Self {
        Self {
            db_path,
            url,
            attach_port: None,
            launch_port: None,
            max_requests_per_session: 5_000,
            max_repeated_requests_per_url: 250,
            prune_interval_requests: 250,
        }
    }
}

pub async fn capture_url(
    options: CaptureOptions,
    updates: std::sync::mpsc::Sender<CaptureUpdate>,
) -> Result<()> {
    let max_requests_per_session = options.max_requests_per_session;
    let max_repeated_requests_per_url = options.max_repeated_requests_per_url;
    let prune_interval_requests = options.prune_interval_requests.max(1);
    let CaptureContext {
        _browser,
        target,
        store,
        session,
        tab,
        run,
        url,
    } = initialize_capture(options, &updates).await?;

    let (mut ws, mut next_id) =
        connect_and_enable_capture(&target.websocket_url, &target.url, &url).await?;
    let _ = updates.send(CaptureUpdate::Status(
        "attached; reloading page for capture".to_string(),
    ));

    let mut ingestor = EventIngestor::new();
    let mut pending_commands = HashMap::<i64, PendingCommand>::new();
    let mut pending_completions = HashMap::<String, RequestStatus>::new();
    let mut response_mime_types = HashMap::<String, Option<String>>::new();
    let mut snapshots_requested = false;
    let mut requests_since_prune = 0usize;

    while let Some(message) = ws.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)?;

        if let Some(id) = value.get("id").and_then(Value::as_i64) {
            if let Some(pending) = pending_commands.remove(&id) {
                match pending {
                    PendingCommand::Body(pending) => {
                        persist_body_response(&store, &value, &pending)?;
                        let status = pending_completions
                            .remove(&pending.request_id)
                            .unwrap_or(RequestStatus::Complete);
                        ingest_or_ignore_unknown(
                            &mut ingestor,
                            &store,
                            BrowserEvent::RequestCompleted(RequestCompleted {
                                browser_request_id: pending.request_id,
                                status,
                            }),
                        )?;
                        let _ = updates.send(CaptureUpdate::StoreChanged);
                    }
                    PendingCommand::Storage(pending) => {
                        if persist_storage_snapshot(&store, &session, &tab, &run, &value, pending)?
                        {
                            let _ = updates.send(CaptureUpdate::StoreChanged);
                        }
                    }
                    PendingCommand::Cookies => {
                        if persist_cookie_snapshot(
                            &store,
                            &session,
                            &tab,
                            &run,
                            &target.url,
                            &value,
                        )? {
                            let _ = updates.send(CaptureUpdate::StoreChanged);
                        }
                    }
                }
            }
            continue;
        }

        let Some(method) = value.get("method").and_then(Value::as_str) else {
            continue;
        };
        let params = value
            .get("params")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        match method {
            "Page.loadEventFired" if !snapshots_requested => {
                request_state_snapshots(&mut ws, &mut next_id, &mut pending_commands, &target.url)
                    .await?;
                snapshots_requested = true;
                let _ = updates.send(CaptureUpdate::Status(
                    "capturing network, console, storage, and cookies".to_string(),
                ));
            }
            "Network.requestWillBeSent" => {
                if let Some(event) = parse_request_started(&session, &tab, &run, &params) {
                    ingest_or_ignore_unknown(
                        &mut ingestor,
                        &store,
                        BrowserEvent::RequestStarted(event),
                    )?;
                    requests_since_prune = requests_since_prune.saturating_add(1);
                    if requests_since_prune >= prune_interval_requests {
                        store.prune_repeated_session_requests(
                            &session.id,
                            max_repeated_requests_per_url,
                        )?;
                        store.prune_session_requests(&session.id, max_requests_per_session)?;
                        requests_since_prune = 0;
                    }
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Network.responseReceived" => {
                if let Some(event) = parse_response_received(&session, &tab, &run, &params) {
                    response_mime_types
                        .insert(event.browser_request_id.clone(), event.mime_type.clone());
                    ingest_or_ignore_unknown(
                        &mut ingestor,
                        &store,
                        BrowserEvent::ResponseReceived(event),
                    )?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Network.responseReceivedExtraInfo" => {
                let events =
                    parse_set_cookie_events(&session, &tab, &run, &params, "set-cookie-header");
                for event in events {
                    persist_cookie_event(&store, event)?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Network.webSocketFrameSent" | "Network.webSocketFrameReceived" => {
                if let Some(frame) = parse_websocket_frame(&session, &tab, &run, method, &params) {
                    store.insert_websocket_frame(&frame)?;
                    store.append_event(&websocket_frame_event(&frame))?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Network.loadingFinished" => {
                if let Some(request_id) = params.get("requestId").and_then(Value::as_str) {
                    request_response_body(
                        &mut ws,
                        &mut next_id,
                        &mut pending_commands,
                        request_id,
                        response_mime_types.get(request_id).cloned().flatten(),
                    )
                    .await?;
                    pending_completions.insert(request_id.to_string(), RequestStatus::Complete);
                }
            }
            "Network.loadingFailed" => {
                if let Some(request_id) = params.get("requestId").and_then(Value::as_str) {
                    ingest_or_ignore_unknown(
                        &mut ingestor,
                        &store,
                        BrowserEvent::RequestCompleted(RequestCompleted {
                            browser_request_id: request_id.to_string(),
                            status: RequestStatus::Failed,
                        }),
                    )?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Runtime.consoleAPICalled" => {
                if let Some(event) = parse_console_api_called(&session, &tab, &run, &params) {
                    ingest_or_ignore_unknown(
                        &mut ingestor,
                        &store,
                        BrowserEvent::ConsoleLogged(event),
                    )?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Runtime.exceptionThrown" => {
                if let Some(event) = parse_exception_thrown(&session, &tab, &run, &params) {
                    ingest_or_ignore_unknown(
                        &mut ingestor,
                        &store,
                        BrowserEvent::ConsoleLogged(event),
                    )?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "Runtime.bindingCalled" => {
                if let Some(event) = parse_cookie_binding_called(&session, &tab, &run, &params) {
                    persist_cookie_event(&store, event)?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            "DOMStorage.domStorageItemAdded"
            | "DOMStorage.domStorageItemUpdated"
            | "DOMStorage.domStorageItemRemoved"
            | "DOMStorage.domStorageItemsCleared" => {
                if let Some(event) = parse_dom_storage_event(&session, &tab, &run, method, &params)
                {
                    ingest_or_ignore_unknown(
                        &mut ingestor,
                        &store,
                        BrowserEvent::StorageChanged(event),
                    )?;
                    let _ = updates.send(CaptureUpdate::StoreChanged);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_added_storage_value_from_value_field() -> Result<()> {
        let session = Session::new(None, Some("https://example.test".to_string()));
        let tab = Tab::new(session.id.clone(), Some("https://example.test".to_string()));
        let run = Run::new(
            session.id.clone(),
            tab.id.clone(),
            "https://example.test".to_string(),
            RunTrigger::InitialLoad,
        );
        let event = parse_dom_storage_event(
            &session,
            &tab,
            &run,
            "DOMStorage.domStorageItemAdded",
            &json!({
                "storageId": {
                    "securityOrigin": "https://example.test",
                    "isLocalStorage": true
                },
                "key": "token",
                "value": "abc123"
            }),
        )
        .ok_or_else(|| CdpError::Http("storage event was not parsed".to_string()))?;

        assert_eq!(event.storage_type, "localStorage");
        assert_eq!(event.key.as_deref(), Some("token"));
        assert_eq!(event.new_value.as_deref(), Some("abc123"));
        Ok(())
    }
}
