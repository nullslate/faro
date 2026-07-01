use super::options::{CliOptions, parse_duration};
use super::output::print_json;
use anyhow::{Context, bail};
use faro_cdp::{CaptureOptions, CaptureUpdate};
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
struct CliCaptureEvent {
    kind: String,
    db_path: String,
    session_id: Option<String>,
    url: Option<String>,
    websocket_url: Option<String>,
    message: Option<String>,
}

pub(super) fn handle_capture(options: CliOptions, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut duration = None;
    let mut positional = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json_output = true,
            "--for" | "--duration" => {
                let Some(value) = iter.next() else {
                    bail!("usage: faro capture <url> [--for <duration>] [--json]");
                };
                duration = Some(parse_duration(&value)?);
            }
            _ => positional.push(arg),
        }
    }
    let Some(url) = positional.first().cloned() else {
        bail!("usage: faro capture <url> [--for <duration>] [--json]");
    };
    if positional.len() > 1 {
        bail!("usage: faro capture <url> [--for <duration>] [--json]");
    }

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
    let deadline = duration.map(|duration| Instant::now() + duration);
    if !json_output {
        println!("capturing into {}", options.db_path.display());
    }

    loop {
        let wait = deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|| Duration::from_millis(250));
        if wait.is_zero() {
            break;
        }
        match updates.recv_timeout(wait.min(Duration::from_millis(250))) {
            Ok(update) => print_capture_update(update, &options.db_path, json_output)?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn print_capture_update(
    update: CaptureUpdate,
    db_path: &Path,
    json_output: bool,
) -> anyhow::Result<()> {
    let db_path = db_path.display().to_string();
    let event = match update {
        CaptureUpdate::SessionStarted { session_id, url } => CliCaptureEvent {
            kind: "session_started".to_string(),
            db_path,
            session_id: Some(session_id),
            url: Some(url),
            websocket_url: None,
            message: None,
        },
        CaptureUpdate::Attached { url, websocket_url } => CliCaptureEvent {
            kind: "attached".to_string(),
            db_path,
            session_id: None,
            url: Some(url),
            websocket_url: Some(websocket_url),
            message: None,
        },
        CaptureUpdate::Status(message) => CliCaptureEvent {
            kind: "status".to_string(),
            db_path,
            session_id: None,
            url: None,
            websocket_url: None,
            message: Some(message),
        },
        CaptureUpdate::StoreChanged => CliCaptureEvent {
            kind: "store_changed".to_string(),
            db_path,
            session_id: None,
            url: None,
            websocket_url: None,
            message: None,
        },
        CaptureUpdate::Error(message) => CliCaptureEvent {
            kind: "error".to_string(),
            db_path,
            session_id: None,
            url: None,
            websocket_url: None,
            message: Some(message),
        },
    };
    if json_output {
        print_json(&event).context("serialize capture JSON event")?;
    } else {
        print_capture_text(&event);
    }
    Ok(())
}

fn print_capture_text(event: &CliCaptureEvent) {
    match event.kind.as_str() {
        "session_started" => {
            println!(
                "session {} {}",
                event.session_id.as_deref().unwrap_or("-"),
                event.url.as_deref().unwrap_or("-")
            );
        }
        "attached" => {
            println!(
                "attached {} {}",
                event.url.as_deref().unwrap_or("-"),
                event.websocket_url.as_deref().unwrap_or("-")
            );
        }
        "status" => println!("{}", event.message.as_deref().unwrap_or("status")),
        "store_changed" => println!("store changed"),
        "error" => eprintln!("capture error: {}", event.message.as_deref().unwrap_or("-")),
        _ => {}
    }
}
