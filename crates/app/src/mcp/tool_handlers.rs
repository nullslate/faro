use crate::cli::CliOptions;
use anyhow::{Context, bail};
use serde_json::{Value, json};

mod browser;
mod capture;
mod data;
mod helpers;
mod requests;
mod sessions;

use browser::{evaluate_js_tool, reload_page_tool};
use capture::capture_url_tool;
use data::{
    get_storage_item_tool, list_console_errors_tool, list_cookies_tool, list_storage_items_tool,
    list_websocket_frames_tool, run_readonly_sql_tool,
};
use requests::{
    copy_request_as_curl_tool, get_replay_tool, get_request_tool, get_response_body_tool,
    list_replays_tool, list_requests_tool, replay_request_tool,
};
use sessions::{delete_all_sessions_tool, get_session_tool, list_sessions_tool};

use super::security::{audit_tool_call, require_tool_permission};

pub(super) fn call_tool(options: &CliOptions, params: &Value) -> anyhow::Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .context("tools/call missing name")?;
    let args = params.get("arguments").unwrap_or(&Value::Null);
    require_tool_permission(options, name, args)?;
    audit_tool_call(name, args);
    let result = match name {
        "capture_url" => capture_url_tool(options, args),
        "list_sessions" => list_sessions_tool(&options.db_path),
        "get_session" => get_session_tool(&options.db_path, args),
        "delete_all_sessions" => delete_all_sessions_tool(&options.db_path, args),
        "list_requests" => list_requests_tool(&options.db_path, args),
        "get_request" => get_request_tool(options, args),
        "get_response_body" => get_response_body_tool(options, args),
        "list_replays" => list_replays_tool(&options.db_path, args),
        "get_replay" => get_replay_tool(options, args),
        "list_websocket_frames" => list_websocket_frames_tool(&options.db_path, args),
        "list_console_errors" => list_console_errors_tool(&options.db_path, args),
        "list_storage_items" => list_storage_items_tool(&options.db_path, args),
        "get_storage_item" => get_storage_item_tool(&options.db_path, args),
        "list_cookies" => list_cookies_tool(&options.db_path, args),
        "copy_request_as_curl" => copy_request_as_curl_tool(options, args),
        "replay_request" => replay_request_tool(&options.db_path, args),
        "evaluate_js" => evaluate_js_tool(args),
        "reload_page" => reload_page_tool(args),
        "run_readonly_sql" => run_readonly_sql_tool(&options.db_path, args),
        _ => bail!("unknown tool `{name}`"),
    }?;
    Ok(tool_result(result, false))
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
