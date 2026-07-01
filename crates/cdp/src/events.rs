use faro_capture::{ConsoleLogged, RequestStarted, ResponseReceived, StorageChanged};
use faro_core::{
    ConsoleLevel, CookieEventRecord, Header, Run, Session, Tab, WebSocketFrameDirection,
    WebSocketFrameRecord,
};
use serde_json::{Value, json};

pub(crate) fn parse_request_started(
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

pub(crate) fn parse_response_received(
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

pub(crate) fn parse_websocket_frame(
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

pub(crate) fn parse_console_api_called(
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

pub(crate) fn parse_exception_thrown(
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

pub(crate) fn parse_dom_storage_event(
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

pub(crate) fn parse_cookie_binding_called(
    session: &Session,
    tab: &Tab,
    run: &Run,
    params: &Value,
) -> Option<CookieEventRecord> {
    if params.get("name").and_then(Value::as_str)? != "faroCookieMutation" {
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

pub(crate) fn parse_set_cookie_events(
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

#[derive(Debug, Clone)]
struct ParsedCookieAssignment {
    name: Option<String>,
    value: Option<String>,
    domain: Option<String>,
    path: Option<String>,
}
