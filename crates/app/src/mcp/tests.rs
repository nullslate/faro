use super::*;
use crate::config::RedactionConfig;
use faro_core::{Header, RequestRecord, Session};
use faro_store::Store;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult = anyhow::Result<()>;

fn test_options(db_path: std::path::PathBuf) -> CliOptions {
    CliOptions {
        db_path,
        attach_port: None,
        launch_port: None,
        launch_on_start: false,
        mcp_allow_mutation: false,
        mcp_allow_sensitive: false,
        redaction: RedactionConfig::default(),
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
    assert!(names.contains(&"get_session"));
    assert!(names.contains(&"delete_all_sessions"));
    assert!(names.contains(&"list_storage_items"));
    assert!(names.contains(&"list_replays"));
    assert!(names.contains(&"get_replay"));
    assert!(names.contains(&"list_websocket_frames"));
    assert!(names.contains(&"evaluate_js"));
    assert!(names.contains(&"reload_page"));
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

#[test]
fn mutating_mcp_tools_require_opt_in() -> TestResult {
    let options = test_options(temp_db_path("mutating")?);
    let response = handle_message(
        &options,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "reload_page",
                "arguments": { "websocket_url": "ws://127.0.0.1/devtools/page/1" }
            }
        }),
    )
    .context("missing tools/call response")?;

    let message = response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .context("missing error message")?;
    assert!(message.contains("--mcp-allow-mutation"));
    Ok(())
}

#[test]
fn copy_request_as_curl_redacts_by_default() -> TestResult {
    let db_path = temp_db_path("safe-curl")?;
    let request_id;
    {
        let store = Store::open(&db_path)?;
        let session = Session {
            id: "session".to_string(),
            created_at: 1,
            name: None,
            root_url: Some("https://example.test".to_string()),
        };
        store.insert_session(&session)?;
        let mut request =
            RequestRecord::started(session.id, None, None, "POST", "https://example.test/api");
        request
            .request_headers
            .push(Header::new("authorization", "Bearer secret-token"));
        request
            .request_headers
            .push(Header::new("content-type", "application/json"));
        request_id = request.id.clone();
        store.insert_request(&request)?;
    }

    let options = test_options(db_path.clone());
    let response = handle_message(
        &options,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "copy_request_as_curl",
                "arguments": { "request_id": request_id }
            }
        }),
    )
    .context("missing tools/call response")?;
    let value = response_text(&response)?;
    let command = value
        .get("command")
        .and_then(Value::as_str)
        .context("missing command")?;

    assert!(command.contains("authorization: [redacted]"));
    assert!(!command.contains("secret-token"));
    assert_eq!(value.get("redacted").and_then(Value::as_bool), Some(true));
    std::fs::remove_file(db_path).context("remove temp MCP db")?;
    Ok(())
}

#[test]
fn sensitive_curl_requires_opt_in() -> TestResult {
    let options = test_options(temp_db_path("sensitive")?);
    let response = handle_message(
        &options,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "copy_request_as_curl",
                "arguments": {
                    "request_id": "missing",
                    "include_sensitive": true
                }
            }
        }),
    )
    .context("missing tools/call response")?;
    let message = response
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .context("missing error message")?;
    assert!(message.contains("--mcp-allow-sensitive"));
    Ok(())
}
