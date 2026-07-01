use crate::protocol::send_command;
use crate::{CdpError, Result};
use faro_core::CookieRecord;
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

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
