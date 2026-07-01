use crate::cli::CliOptions;
use faro_cdp::CaptureOptions;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

use super::helpers::{capture_update_json, parse_capture_duration, required_string};

pub(super) fn capture_url_tool(options: &CliOptions, args: &Value) -> anyhow::Result<Value> {
    let url = required_string(args, "url")?;
    let duration = parse_capture_duration(args)?;
    let capture_options = CaptureOptions {
        db_path: options.db_path.clone(),
        url,
        attach_port: options.attach_port,
        launch_port: options.launch_port,
        max_requests_per_session: options.max_requests_per_session,
        max_repeated_requests_per_url: options.max_repeated_requests_per_url,
        prune_interval_requests: options.prune_interval_requests,
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
