use crate::cli::CliOptions;
use anyhow::Context;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

mod schema;
mod security;
mod tool_handlers;

use schema::tools;
use tool_handlers::call_tool;

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

#[cfg(test)]
mod tests;
