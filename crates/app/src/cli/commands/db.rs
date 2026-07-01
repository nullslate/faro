use anyhow::{Context, bail};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::output::print_json;
use super::super::store::open_store;
use crate::query::path_for_url;

#[derive(Debug, Serialize)]
pub(super) struct CliCompactResult {
    pub(super) orphan_bodies_deleted: usize,
    pub(super) vacuumed: bool,
}

#[derive(Debug, Serialize)]
struct CliDbStats {
    db_path: String,
    db_bytes: u64,
    wal_bytes: u64,
    shm_bytes: u64,
    total_bytes: u64,
    body_storage: CliBodyStorageStats,
    top_sessions: Vec<CliSessionStorageStats>,
    top_repeated_requests: Vec<CliRepeatedRequestGroup>,
    tables: Vec<CliTableStats>,
}

#[derive(Debug, Serialize)]
struct CliBodyStorageStats {
    bodies: usize,
    total_bytes: u64,
    inline_bytes: u64,
    external_bytes: u64,
}

#[derive(Debug, Serialize)]
struct CliTableStats {
    table: String,
    rows: usize,
}

#[derive(Debug, Serialize)]
struct CliSessionStorageStats {
    id: String,
    created_at: i64,
    name: Option<String>,
    root_url: Option<String>,
    requests: usize,
    console_errors: usize,
    replays: usize,
    websocket_frames: usize,
    storage_events: usize,
    cookie_events: usize,
    bodies: usize,
    body_bytes: u64,
}

#[derive(Debug, Serialize)]
struct CliRepeatedRequestGroup {
    session_id: String,
    root_url: Option<String>,
    method: String,
    resource_type: Option<String>,
    domain: String,
    path: String,
    url: String,
    requests: usize,
    error_responses: usize,
    body_bytes: u64,
    first_started_at: i64,
    last_started_at: i64,
}

pub(crate) fn handle_db(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    let Some(command) = args.first().cloned() else {
        bail!("usage: faro db <stats|compact> [--json] [--vacuum]");
    };
    args.remove(0);
    match command.as_str() {
        "stats" => handle_stats(db_path, args),
        "compact" => handle_compact(db_path, args),
        _ => bail!("usage: faro db <stats|compact> [--json] [--vacuum]"),
    }
}

fn handle_stats(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown db stats option: {arg}");
        }
    }

    let stats = database_stats(db_path)?;
    if json_output {
        print_json(&stats)?;
    } else {
        println!("database {}", stats.db_path);
        println!(
            "size db={} wal={} shm={} total={}",
            format_bytes(stats.db_bytes),
            format_bytes(stats.wal_bytes),
            format_bytes(stats.shm_bytes),
            format_bytes(stats.total_bytes)
        );
        println!(
            "bodies count={} total={} inline={} external={}",
            stats.body_storage.bodies,
            format_bytes(stats.body_storage.total_bytes),
            format_bytes(stats.body_storage.inline_bytes),
            format_bytes(stats.body_storage.external_bytes)
        );
        if !stats.top_sessions.is_empty() {
            println!("top sessions");
            for session in &stats.top_sessions {
                println!(
                    "  {} req={} body={} bodies={} ws={} err={} {}",
                    session.id,
                    session.requests,
                    format_bytes(session.body_bytes),
                    session.bodies,
                    session.websocket_frames,
                    session.console_errors,
                    session.root_url.as_deref().unwrap_or("-")
                );
            }
        }
        if !stats.top_repeated_requests.is_empty() {
            println!("top repeated requests");
            for group in &stats.top_repeated_requests {
                println!(
                    "  {} {} {}{} count={} body={} errors={} session={}",
                    group.method,
                    group.resource_type.as_deref().unwrap_or("-"),
                    group.domain,
                    group.path,
                    group.requests,
                    format_bytes(group.body_bytes),
                    group.error_responses,
                    group.session_id
                );
            }
        }
        for table in stats.tables {
            println!("{:<20} {}", table.table, table.rows);
        }
    }
    Ok(())
}

pub(super) fn handle_compact(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let json_output = args.iter().any(|arg| arg == "--json");
    let vacuum = args.iter().any(|arg| arg == "--vacuum");
    for arg in &args {
        if !matches!(arg.as_str(), "--json" | "--vacuum") {
            bail!("unknown compact option: {arg}");
        }
    }

    let result = compact_database(db_path, vacuum)?;
    if json_output {
        print_json(&result)?;
    } else if vacuum {
        println!(
            "deleted {} orphan bodies and vacuumed database",
            result.orphan_bodies_deleted
        );
    } else {
        println!("deleted {} orphan bodies", result.orphan_bodies_deleted);
    }
    Ok(())
}

fn compact_database(db_path: &PathBuf, vacuum: bool) -> anyhow::Result<CliCompactResult> {
    let store = open_store(db_path)?;
    let orphan_bodies_deleted = store
        .delete_orphan_bodies()
        .context("delete orphan bodies")?;
    if vacuum {
        store.checkpoint_and_vacuum().context("vacuum database")?;
    }
    Ok(CliCompactResult {
        orphan_bodies_deleted,
        vacuumed: vacuum,
    })
}

fn database_stats(db_path: &PathBuf) -> anyhow::Result<CliDbStats> {
    let store = open_store(db_path)?;
    let db_bytes = file_size(db_path).unwrap_or(0);
    let wal_path = sidecar_path(db_path, "wal");
    let shm_path = sidecar_path(db_path, "shm");
    let wal_bytes = file_size(&wal_path).unwrap_or(0);
    let shm_bytes = file_size(&shm_path).unwrap_or(0);
    let body_storage = store
        .body_storage_stats()
        .context("load body storage stats")?;
    let top_sessions = store
        .top_session_storage_stats(5)
        .context("load top session storage stats")?
        .into_iter()
        .map(|session| CliSessionStorageStats {
            id: session.id,
            created_at: session.created_at,
            name: session.name,
            root_url: session.root_url,
            requests: session.requests,
            console_errors: session.console_errors,
            replays: session.replays,
            websocket_frames: session.websocket_frames,
            storage_events: session.storage_events,
            cookie_events: session.cookie_events,
            bodies: session.bodies,
            body_bytes: session.body_bytes,
        })
        .collect();
    let top_repeated_requests = store
        .top_repeated_request_groups(8)
        .context("load top repeated request groups")?
        .into_iter()
        .map(|group| CliRepeatedRequestGroup {
            session_id: group.session_id,
            root_url: group.root_url,
            method: group.method,
            resource_type: group.resource_type,
            domain: domain_for_url(&group.url),
            path: path_for_url(&group.url),
            url: group.url,
            requests: group.requests,
            error_responses: group.error_responses,
            body_bytes: group.body_bytes,
            first_started_at: group.first_started_at,
            last_started_at: group.last_started_at,
        })
        .collect();
    let tables = store
        .table_row_counts()
        .context("load database table row counts")?
        .into_iter()
        .map(|count| CliTableStats {
            table: count.table,
            rows: count.rows,
        })
        .collect();
    Ok(CliDbStats {
        db_path: db_path.display().to_string(),
        db_bytes,
        wal_bytes,
        shm_bytes,
        total_bytes: db_bytes.saturating_add(wal_bytes).saturating_add(shm_bytes),
        body_storage: CliBodyStorageStats {
            bodies: body_storage.bodies,
            total_bytes: body_storage.total_bytes,
            inline_bytes: body_storage.inline_bytes,
            external_bytes: body_storage.external_bytes,
        },
        top_sessions,
        top_repeated_requests,
        tables,
    })
}

fn domain_for_url(value: &str) -> String {
    let without_scheme = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or(value);
    without_scheme
        .split('/')
        .next()
        .filter(|domain| !domain.is_empty())
        .unwrap_or("-")
        .to_string()
}

fn sidecar_path(db_path: &Path, extension: &str) -> PathBuf {
    PathBuf::from(format!("{}-{extension}", db_path.display()))
}

fn file_size(path: &Path) -> anyhow::Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("read metadata for {}", path.display()))?
        .len())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        format!("{:.1}mb", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}kb", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}b")
    }
}
