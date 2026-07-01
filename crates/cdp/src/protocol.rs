use crate::Result;
use futures_util::SinkExt;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

pub(crate) async fn send_command(
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
