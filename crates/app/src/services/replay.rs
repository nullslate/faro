use super::body::load_body_text;
use super::curl::{build_curl_args, build_curl_command};
use super::requests::request_with_latest_response;
use anyhow::Context;
use faro_core::ReplayRecord;
use faro_store::{Store, inline_text_body};
use serde::Serialize;
use std::process::Command;

pub(crate) struct ReplayPlan {
    pub(crate) args: Vec<String>,
    pub(crate) replay: ReplayRecord,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReplayExecution {
    pub(crate) replay: ReplayRecord,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) fn replay_plan(store: &Store, request_id: &str) -> anyhow::Result<ReplayPlan> {
    let (request, _) = request_with_latest_response(store, request_id)?;
    let request_body = load_body_text(store, request.request_body_ref.as_deref())?;
    let args = build_curl_args(&request, request_body.as_deref());
    let command = build_curl_command(&args);
    let replay = ReplayRecord::new(
        request.session_id.clone(),
        request.tab_id.clone(),
        request.run_id.clone(),
        request.id.clone(),
        command,
    );
    Ok(ReplayPlan { args, replay })
}

pub(crate) fn execute_replay(store: &Store, request_id: &str) -> anyhow::Result<ReplayExecution> {
    if !command_exists("curl") {
        anyhow::bail!("cannot replay: curl not found");
    }

    let plan = replay_plan(store, request_id)?;
    let mut replay = plan.replay;
    let output = Command::new("curl")
        .args(&plan.args)
        .output()
        .context("run curl replay")?;
    replay.exit_code = output.status.code().map(i64::from);
    replay.status_code = parse_http_status(&output.stdout);
    let body_text = split_http_body(&output.stdout);
    if !body_text.is_empty() {
        let body = inline_text_body(None, body_text);
        replay.response_body_ref = Some(body.id.clone());
        store
            .insert_body(&body)
            .context("insert replay response body")?;
    }
    store
        .insert_replay(&replay)
        .context("insert replay record")?;
    store
        .append_event(&faro_core::request_replayed_event(&replay))
        .context("append replay event")?;

    Ok(ReplayExecution {
        replay,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub(crate) fn parse_http_status(output: &[u8]) -> Option<i64> {
    let text = String::from_utf8_lossy(output);
    let mut status = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(protocol) = parts.next() else {
            continue;
        };
        if !protocol.starts_with("HTTP/") {
            continue;
        }
        let Some(value) = parts.next() else {
            continue;
        };
        let Ok(parsed) = value.parse::<i64>() else {
            continue;
        };
        status = Some(parsed);
    }
    status
}

pub(crate) fn split_http_body(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.rsplit_once("\r\n\r\n")
        .or_else(|| text.rsplit_once("\n\n"))
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(command).exists()))
        .unwrap_or(false)
}
