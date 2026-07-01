use anyhow::Context;
use serde_json::{Value, json};

use super::helpers::required_string;

pub(super) fn evaluate_js_tool(args: &Value) -> anyhow::Result<Value> {
    let websocket_url = required_string(args, "websocket_url")?;
    let expression = required_string(args, "expression")?;
    let result = faro_cdp::evaluate_expression_blocking(&websocket_url, &expression)
        .context("evaluate JavaScript through CDP")?;
    Ok(json!({ "result": result }))
}

pub(super) fn reload_page_tool(args: &Value) -> anyhow::Result<Value> {
    let websocket_url = required_string(args, "websocket_url")?;
    faro_cdp::reload_page_blocking(&websocket_url).context("reload page through CDP")?;
    Ok(json!({ "reloaded": true }))
}
