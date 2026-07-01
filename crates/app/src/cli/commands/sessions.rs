use crate::services::session_summaries;
use anyhow::{Context, bail};
use serde::Serialize;
use std::path::PathBuf;

use super::super::output::print_json;
use super::super::store::open_store;

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

#[derive(Debug, Serialize)]
struct CliSessionsCompactResult {
    orphan_bodies_deleted: usize,
    vacuumed: bool,
}

pub(crate) fn handle_sessions(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    let Some(command) = args.first().cloned() else {
        bail!("usage: faro sessions <list|compact|nuke> [--json] [--yes] [--vacuum]");
    };
    args.remove(0);
    match command.as_str() {
        "list" => handle_sessions_list(db_path, args),
        "compact" => handle_sessions_compact(db_path, args),
        "nuke" | "clear" => handle_sessions_nuke(db_path, args),
        _ => bail!("usage: faro sessions <list|compact|nuke> [--json] [--yes] [--vacuum]"),
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
    let json_output = args.iter().any(|arg| arg == "--json");
    let vacuum = args.iter().any(|arg| arg == "--vacuum");
    for arg in &args {
        if !matches!(arg.as_str(), "--json" | "--vacuum") {
            bail!("unknown sessions compact option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let orphan_bodies_deleted = store
        .delete_orphan_bodies()
        .context("delete orphan bodies")?;
    if vacuum {
        store.checkpoint_and_vacuum().context("vacuum database")?;
    }
    let result = CliSessionsCompactResult {
        orphan_bodies_deleted,
        vacuumed: vacuum,
    };
    if json_output {
        print_json(&result)?;
    } else if vacuum {
        println!("deleted {orphan_bodies_deleted} orphan bodies and vacuumed database");
    } else {
        println!("deleted {orphan_bodies_deleted} orphan bodies");
    }
    Ok(())
}
