use faro_core::WebSocketFrameRecord;

pub(super) fn filter_websocket_indices(
    frames: &[WebSocketFrameRecord],
    filter: &str,
) -> Vec<usize> {
    let filter = filter.trim().to_lowercase();
    frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| websocket_frame_matches(frame, &filter).then_some(index))
        .collect()
}

fn websocket_frame_matches(frame: &WebSocketFrameRecord, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let direction = frame.direction.as_str();
    let opcode = websocket_opcode_label(frame.opcode);
    direction.contains(filter)
        || opcode.contains(filter)
        || frame.browser_request_id.to_lowercase().contains(filter)
        || frame.payload.to_lowercase().contains(filter)
}

fn websocket_opcode_label(opcode: i64) -> &'static str {
    match opcode {
        0 => "continuation",
        1 => "text",
        2 => "binary",
        8 => "close",
        9 => "ping",
        10 => "pong",
        _ => "other",
    }
}
