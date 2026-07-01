use super::persistence::{
    PendingBody, PendingStorage, cookie_agent_script, storage_snapshot_expression,
};
use crate::Result;
use crate::protocol::send_command;
use serde_json::json;
use std::collections::HashMap;
use tokio_tungstenite::connect_async;

pub(super) type CdpSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone)]
pub(super) enum PendingCommand {
    Body(PendingBody),
    Storage(PendingStorage),
    Cookies,
}

pub(super) async fn connect_and_enable_capture(
    websocket_url: &str,
    target_url: &str,
    requested_url: &str,
) -> Result<(CdpSocket, i64)> {
    let (mut ws, _) = connect_async(websocket_url).await?;
    let mut next_id = 1_i64;
    send_command(&mut ws, &mut next_id, "Page.enable", json!({})).await?;
    send_command(&mut ws, &mut next_id, "Runtime.enable", json!({})).await?;
    send_command(&mut ws, &mut next_id, "DOMStorage.enable", json!({})).await?;
    send_command(
        &mut ws,
        &mut next_id,
        "Runtime.addBinding",
        json!({ "name": "faroCookieMutation" }),
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
    if target_url != requested_url {
        send_command(
            &mut ws,
            &mut next_id,
            "Page.navigate",
            json!({ "url": requested_url }),
        )
        .await?;
    } else {
        send_command(&mut ws, &mut next_id, "Page.reload", json!({})).await?;
    }
    Ok((ws, next_id))
}

pub(super) async fn request_state_snapshots(
    ws: &mut CdpSocket,
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

pub(super) async fn request_response_body(
    ws: &mut CdpSocket,
    next_id: &mut i64,
    pending_commands: &mut HashMap<i64, PendingCommand>,
    request_id: &str,
    mime_type: Option<String>,
) -> Result<()> {
    let command_id = send_command(
        ws,
        next_id,
        "Network.getResponseBody",
        json!({ "requestId": request_id }),
    )
    .await?;
    pending_commands.insert(
        command_id,
        PendingCommand::Body(PendingBody {
            request_id: request_id.to_string(),
            mime_type,
        }),
    );
    Ok(())
}
