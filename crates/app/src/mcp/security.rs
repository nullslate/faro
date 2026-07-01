use crate::cli::CliOptions;
use anyhow::bail;
use faro_core::config_dir;
use serde_json::{Value, json};
use std::io::Write;

pub(super) fn audit_tool_call(name: &str, args: &Value) {
    let sensitive_curl = name == "copy_request_as_curl"
        && args
            .get("include_sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if !sensitive_curl
        && !matches!(
            name,
            "capture_url"
                | "delete_all_sessions"
                | "prune_session"
                | "replay_request"
                | "evaluate_js"
                | "reload_page"
        )
    {
        return;
    }
    append_audit_event(
        "mcp.tool_call",
        json!({
            "tool": name,
            "sensitive": sensitive_curl,
            "mutating": !sensitive_curl
        }),
    );
}

fn append_audit_event(action: &str, details: Value) {
    let Some(dir) = config_dir("faro") else {
        return;
    };
    let event = json!({
        "ts": faro_core::now_ms(),
        "source": "mcp",
        "action": action,
        "details": details
    });
    let result = std::fs::create_dir_all(&dir).and_then(|()| {
        let path = dir.join("audit.jsonl");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{event}")
    });
    let _ = result;
}

pub(super) fn require_tool_permission(
    options: &CliOptions,
    name: &str,
    args: &Value,
) -> anyhow::Result<()> {
    if matches!(
        name,
        "capture_url"
            | "delete_all_sessions"
            | "prune_session"
            | "replay_request"
            | "evaluate_js"
            | "reload_page"
    ) && !options.mcp_allow_mutation
    {
        bail!(
            "MCP tool `{name}` is disabled by default; restart faro mcp with --mcp-allow-mutation to enable mutating/browser actions"
        );
    }
    if name == "copy_request_as_curl"
        && args
            .get("include_sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !options.mcp_allow_sensitive
    {
        bail!(
            "copy_request_as_curl include_sensitive=true is disabled by default; restart faro mcp with --mcp-allow-sensitive to include credentials and request bodies"
        );
    }
    Ok(())
}
