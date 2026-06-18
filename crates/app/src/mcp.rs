use crate::cli::{
    CliOptions, CliReplayResult, build_curl_args, build_curl_argv, build_curl_command,
    current_storage_items, find_request_with_response, latest_cookies, latest_session, load_body,
    load_body_text, open_store, parse_duration, request_matches_filter, request_matches_route,
    request_rows_for_session,
};
use anyhow::{Context, bail};
use faro_cdp::{CaptureOptions, CaptureUpdate};
use faro_core::{ReplayRecord, Session, request_replayed_event};
use faro_store::inline_text_body;
use serde_json::{Map, Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

pub(crate) fn run(options: CliOptions) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line.context("read MCP stdin line")?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value =
            serde_json::from_str(&line).with_context(|| format!("parse MCP message `{line}`"))?;
        let response = handle_message(&options, request);
        if let Some(response) = response {
            writeln!(
                stdout,
                "{}",
                serde_json::to_string(&response).context("serialize MCP response")?
            )
            .context("write MCP response")?;
            stdout.flush().context("flush MCP response")?;
        }
    }
    Ok(())
}

fn handle_message(options: &CliOptions, request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let id = id?;
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "faro",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(options, request.get("params").unwrap_or(&Value::Null)),
        _ => Err(anyhow::anyhow!("unsupported MCP method `{method}`")),
    };
    Some(match result {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": error.to_string()
            }
        }),
    })
}

fn call_tool(options: &CliOptions, params: &Value) -> anyhow::Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .context("tools/call missing name")?;
    let args = params.get("arguments").unwrap_or(&Value::Null);
    let result = match name {
        "capture_url" => capture_url_tool(options, args),
        "list_sessions" => list_sessions_tool(&options.db_path),
        "delete_all_sessions" => delete_all_sessions_tool(&options.db_path, args),
        "list_requests" => list_requests_tool(&options.db_path, args),
        "get_request" => get_request_tool(&options.db_path, args),
        "get_response_body" => get_response_body_tool(&options.db_path, args),
        "list_console_errors" => list_console_errors_tool(&options.db_path, args),
        "list_storage_items" => list_storage_items_tool(&options.db_path, args),
        "get_storage_item" => get_storage_item_tool(&options.db_path, args),
        "list_cookies" => list_cookies_tool(&options.db_path, args),
        "copy_request_as_curl" => copy_request_as_curl_tool(&options.db_path, args),
        "replay_request" => replay_request_tool(&options.db_path, args),
        "run_readonly_sql" => run_readonly_sql_tool(&options.db_path, args),
        _ => bail!("unknown tool `{name}`"),
    }?;
    Ok(tool_result(result, false))
}

fn capture_url_tool(options: &CliOptions, args: &Value) -> anyhow::Result<Value> {
    let url = required_string(args, "url")?;
    let duration = optional_string(args, "duration")
        .map(|value| parse_duration(&value))
        .transpose()?
        .or_else(|| {
            args.get("duration_ms")
                .and_then(Value::as_u64)
                .map(Duration::from_millis)
        })
        .unwrap_or_else(|| Duration::from_secs(10));
    let capture_options = CaptureOptions {
        db_path: options.db_path.clone(),
        url,
        attach_port: options.attach_port,
        launch_port: options.launch_port,
    };
    let updates = faro_cdp::spawn_capture(capture_options);
    let deadline = Instant::now() + duration;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        let wait = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250));
        if wait.is_zero() {
            break;
        }
        match updates.recv_timeout(wait) {
            Ok(update) => events.push(capture_update_json(update, &options.db_path)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(json!({
        "db_path": options.db_path,
        "duration_ms": duration.as_millis(),
        "events": events
    }))
}

fn list_requests_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let filter = optional_string(args, "filter");
    let route = optional_string(args, "route");
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.min(500) as usize)
        .unwrap_or(100);
    let rows = request_rows_for_session(&store, &session.id)?
        .into_iter()
        .filter(|row| {
            filter
                .as_deref()
                .map(|filter| request_matches_filter(row, filter))
                .unwrap_or(true)
        })
        .filter(|row| {
            route
                .as_deref()
                .map(|route| request_matches_route(&row.url, route))
                .unwrap_or(true)
        })
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "requests": rows }))
}

fn list_sessions_tool(db_path: &Path) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let sessions = store
        .sessions()
        .context("load sessions")?
        .into_iter()
        .map(|session| {
            let counts = store
                .session_summary_counts(&session.id)
                .with_context(|| format!("load session summary for {}", session.id))?;
            anyhow::Ok(json!({
                "id": session.id,
                "created_at": session.created_at,
                "name": session.name,
                "root_url": session.root_url,
                "counts": {
                    "requests": counts.requests,
                    "errors": counts.console_errors,
                    "replays": counts.replays,
                    "websocket_frames": counts.websocket_frames,
                    "storage_events": counts.storage_events,
                    "cookie_events": counts.cookie_events
                }
            }))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(json!({ "sessions": sessions }))
}

fn delete_all_sessions_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let confirm = args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !confirm {
        bail!("delete_all_sessions requires confirm=true");
    }
    let store = open_store(&db_path.to_path_buf())?;
    let deleted = store.delete_all_sessions().context("delete all sessions")?;
    Ok(json!({ "deleted": deleted }))
}

fn get_request_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let include_body = args
        .get("include_body")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = open_store(&db_path.to_path_buf())?;
    let (request, response) = find_request_with_response(&store, &request_id)?;
    Ok(json!({
        "request": request,
        "response": response,
        "request_body": if include_body {
            load_body(&store, request.request_body_ref.as_deref())?
        } else {
            None
        },
        "response_body": if include_body {
            load_body(
                &store,
                response
                    .as_ref()
                    .and_then(|response| response.body_ref.as_deref()),
            )?
        } else {
            None
        }
    }))
}

fn get_response_body_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let store = open_store(&db_path.to_path_buf())?;
    let (_, response) = find_request_with_response(&store, &request_id)?;
    let body = load_body(
        &store,
        response
            .as_ref()
            .and_then(|response| response.body_ref.as_deref()),
    )?;
    Ok(json!({ "request_id": request_id, "body": body }))
}

fn list_console_errors_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let errors = store
        .console_logs_for_session(&session.id)
        .with_context(|| format!("load console logs for session {}", session.id))?
        .into_iter()
        .filter(|log| {
            matches!(
                log.level,
                faro_core::ConsoleLevel::Error | faro_core::ConsoleLevel::Fatal
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "errors": errors }))
}

fn get_storage_item_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let storage_type = required_string(args, "storage_type")?;
    let key = required_string(args, "key")?;
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let items = current_storage_items(&store, &session.id)?
        .into_iter()
        .filter(|item| item.storage_type == storage_type && item.key == key)
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "items": items }))
}

fn list_storage_items_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let storage_type = optional_string(args, "storage_type");
    let key_contains = optional_string(args, "key_contains").map(|value| value.to_lowercase());
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.min(1000) as usize)
        .unwrap_or(200);
    let items = current_storage_items(&store, &session.id)?
        .into_iter()
        .filter(|item| {
            storage_type
                .as_deref()
                .map(|storage_type| item.storage_type == storage_type)
                .unwrap_or(true)
        })
        .filter(|item| {
            key_contains
                .as_deref()
                .map(|needle| item.key.to_lowercase().contains(needle))
                .unwrap_or(true)
        })
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "items": items }))
}

fn list_cookies_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    Ok(json!({
        "session_id": session.id,
        "cookies": latest_cookies(&store, &session.id)?
    }))
}

fn copy_request_as_curl_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let store = open_store(&db_path.to_path_buf())?;
    let (request, _) = find_request_with_response(&store, &request_id)?;
    let request_body = load_body_text(&store, request.request_body_ref.as_deref())?;
    let args = build_curl_args(&request, request_body.as_deref());
    Ok(json!({
        "request_id": request.id,
        "command": build_curl_command(&args),
        "args": build_curl_argv(&args)
    }))
}

fn replay_request_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let store = open_store(&db_path.to_path_buf())?;
    let (request, _) = find_request_with_response(&store, &request_id)?;
    let request_body = load_body_text(&store, request.request_body_ref.as_deref())?;
    let args = build_curl_args(&request, request_body.as_deref());
    let command = build_curl_command(&args);
    let mut replay = ReplayRecord::new(
        request.session_id.clone(),
        request.tab_id.clone(),
        request.run_id.clone(),
        request.id.clone(),
        command,
    );
    let output = Command::new("curl")
        .args(&args)
        .output()
        .context("run curl replay")?;
    replay.exit_code = output.status.code().map(i64::from);
    replay.status_code = parse_http_status(&output.stdout);
    let body_text = split_http_body(&output.stdout);
    if !body_text.is_empty() {
        let body = inline_text_body(None, body_text);
        replay.response_body_ref = Some(body.id.clone());
        store
            .insert_body(&body)
            .context("insert replay response body")?;
    }
    store
        .insert_replay(&replay)
        .context("insert replay record")?;
    store
        .append_event(&request_replayed_event(&replay))
        .context("append replay event")?;
    Ok(json!(CliReplayResult {
        replay,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }))
}

fn run_readonly_sql_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let query = required_string(args, "query")?;
    let result = faro_store::Store::query_readonly(db_path, &query)
        .with_context(|| format!("run read-only SQL against {}", db_path.display()))?;
    let rows = result
        .rows
        .into_iter()
        .map(|row| {
            result
                .columns
                .iter()
                .cloned()
                .zip(row.into_iter().map(Value::String))
                .collect::<Map<_, _>>()
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "columns": result.columns,
        "row_count": rows.len(),
        "rows": rows,
        "duration_ms": result.duration_ms
    }))
}

fn resolve_session(store: &faro_store::Store, args: &Value) -> anyhow::Result<Session> {
    if let Some(session_id) = optional_string(args, "session_id") {
        return store
            .sessions()
            .context("load sessions")?
            .into_iter()
            .find(|session| session.id == session_id)
            .with_context(|| format!("session `{session_id}` not found"));
    }
    latest_session(store)?.context("no faro sessions found")
}

fn capture_update_json(update: CaptureUpdate, db_path: &Path) -> Value {
    match update {
        CaptureUpdate::SessionStarted { session_id, url } => json!({
            "kind": "session_started",
            "db_path": db_path,
            "session_id": session_id,
            "url": url
        }),
        CaptureUpdate::Attached { url, websocket_url } => json!({
            "kind": "attached",
            "db_path": db_path,
            "url": url,
            "websocket_url": websocket_url
        }),
        CaptureUpdate::Status(message) => json!({
            "kind": "status",
            "db_path": db_path,
            "message": message
        }),
        CaptureUpdate::StoreChanged => json!({
            "kind": "store_changed",
            "db_path": db_path
        }),
        CaptureUpdate::Error(message) => json!({
            "kind": "error",
            "db_path": db_path,
            "message": message
        }),
    }
}

fn required_string(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("missing string argument `{key}`"))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn tool_result(value: Value, is_error: bool) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
            }
        ],
        "isError": is_error
    })
}

fn parse_http_status(output: &[u8]) -> Option<i64> {
    let text = String::from_utf8_lossy(output);
    let mut status = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(protocol) = parts.next() else {
            continue;
        };
        if !protocol.starts_with("HTTP/") {
            continue;
        }
        let Some(value) = parts.next() else {
            continue;
        };
        let Ok(parsed) = value.parse::<i64>() else {
            continue;
        };
        status = Some(parsed);
    }
    status
}

fn split_http_body(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.rsplit_once("\r\n\r\n")
        .or_else(|| text.rsplit_once("\n\n"))
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

fn tools() -> Value {
    json!([
        tool(
            "capture_url",
            "Launch or attach Chrome, capture a URL for a bounded duration, and persist observations in the Faro DB.",
            object_schema(
                &[
                    ("url", "string", "URL to open and capture."),
                    (
                        "duration",
                        "string",
                        "Optional duration such as 10s, 500ms, or 1m."
                    ),
                    (
                        "duration_ms",
                        "number",
                        "Optional capture duration in milliseconds."
                    )
                ],
                &["url"]
            )
        ),
        tool(
            "list_sessions",
            "List Faro capture sessions with summary counts. Use this first when multiple sessions may exist.",
            object_schema(&[], &[])
        ),
        tool(
            "delete_all_sessions",
            "Delete all Faro sessions and cascaded captured data. Requires confirm=true.",
            object_schema(
                &[("confirm", "boolean", "Must be true to delete all sessions.")],
                &["confirm"]
            )
        ),
        tool(
            "list_requests",
            "List captured network requests from a session. Defaults to the latest session.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    (
                        "route",
                        "string",
                        "Optional route filter, e.g. /api/users, /api/users/:id, or /api/*."
                    ),
                    (
                        "filter",
                        "string",
                        "Optional expression filter, e.g. status >= 400."
                    ),
                    ("limit", "number", "Maximum rows to return, capped at 500.")
                ],
                &[]
            )
        ),
        tool(
            "get_request",
            "Get a captured request and response by request id.",
            object_schema(
                &[
                    ("request_id", "string", "Faro request id."),
                    (
                        "include_body",
                        "boolean",
                        "Include captured request/response body text."
                    )
                ],
                &["request_id"]
            )
        ),
        tool(
            "get_response_body",
            "Get the captured response body for a request id.",
            object_schema(
                &[("request_id", "string", "Faro request id.")],
                &["request_id"]
            )
        ),
        tool(
            "list_console_errors",
            "List console error and fatal logs from a session. Defaults to the latest session.",
            object_schema(
                &[(
                    "session_id",
                    "string",
                    "Optional Faro session id. Omit to use the latest session."
                )],
                &[]
            )
        ),
        tool(
            "list_storage_items",
            "List current localStorage/sessionStorage items for a session. Useful for discovering auth/session keys.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    (
                        "storage_type",
                        "string",
                        "Optional storage type: localStorage or sessionStorage."
                    ),
                    (
                        "key_contains",
                        "string",
                        "Optional case-insensitive substring filter for keys."
                    ),
                    ("limit", "number", "Maximum rows to return, capped at 1000.")
                ],
                &[]
            )
        ),
        tool(
            "get_storage_item",
            "Get current localStorage/sessionStorage values for a key.",
            object_schema(
                &[
                    (
                        "session_id",
                        "string",
                        "Optional Faro session id. Omit to use the latest session."
                    ),
                    ("storage_type", "string", "localStorage or sessionStorage."),
                    ("key", "string", "Storage key.")
                ],
                &["storage_type", "key"]
            )
        ),
        tool(
            "list_cookies",
            "List cookies from a session's latest cookie snapshot. Defaults to the latest session.",
            object_schema(
                &[(
                    "session_id",
                    "string",
                    "Optional Faro session id. Omit to use the latest session."
                )],
                &[]
            )
        ),
        tool(
            "copy_request_as_curl",
            "Build a shareable curl command for a captured request.",
            object_schema(
                &[("request_id", "string", "Faro request id.")],
                &["request_id"]
            )
        ),
        tool(
            "replay_request",
            "Replay a captured request with curl and persist the replay record.",
            object_schema(
                &[("request_id", "string", "Faro request id.")],
                &["request_id"]
            )
        ),
        tool(
            "run_readonly_sql",
            "Run a read-only SQL query against the Faro SQLite database.",
            object_schema(&[("query", "string", "Read-only SQL query.")], &["query"])
        )
    ])
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn object_schema(properties: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let mut props = Map::new();
    for (name, kind, description) in properties {
        props.insert(
            (*name).to_string(),
            json!({
                "type": kind,
                "description": description
            }),
        );
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::{RequestRecord, Session};
    use faro_store::Store;
    use std::time::{SystemTime, UNIX_EPOCH};

    type TestResult = anyhow::Result<()>;

    fn test_options(db_path: std::path::PathBuf) -> CliOptions {
        CliOptions {
            db_path,
            attach_port: None,
            launch_port: None,
            launch_on_start: false,
        }
    }

    fn temp_db_path(name: &str) -> anyhow::Result<std::path::PathBuf> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        Ok(std::env::temp_dir().join(format!("faro-mcp-{name}-{}-{now}.db", std::process::id())))
    }

    fn response_text(response: &Value) -> anyhow::Result<Value> {
        let text = response
            .get("result")
            .and_then(|result| result.get("content"))
            .and_then(Value::as_array)
            .and_then(|content| content.first())
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .context("missing MCP text content")?;
        serde_json::from_str(text).context("parse MCP tool text content")
    }

    #[test]
    fn tools_list_includes_session_tools() -> TestResult {
        let options = test_options(temp_db_path("tools")?);
        let response = handle_message(
            &options,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .context("missing tools/list response")?;
        let tools = response
            .get("result")
            .and_then(|result| result.get("tools"))
            .and_then(Value::as_array)
            .context("missing tools")?;
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(names.contains(&"list_sessions"));
        assert!(names.contains(&"delete_all_sessions"));
        assert!(names.contains(&"list_storage_items"));
        Ok(())
    }

    #[test]
    fn list_requests_accepts_explicit_session_id() -> TestResult {
        let db_path = temp_db_path("sessions")?;
        let first = Session {
            id: "first".to_string(),
            created_at: 1,
            name: Some("first".to_string()),
            root_url: Some("https://first.test".to_string()),
        };
        let second = Session {
            id: "second".to_string(),
            created_at: 2,
            name: Some("second".to_string()),
            root_url: Some("https://second.test".to_string()),
        };
        {
            let store = Store::open(&db_path)?;
            store.insert_session(&first)?;
            store.insert_session(&second)?;
            store.insert_request(&RequestRecord::started(
                first.id.clone(),
                None,
                None,
                "GET",
                "https://first.test/api",
            ))?;
            store.insert_request(&RequestRecord::started(
                second.id.clone(),
                None,
                None,
                "GET",
                "https://second.test/api",
            ))?;
        }

        let options = test_options(db_path.clone());
        let response = handle_message(
            &options,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "list_requests",
                    "arguments": { "session_id": "first" }
                }
            }),
        )
        .context("missing tools/call response")?;
        let value = response_text(&response)?;

        assert_eq!(
            value.get("session_id").and_then(Value::as_str),
            Some("first")
        );
        let requests = value
            .get("requests")
            .and_then(Value::as_array)
            .context("missing requests")?;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests
                .first()
                .and_then(|request| request.get("url"))
                .and_then(Value::as_str),
            Some("https://first.test/api")
        );
        std::fs::remove_file(db_path).context("remove temp MCP db")?;
        Ok(())
    }

    #[test]
    fn delete_all_sessions_requires_confirmation() -> TestResult {
        let db_path = temp_db_path("delete")?;
        {
            let store = Store::open(&db_path)?;
            store.insert_session(&Session {
                id: "session".to_string(),
                created_at: 1,
                name: None,
                root_url: Some("https://example.test".to_string()),
            })?;
        }
        let options = test_options(db_path.clone());
        let response = handle_message(
            &options,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "delete_all_sessions",
                    "arguments": {}
                }
            }),
        )
        .context("missing tools/call response")?;

        assert!(response.get("error").is_some());
        assert_eq!(Store::open(&db_path)?.sessions()?.len(), 1);
        std::fs::remove_file(db_path).context("remove temp MCP db")?;
        Ok(())
    }
}
