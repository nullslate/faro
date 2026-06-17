use base64::Engine;
use devbench_capture::{
    AdapterError, BrowserEvent, ConsoleLogged, EventIngestor, RequestCompleted, RequestStarted,
    ResponseReceived, StorageChanged,
};
use devbench_core::{
    ConsoleLevel, CookieEventRecord, CookieRecord, CookieSnapshotRecord, Header, RequestStatus,
    Run, RunTrigger, Session, StorageEntry, StorageSnapshotRecord, Tab, WebSocketFrameDirection,
    WebSocketFrameRecord, cookie_event_observed_event, cookie_observed_event,
    storage_snapshot_created_event, websocket_frame_event,
};
use devbench_store::{Store, inline_text_body};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const MAX_BODY_BYTES: usize = 512 * 1024;
const DEFAULT_LAUNCH_PORT: u16 = 9223;

#[derive(Debug, Clone)]
pub struct BrowserLaunchOptions {
    pub url: String,
    pub browser_binary: Option<PathBuf>,
    pub user_data_dir: Option<PathBuf>,
    pub remote_debugging_port: Option<u16>,
}

impl BrowserLaunchOptions {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            browser_binary: None,
            user_data_dir: None,
            remote_debugging_port: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CdpTarget {
    pub id: String,
    pub url: String,
    pub websocket_url: String,
}

#[derive(Debug, Clone)]
pub enum CaptureUpdate {
    SessionStarted { session_id: String, url: String },
    Attached { url: String, websocket_url: String },
    Status(String),
    StoreChanged,
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CdpError {
    #[error("could not find Chromium/Chrome/Brave; set DEVBENCH_BROWSER=/path/to/browser")]
    BrowserNotFound,
    #[error("browser launch failed: {0}")]
    BrowserLaunch(std::io::Error),
    #[error("DevToolsActivePort was not created in {0}")]
    DevToolsPortMissing(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("websocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("store error: {0}")]
    Store(#[from] devbench_store::StoreError),
    #[error("capture error: {0}")]
    Capture(#[from] devbench_capture::AdapterError),
}

pub type Result<T> = std::result::Result<T, CdpError>;

impl From<tokio_tungstenite::tungstenite::Error> for CdpError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(error))
    }
}

pub struct BrowserController {
    child: Option<Child>,
    profile_dir: Option<PathBuf>,
}

impl Drop for BrowserController {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(profile_dir) = &self.profile_dir {
            let _ = fs::remove_dir_all(profile_dir);
        }
    }
}

impl BrowserController {
    pub async fn launch_and_attach(options: BrowserLaunchOptions) -> Result<(Self, CdpTarget)> {
        let browser = options
            .browser_binary
            .clone()
            .or_else(find_browser_binary)
            .ok_or(CdpError::BrowserNotFound)?;
        let profile_dir = options
            .user_data_dir
            .clone()
            .unwrap_or_else(default_profile_dir);
        fs::create_dir_all(&profile_dir).map_err(|error| {
            CdpError::Http(format!(
                "create browser profile directory {}: {error}",
                profile_dir.display()
            ))
        })?;

        let debugging_port = options
            .remote_debugging_port
            .map(Ok)
            .unwrap_or_else(free_local_port)?;
        let port_arg = format!("--remote-debugging-port={debugging_port}");

        let child = Command::new(&browser)
            .arg(port_arg)
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("--class=devbench-browser")
            .arg("--name=devbench-browser")
            .arg("--new-window")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--remote-allow-origins=*")
            .arg(&options.url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                CdpError::Http(format!(
                    "launch browser {} for {}: {error}",
                    browser.display(),
                    options.url
                ))
            })?;

        wait_for_devtools_http(debugging_port)?;
        let target = select_page_target(debugging_port, &options.url)?;
        Ok((
            Self {
                child: Some(child),
                profile_dir: None,
            },
            target,
        ))
    }

    pub async fn attach_existing(port: u16, target_url: &str) -> Result<(Self, CdpTarget)> {
        wait_for_devtools_http(port)?;
        let target = select_page_target(port, target_url)?;
        Ok((
            Self {
                child: None,
                profile_dir: None,
            },
            target,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub db_path: PathBuf,
    pub url: String,
    pub attach_port: Option<u16>,
    pub launch_port: Option<u16>,
}

impl CaptureOptions {
    pub fn launch(db_path: PathBuf, url: String) -> Self {
        Self {
            db_path,
            url,
            attach_port: None,
            launch_port: None,
        }
    }
}

pub fn spawn_network_capture(db_path: PathBuf, url: String) -> mpsc::Receiver<CaptureUpdate> {
    spawn_capture(CaptureOptions::launch(db_path, url))
}

pub fn evaluate_expression_blocking(websocket_url: &str, expression: &str) -> Result<String> {
    let websocket_url = websocket_url.to_string();
    let expression = expression.to_string();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| CdpError::Http(format!("failed to start async runtime: {error}")))?;
        runtime.block_on(evaluate_expression(&websocket_url, &expression))
    })
    .join()
    .map_err(|_| CdpError::Http("console eval worker thread panicked".to_string()))?
}

pub fn reload_page_blocking(websocket_url: &str) -> Result<()> {
    let websocket_url = websocket_url.to_string();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| CdpError::Http(format!("failed to start async runtime: {error}")))?;
        runtime.block_on(reload_page(&websocket_url))
    })
    .join()
    .map_err(|_| CdpError::Http("page reload worker thread panicked".to_string()))?
}

pub fn set_storage_item_blocking(
    websocket_url: &str,
    origin: &str,
    storage_type: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    let websocket_url = websocket_url.to_string();
    let origin = origin.to_string();
    let storage_type = storage_type.to_string();
    let key = key.to_string();
    let value = value.to_string();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| CdpError::Http(format!("failed to start async runtime: {error}")))?;
        runtime.block_on(set_storage_item(
            &websocket_url,
            &origin,
            &storage_type,
            &key,
            &value,
        ))
    })
    .join()
    .map_err(|_| CdpError::Http("storage edit worker thread panicked".to_string()))?
}

pub fn set_cookie_value_blocking(
    websocket_url: &str,
    cookie: &CookieRecord,
    value: &str,
) -> Result<()> {
    let websocket_url = websocket_url.to_string();
    let cookie = cookie.clone();
    let value = value.to_string();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| CdpError::Http(format!("failed to start async runtime: {error}")))?;
        runtime.block_on(set_cookie_value(&websocket_url, &cookie, &value))
    })
    .join()
    .map_err(|_| CdpError::Http("cookie edit worker thread panicked".to_string()))?
}

pub async fn reload_page(websocket_url: &str) -> Result<()> {
    let (mut ws, _) = connect_async(websocket_url).await?;
    let mut next_id = 1_i64;
    send_command(&mut ws, &mut next_id, "Page.reload", json!({})).await?;
    Ok(())
}

pub async fn set_storage_item(
    websocket_url: &str,
    origin: &str,
    storage_type: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    let (mut ws, _) = connect_async(websocket_url).await?;
    let mut next_id = 1_i64;
    send_command(
        &mut ws,
        &mut next_id,
        "DOMStorage.setDOMStorageItem",
        json!({
            "storageId": {
                "securityOrigin": origin,
                "isLocalStorage": storage_type == "localStorage"
            },
            "key": key,
            "value": value
        }),
    )
    .await?;
    Ok(())
}

pub async fn set_cookie_value(
    websocket_url: &str,
    cookie: &CookieRecord,
    value: &str,
) -> Result<()> {
    let (mut ws, _) = connect_async(websocket_url).await?;
    let mut next_id = 1_i64;
    let mut params = json!({
        "name": cookie.name,
        "value": value,
        "domain": cookie.domain,
        "path": cookie.path,
        "secure": cookie.secure,
        "httpOnly": cookie.http_only
    });
    if let Some(expires) = cookie.expires {
        params["expires"] = json!(expires);
    }
    if let Some(same_site) = &cookie.same_site {
        params["sameSite"] = json!(same_site);
    }
    send_command(&mut ws, &mut next_id, "Network.setCookie", params).await?;
    Ok(())
}

pub async fn evaluate_expression(websocket_url: &str, expression: &str) -> Result<String> {
    let (mut ws, _) = connect_async(websocket_url).await?;
    let mut next_id = 1_i64;
    let command_id = send_command(
        &mut ws,
        &mut next_id,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "awaitPromise": true,
            "returnByValue": true,
            "generatePreview": true,
            "replMode": true
        }),
    )
    .await?;

    while let Some(message) = ws.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)?;
        if value.get("id").and_then(Value::as_i64) == Some(command_id) {
            return Ok(format_runtime_evaluation(&value));
        }
    }

    Err(CdpError::Http("evaluation socket closed".to_string()))
}

pub fn spawn_capture(options: CaptureOptions) -> mpsc::Receiver<CaptureUpdate> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = tx.send(CaptureUpdate::Error(format!(
                    "failed to start async runtime: {error}"
                )));
                return;
            }
        };

        runtime.block_on(async move {
            if let Err(error) = capture_url(options, tx.clone()).await {
                let _ = tx.send(CaptureUpdate::Error(error.to_string()));
            }
        });
    });
    rx
}

pub async fn capture_url(
    options: CaptureOptions,
    updates: mpsc::Sender<CaptureUpdate>,
) -> Result<()> {
    let db_path = options.db_path;
    let url = options.url;
    let store = Store::open(&db_path)?;
    let session = Session::new(Some("CDP session".to_string()), Some(url.clone()));
    let tab = Tab::new(session.id.clone(), Some(url.clone()));
    let run = Run::new(
        session.id.clone(),
        tab.id.clone(),
        url.clone(),
        RunTrigger::InitialLoad,
    );
    store.insert_session(&session)?;
    store.insert_tab(&tab)?;
    store.insert_run(&run)?;
    let _ = updates.send(CaptureUpdate::SessionStarted {
        session_id: session.id.clone(),
        url: url.clone(),
    });
    let _ = updates.send(CaptureUpdate::StoreChanged);
    let _ = updates.send(CaptureUpdate::Status(if options.attach_port.is_some() {
        "attaching to browser".to_string()
    } else {
        "launching browser".to_string()
    }));

    let launch_port = options.launch_port.unwrap_or(DEFAULT_LAUNCH_PORT);
    let (_browser, target) = if let Some(port) = options.attach_port {
        BrowserController::attach_existing(port, &url).await?
    } else if devtools_http_available(launch_port)
        && let Ok(attached) = BrowserController::attach_existing(launch_port, &url).await
    {
        let _ = updates.send(CaptureUpdate::Status(format!(
            "reconnected to browser on port {launch_port}"
        )));
        attached
    } else {
        let mut launch = BrowserLaunchOptions::new(url.clone());
        launch.remote_debugging_port = Some(launch_port);
        BrowserController::launch_and_attach(launch).await?
    };
    let _ = updates.send(CaptureUpdate::Attached {
        url: target.url.clone(),
        websocket_url: target.websocket_url.clone(),
    });
    let _ = updates.send(CaptureUpdate::Status(format!("attached {}", target.url)));

    let (mut ws, _) = connect_async(&target.websocket_url).await?;
    let mut next_id = 1_i64;
    send_command(&mut ws, &mut next_id, "Page.enable", json!({})).await?;
    send_command(&mut ws, &mut next_id, "Runtime.enable", json!({})).await?;
    send_command(&mut ws, &mut next_id, "DOMStorage.enable", json!({})).await?;
    send_command(
        &mut ws,
        &mut next_id,
        "Runtime.addBinding",
        json!({ "name": "devbenchCookieMutation" }),
    )
    .await?;
    send_command(
        &mut ws,
        &mut next_id,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": cookie_agent_script() }),
    )
    .await?;
    send_command(
        &mut ws,
        &mut next_id,
        "Network.enable",
        json!({
            "maxTotalBufferSize": 8 * 1024 * 1024,
            "maxResourceBufferSize": 1024 * 1024
        }),
    )
    .await?;
    if target.url != url {
        send_command(
            &mut ws,
            &mut next_id,
            "Page.navigate",
            json!({ "url": url }),
        )
        .await?;
    } else {
        send_command(&mut ws, &mut next_id, "Page.reload", json!({})).await?;
    }
    let _ = updates.send(CaptureUpdate::Status(
        "attached; reloading page for capture".to_string(),
    ));

    let mut ingestor = EventIngestor::new();
    let mut pending_commands = HashMap::<i64, PendingCommand>::new();
    let mut pending_completions = HashMap::<String, RequestStatus>::new();
    let mut response_mime_types = HashMap::<String, Option<String>>::new();
    let mut snapshots_requested = false;

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
        let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
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
                    let command_id = next_id;
                    send_command(
                        &mut ws,
                        &mut next_id,
                        "Network.getResponseBody",
                        json!({ "requestId": request_id }),
                    )
                    .await?;
                    pending_commands.insert(
                        command_id,
                        PendingCommand::Body(PendingBody {
                            request_id: request_id.to_string(),
                            mime_type: response_mime_types.get(request_id).cloned().flatten(),
                        }),
                    );
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

fn ingest_or_ignore_unknown(
    ingestor: &mut EventIngestor,
    store: &Store,
    event: BrowserEvent,
) -> Result<bool> {
    match ingestor.ingest(store, event) {
        Ok(_) => Ok(true),
        Err(AdapterError::UnknownBrowserRequest(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

async fn send_command(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: &mut i64,
    method: &str,
    params: Value,
) -> Result<i64> {
    let id = *next_id;
    *next_id += 1;
    ws.send(Message::Text(
        json!({ "id": id, "method": method, "params": params })
            .to_string()
            .into(),
    ))
    .await?;
    Ok(id)
}

fn format_runtime_evaluation(value: &Value) -> String {
    if let Some(details) = value
        .get("result")
        .and_then(|result| result.get("exceptionDetails"))
    {
        let text = details
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("exception");
        let description = details
            .get("exception")
            .and_then(|exception| exception.get("description"))
            .and_then(Value::as_str)
            .or_else(|| {
                details
                    .get("exception")
                    .and_then(|exception| exception.get("value"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("");
        return if description.is_empty() {
            format!("Error: {text}")
        } else {
            format!("Error: {text}\n{description}")
        };
    }

    let Some(result) = value.get("result").and_then(|result| result.get("result")) else {
        return value.to_string();
    };

    if let Some(value) = result.get("value") {
        if let Some(text) = value.as_str() {
            return text.to_string();
        }
        return serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    }

    result
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| result.get("type").and_then(Value::as_str))
        .unwrap_or("undefined")
        .to_string()
}

fn parse_request_started(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
) -> Option<RequestStarted> {
    let request_id = params.get("requestId")?.as_str()?.to_string();
    let request = params.get("request")?;
    Some(RequestStarted {
        session_id: session.id.clone(),
        tab_id: Some(tab.id.clone()),
        run_id: Some(run.id.clone()),
        browser_request_id: request_id,
        method: request.get("method")?.as_str()?.to_string(),
        url: request.get("url")?.as_str()?.to_string(),
        resource_type: params
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string),
        initiator: params
            .get("initiator")
            .and_then(|initiator| initiator.get("type"))
            .and_then(Value::as_str)
            .map(str::to_string),
        headers: parse_headers(request.get("headers")),
        body: request
            .get("postData")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_response_received(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
) -> Option<ResponseReceived> {
    let request_id = params.get("requestId")?.as_str()?.to_string();
    let response = params.get("response")?;
    Some(ResponseReceived {
        session_id: session.id.clone(),
        tab_id: Some(tab.id.clone()),
        run_id: Some(run.id.clone()),
        browser_request_id: request_id,
        status_code: response.get("status").and_then(Value::as_i64),
        status_text: response
            .get("statusText")
            .and_then(Value::as_str)
            .map(str::to_string),
        mime_type: response
            .get("mimeType")
            .and_then(Value::as_str)
            .map(str::to_string),
        headers: parse_headers(response.get("headers")),
        body_size: None,
        body_truncated: false,
    })
}

fn parse_websocket_frame(
    session: &Session,
    tab: &Tab,
    run: &Run,
    method: &str,
    params: &Value,
) -> Option<WebSocketFrameRecord> {
    let request_id = params.get("requestId")?.as_str()?.to_string();
    let response = params.get("response")?;
    let direction = if method == "Network.webSocketFrameSent" {
        WebSocketFrameDirection::Sent
    } else {
        WebSocketFrameDirection::Received
    };
    Some(WebSocketFrameRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        request_id,
        direction,
        response.get("opcode").and_then(Value::as_i64).unwrap_or(1),
        response
            .get("mask")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        response
            .get("payloadData")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    ))
}

fn parse_console_api_called(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
) -> Option<ConsoleLogged> {
    let kind = params.get("type").and_then(Value::as_str).unwrap_or("log");
    let args = params
        .get("args")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .map(remote_object_preview)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| kind.to_string());
    let (source, line) = stack_location(params.get("stackTrace"));

    Some(ConsoleLogged {
        session_id: session.id.clone(),
        tab_id: Some(tab.id.clone()),
        run_id: Some(run.id.clone()),
        level: console_level(kind),
        message: args,
        source,
        line,
    })
}

fn parse_exception_thrown(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
) -> Option<ConsoleLogged> {
    let details = params.get("exceptionDetails")?;
    let exception = details.get("exception");
    let message = exception
        .and_then(|exception| exception.get("description"))
        .and_then(Value::as_str)
        .or_else(|| details.get("text").and_then(Value::as_str))
        .unwrap_or("exception")
        .to_string();
    let source = details
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    let line = details.get("lineNumber").and_then(Value::as_i64);

    Some(ConsoleLogged {
        session_id: session.id.clone(),
        tab_id: Some(tab.id.clone()),
        run_id: Some(run.id.clone()),
        level: ConsoleLevel::Error,
        message,
        source,
        line,
    })
}

fn parse_dom_storage_event(
    session: &Session,
    tab: &Tab,
    run: &Run,
    method: &str,
    params: &Value,
) -> Option<StorageChanged> {
    let storage_id = params.get("storageId")?;
    let origin = storage_id
        .get("securityOrigin")
        .and_then(Value::as_str)
        .or_else(|| storage_id.get("storageKey").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let storage_type = if storage_id
        .get("isLocalStorage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "localStorage"
    } else {
        "sessionStorage"
    }
    .to_string();

    let operation = match method {
        "DOMStorage.domStorageItemAdded" => "set",
        "DOMStorage.domStorageItemUpdated" => "update",
        "DOMStorage.domStorageItemRemoved" => "remove",
        "DOMStorage.domStorageItemsCleared" => "clear",
        _ => return None,
    }
    .to_string();

    Some(StorageChanged {
        session_id: session.id.clone(),
        tab_id: Some(tab.id.clone()),
        run_id: Some(run.id.clone()),
        origin,
        storage_type,
        operation,
        key: params
            .get("key")
            .and_then(Value::as_str)
            .map(str::to_string),
        old_value: params
            .get("oldValue")
            .and_then(Value::as_str)
            .map(str::to_string),
        new_value: params
            .get("newValue")
            .or_else(|| params.get("value"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_cookie_binding_called(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
) -> Option<CookieEventRecord> {
    if params.get("name").and_then(Value::as_str)? != "devbenchCookieMutation" {
        return None;
    }
    let payload = params.get("payload").and_then(Value::as_str)?;
    let payload = match serde_json::from_str::<Value>(payload) {
        Ok(payload) => payload,
        Err(_) => return None,
    };
    let raw_cookie = payload.get("cookie").and_then(Value::as_str).unwrap_or("");
    let parsed = parse_cookie_assignment(raw_cookie);

    Some(CookieEventRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        "document.cookie",
        parsed.name,
        payload
            .get("host")
            .and_then(Value::as_str)
            .map(str::to_string),
        parsed.path,
        parsed.value,
        Some(payload),
    ))
}

fn parse_set_cookie_events(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
    operation: &str,
) -> Vec<CookieEventRecord> {
    let Some(headers) = params.get("headers").and_then(Value::as_object) else {
        return Vec::new();
    };

    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .flat_map(|(_, value)| {
            value
                .as_str()
                .unwrap_or("")
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parsed = parse_cookie_assignment(&line);
            CookieEventRecord::new(
                session.id.clone(),
                Some(tab.id.clone()),
                Some(run.id.clone()),
                operation,
                parsed.name,
                parsed.domain,
                parsed.path,
                parsed.value,
                Some(json!({ "raw": line })),
            )
        })
        .collect()
}

fn parse_cookie_assignment(raw_cookie: &str) -> ParsedCookieAssignment {
    let mut parts = raw_cookie.split(';').map(str::trim);
    let (name, value) = parts
        .next()
        .and_then(|pair| pair.split_once('='))
        .map(|(name, value)| (Some(name.to_string()), Some(value.to_string())))
        .unwrap_or((None, None));

    let mut domain = None;
    let mut path = None;
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key.eq_ignore_ascii_case("domain") {
            domain = Some(value.to_string());
        } else if key.eq_ignore_ascii_case("path") {
            path = Some(value.to_string());
        }
    }

    ParsedCookieAssignment {
        name,
        value,
        domain,
        path,
    }
}

fn persist_cookie_event(store: &Store, event: CookieEventRecord) -> Result<()> {
    let envelope = cookie_event_observed_event(&event);
    store.insert_cookie_event(&event)?;
    store.append_event(&envelope)?;
    Ok(())
}

fn cookie_agent_script() -> &'static str {
    r#"
(() => {
  if (window.__devbenchCookieAgentInstalled) return;
  window.__devbenchCookieAgentInstalled = true;
  const descriptor =
    Object.getOwnPropertyDescriptor(Document.prototype, "cookie") ||
    Object.getOwnPropertyDescriptor(HTMLDocument.prototype, "cookie");
  if (!descriptor || !descriptor.configurable || !descriptor.get || !descriptor.set) return;
  Object.defineProperty(Document.prototype, "cookie", {
    configurable: true,
    enumerable: descriptor.enumerable,
    get() {
      return descriptor.get.call(this);
    },
    set(value) {
      try {
        window.devbenchCookieMutation(JSON.stringify({
          cookie: String(value),
          href: location.href,
          origin: location.origin,
          host: location.hostname,
          ts: Date.now()
        }));
      } catch (_) {}
      return descriptor.set.call(this, value);
    }
  });
})();
"#
}

fn remote_object_preview(value: &Value) -> String {
    value
        .get("value")
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string())
        })
        .or_else(|| {
            value
                .get("unserializableValue")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn stack_location(stack_trace: Option<&Value>) -> (Option<String>, Option<i64>) {
    let Some(call_frame) = stack_trace
        .and_then(|stack| stack.get("callFrames"))
        .and_then(Value::as_array)
        .and_then(|frames| frames.first())
    else {
        return (None, None);
    };

    (
        call_frame
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(str::to_string),
        call_frame.get("lineNumber").and_then(Value::as_i64),
    )
}

fn console_level(kind: &str) -> ConsoleLevel {
    match kind {
        "debug" => ConsoleLevel::Debug,
        "warning" => ConsoleLevel::Warning,
        "error" | "assert" => ConsoleLevel::Error,
        "trace" => ConsoleLevel::Trace,
        _ => ConsoleLevel::Info,
    }
}

fn parse_headers(value: Option<&Value>) -> Vec<Header> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    object
        .iter()
        .map(|(name, value)| {
            Header::new(
                name,
                value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string()),
            )
        })
        .collect()
}

fn persist_body_response(store: &Store, value: &Value, pending: &PendingBody) -> Result<()> {
    let Some(result) = value.get("result") else {
        return Ok(());
    };
    let body_text = result
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_base64 = result
        .get("base64Encoded")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if body_text.len() > MAX_BODY_BYTES {
        return Ok(());
    }

    let body = if is_base64 {
        let Some(mime_type) = pending
            .mime_type
            .as_deref()
            .filter(|mime| is_image_mime(mime))
        else {
            return Ok(());
        };
        let decoded_size = base64::engine::general_purpose::STANDARD
            .decode(body_text.as_bytes())
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        if decoded_size > MAX_BODY_BYTES {
            return Ok(());
        }
        inline_text_body(
            Some(mime_type.to_string()),
            format!("data:{mime_type};base64,{body_text}"),
        )
    } else {
        inline_text_body(None, body_text)
    };
    let _ = store.attach_body_to_response_by_browser_request_id(
        &pending.request_id,
        &body,
        body.size as usize >= MAX_BODY_BYTES,
    )?;
    Ok(())
}

fn is_image_mime(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
}

async fn request_state_snapshots(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: &mut i64,
    pending_commands: &mut HashMap<i64, PendingCommand>,
    target_url: &str,
) -> Result<()> {
    for storage_type in ["localStorage", "sessionStorage"] {
        let command_id = send_command(
            ws,
            next_id,
            "Runtime.evaluate",
            json!({
                "expression": storage_snapshot_expression(storage_type),
                "returnByValue": true
            }),
        )
        .await?;
        pending_commands.insert(
            command_id,
            PendingCommand::Storage(PendingStorage {
                storage_type: storage_type.to_string(),
            }),
        );
    }

    let command_id = send_command(
        ws,
        next_id,
        "Network.getCookies",
        json!({ "urls": [target_url] }),
    )
    .await?;
    pending_commands.insert(command_id, PendingCommand::Cookies);

    Ok(())
}

fn storage_snapshot_expression(storage_type: &str) -> String {
    format!(
        r#"(() => {{
            const storage = window.{storage_type};
            const entries = [];
            for (let index = 0; index < storage.length; index++) {{
                const key = storage.key(index);
                entries.push({{ key, value: storage.getItem(key) }});
            }}
            return {{ origin: location.origin, entries }};
        }})()"#
    )
}

fn persist_storage_snapshot(
    store: &Store,
    session: &Session,
    tab: &Tab,
    run: &Run,
    value: &Value,
    pending: PendingStorage,
) -> Result<bool> {
    let Some(result_value) = value
        .get("result")
        .and_then(|result| result.get("result"))
        .and_then(|result| result.get("value"))
    else {
        return Ok(false);
    };

    let origin = result_value
        .get("origin")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let entries = result_value
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(StorageEntry::new(
                        entry.get("key")?.as_str()?.to_string(),
                        entry
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let sha256 = sha256_json(&entries)?;
    let snapshot = StorageSnapshotRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        origin,
        pending.storage_type,
        entries,
        sha256,
    );
    let event = storage_snapshot_created_event(&snapshot);
    store.insert_storage_snapshot(&snapshot)?;
    store.append_event(&event)?;
    Ok(true)
}

fn persist_cookie_snapshot(
    store: &Store,
    session: &Session,
    tab: &Tab,
    run: &Run,
    target_url: &str,
    value: &Value,
) -> Result<bool> {
    let cookies = value
        .get("result")
        .and_then(|result| result.get("cookies"))
        .and_then(Value::as_array)
        .map(|cookies| cookies.iter().map(parse_cookie).collect::<Vec<_>>())
        .unwrap_or_default();

    let snapshot = CookieSnapshotRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        Some(target_url.to_string()),
        cookies,
    );
    let event = cookie_observed_event(&snapshot);
    store.insert_cookie_snapshot(&snapshot)?;
    store.append_event(&event)?;
    Ok(true)
}

fn parse_cookie(value: &Value) -> CookieRecord {
    CookieRecord {
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value: value
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        domain: value
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        path: value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        expires: value.get("expires").and_then(Value::as_f64),
        http_only: value
            .get("httpOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        secure: value
            .get("secure")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        same_site: value
            .get("sameSite")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn sha256_json<T: serde::Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_vec(value)?;
    let digest = Sha256::digest(&json);
    Ok(digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}

#[derive(Debug, Clone)]
enum PendingCommand {
    Body(PendingBody),
    Storage(PendingStorage),
    Cookies,
}

#[derive(Debug, Clone)]
struct ParsedCookieAssignment {
    name: Option<String>,
    value: Option<String>,
    domain: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingBody {
    request_id: String,
    mime_type: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingStorage {
    storage_type: String,
}

fn find_browser_binary() -> Option<PathBuf> {
    env::var_os("DEVBENCH_BROWSER")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| {
            [
                "google-chrome-stable",
                "google-chrome",
                "chromium",
                "chromium-browser",
                "brave-browser",
                "brave",
            ]
            .into_iter()
            .find_map(find_on_path)
        })
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(binary))
        .find(|path| path.exists())
}

fn default_profile_dir() -> PathBuf {
    if let Ok(config_home) = env::var("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return PathBuf::from(config_home).join("devbench/browser-profile");
    }
    if let Ok(home) = env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".config/devbench/browser-profile");
    }
    env::temp_dir().join("devbench-browser-profile")
}

fn wait_for_devtools_http(port: u16) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(8) {
        if http_get_localhost(port, "/json/version").is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(CdpError::DevToolsPortMissing(format!("127.0.0.1:{port}")))
}

fn devtools_http_available(port: u16) -> bool {
    http_get_localhost(port, "/json/version").is_ok()
}

fn free_local_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| CdpError::Http(format!("bind ephemeral local CDP port: {error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| CdpError::Http(format!("read ephemeral local CDP port: {error}")))?
        .port();
    Ok(port)
}

fn select_page_target(port: u16, target_url: &str) -> Result<CdpTarget> {
    let body = http_get_localhost(port, "/json/list")?;
    let targets = serde_json::from_str::<Vec<JsonTarget>>(&body)
        .map_err(|error| CdpError::Http(format!("parse /json/list from port {port}: {error}")))?;
    let selected = targets
        .iter()
        .find(|target| target.kind == "page" && target.url == target_url)
        .or_else(|| targets.iter().find(|target| target.kind == "page"))
        .ok_or_else(|| CdpError::Http("no page CDP target found".to_string()))?;

    Ok(CdpTarget {
        id: selected.id.clone(),
        url: selected.url.clone(),
        websocket_url: selected.websocket_url.clone(),
    })
}

fn http_get_localhost(port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| {
        CdpError::Http(format!(
            "connect to DevTools http 127.0.0.1:{port}{path}: {error}"
        ))
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| CdpError::Http(format!("set DevTools read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| CdpError::Http(format!("set DevTools write timeout: {error}")))?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| CdpError::Http(format!("write DevTools request {path}: {error}")))?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut content_length = None;
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| CdpError::Http(format!("read DevTools response {path}: {error}")))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if content_length.is_none() {
            content_length = parse_content_length(&bytes);
        }
        if let Some((header_end, length)) = content_length
            && bytes.len() >= header_end + length
        {
            break;
        }
    }
    let response = String::from_utf8_lossy(&bytes);
    response
        .split("\r\n\r\n")
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| CdpError::Http("invalid HTTP response".to_string()))
}

fn parse_content_length(bytes: &[u8]) -> Option<(usize, usize)> {
    let haystack = match std::str::from_utf8(bytes) {
        Ok(haystack) => haystack,
        Err(_) => return None,
    };
    let header_end = haystack.find("\r\n\r\n")? + 4;
    for line in haystack[..header_end].lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        match value.trim().parse::<usize>() {
            Ok(length) => return Some((header_end, length)),
            Err(_) => return None,
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct JsonTarget {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_url: String,
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
