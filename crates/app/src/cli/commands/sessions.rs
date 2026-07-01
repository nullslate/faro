use crate::services::session_summaries;
use anyhow::{Context, bail};
use serde::Serialize;
use std::path::PathBuf;

use super::super::output::print_json;
use super::super::store::open_store;
use super::db::handle_compact;

#[derive(Debug, Serialize)]
struct CliSessionRow {
    id: String,
    created_at: i64,
    name: Option<String>,
    root_url: Option<String>,
    requests: usize,
    errors: usize,
    replays: usize,
    websocket_frames: usize,
    storage_events: usize,
    cookie_events: usize,
}

#[derive(Debug, Serialize)]
struct CliSessionsNukeResult {
    deleted: usize,
}

#[derive(Debug, Default, Serialize)]
struct CliSessionsPruneResult {
    session_id: String,
    repeated_requests_deleted: usize,
    old_requests_deleted: usize,
    console_logs_deleted: usize,
    websocket_frames_deleted: usize,
    vacuumed: bool,
}

pub(crate) fn handle_sessions(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    let Some(command) = args.first().cloned() else {
        bail!("usage: faro sessions <list|compact|nuke|prune> [--json] [--yes] [--vacuum]");
    };
    args.remove(0);
    match command.as_str() {
        "list" => handle_sessions_list(db_path, args),
        "compact" => handle_sessions_compact(db_path, args),
        "nuke" | "clear" => handle_sessions_nuke(db_path, args),
        "prune" => handle_sessions_prune(db_path, args),
        _ => bail!("usage: faro sessions <list|compact|nuke|prune> [--json] [--yes] [--vacuum]"),
    }
}

fn handle_sessions_list(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown sessions list option: {arg}");
        }
    }
    let store = open_store(db_path)?;
    let rows = session_summaries(&store)?
        .into_iter()
        .map(|summary| CliSessionRow {
            id: summary.session.id,
            created_at: summary.session.created_at,
            name: summary.session.name,
            root_url: summary.session.root_url,
            requests: summary.request_count,
            errors: summary.console_error_count,
            replays: summary.replay_count,
            websocket_frames: summary.websocket_count,
            storage_events: summary.storage_count,
            cookie_events: summary.cookie_count,
        })
        .collect::<Vec<_>>();

    if json_output {
        print_json(&rows)?;
    } else {
        for row in rows {
            println!(
                "{} req={} err={} replay={} ws={} store={} cookie={} {}",
                row.id,
                row.requests,
                row.errors,
                row.replays,
                row.websocket_frames,
                row.storage_events,
                row.cookie_events,
                row.root_url.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

fn handle_sessions_nuke(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let json_output = args.iter().any(|arg| arg == "--json");
    let confirmed = args.iter().any(|arg| arg == "--yes" || arg == "-y");
    let vacuum = args.iter().any(|arg| arg == "--vacuum");
    for arg in &args {
        if !matches!(arg.as_str(), "--json" | "--yes" | "-y" | "--vacuum") {
            bail!("unknown sessions nuke option: {arg}");
        }
    }
    if !confirmed {
        bail!("usage: faro sessions nuke --yes [--json] [--vacuum]");
    }

    let store = open_store(db_path)?;
    let deleted = store.delete_all_sessions().context("delete all sessions")?;
    if vacuum {
        store.checkpoint_and_vacuum().context("vacuum database")?;
    }
    let result = CliSessionsNukeResult { deleted };
    if json_output {
        print_json(&result)?;
    } else {
        println!("deleted {deleted} sessions");
    }
    Ok(())
}

fn handle_sessions_compact(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    handle_compact(db_path, args)
}

fn handle_sessions_prune(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let mut session_id = None;
    let mut max_requests = None;
    let mut max_repeated = None;
    let mut max_console = None;
    let mut max_websocket = None;
    let mut json_output = false;
    let mut vacuum = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json_output = true,
            "--vacuum" => vacuum = true,
            "--max-requests" => {
                index += 1;
                max_requests = Some(parse_positive_usize(args.get(index), "--max-requests")?);
            }
            "--max-repeated" => {
                index += 1;
                max_repeated = Some(parse_positive_usize(args.get(index), "--max-repeated")?);
            }
            "--max-console" => {
                index += 1;
                max_console = Some(parse_positive_usize(args.get(index), "--max-console")?);
            }
            "--max-ws" | "--max-websocket" => {
                index += 1;
                max_websocket = Some(parse_positive_usize(
                    args.get(index),
                    args[index - 1].as_str(),
                )?);
            }
            value if value.starts_with('-') => bail!("unknown sessions prune option: {value}"),
            value => {
                if session_id.is_some() {
                    bail!(
                        "usage: faro sessions prune <session-id> [--max-requests N] [--max-repeated N] [--max-console N] [--max-ws N] [--vacuum] [--json]"
                    );
                }
                session_id = Some(value.to_string());
            }
        }
        index += 1;
    }

    let Some(session_id) = session_id else {
        bail!(
            "usage: faro sessions prune <session-id> [--max-requests N] [--max-repeated N] [--max-console N] [--max-ws N] [--vacuum] [--json]"
        );
    };
    if max_requests.is_none()
        && max_repeated.is_none()
        && max_console.is_none()
        && max_websocket.is_none()
    {
        bail!("sessions prune requires at least one max option");
    }

    let store = open_store(db_path)?;
    if !store.session_exists(&session_id)? {
        bail!("session not found: {session_id}");
    }

    let mut result = CliSessionsPruneResult {
        session_id: session_id.clone(),
        vacuumed: vacuum,
        ..CliSessionsPruneResult::default()
    };
    if let Some(limit) = max_repeated {
        result.repeated_requests_deleted = store
            .prune_repeated_session_requests(&session_id, limit)
            .context("prune repeated session requests")?;
    }
    if let Some(limit) = max_requests {
        result.old_requests_deleted = store
            .prune_session_requests(&session_id, limit)
            .context("prune old session requests")?;
    }
    if let Some(limit) = max_console {
        result.console_logs_deleted = store
            .prune_session_console_logs(&session_id, limit)
            .context("prune old console logs")?;
    }
    if let Some(limit) = max_websocket {
        result.websocket_frames_deleted = store
            .prune_session_websocket_frames(&session_id, limit)
            .context("prune old websocket frames")?;
    }
    if vacuum {
        store.checkpoint_and_vacuum().context("vacuum database")?;
    }

    if json_output {
        print_json(&result)?;
    } else {
        println!(
            "pruned {} repeated={} old_requests={} console={} ws={}{}",
            result.session_id,
            result.repeated_requests_deleted,
            result.old_requests_deleted,
            result.console_logs_deleted,
            result.websocket_frames_deleted,
            if result.vacuumed { " vacuumed" } else { "" }
        );
    }
    Ok(())
}

fn parse_positive_usize(value: Option<&String>, flag: &str) -> anyhow::Result<usize> {
    let Some(value) = value else {
        bail!("{flag} requires a value");
    };
    let parsed = value
        .parse::<usize>()
        .with_context(|| format!("parse {flag} value"))?;
    if parsed == 0 {
        bail!("{flag} must be greater than zero");
    }
    Ok(parsed)
}
