use crate::services::latest_session;
use anyhow::{Context, bail};
use faro_core::ConsoleLevel;
use std::path::PathBuf;

use super::super::output::{compact_line, print_json};
use super::super::store::{current_storage_items, latest_cookies, open_store};

pub(crate) fn handle_console(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    if args.first().map(String::as_str) != Some("errors") {
        bail!("usage: faro console errors [--json]");
    }
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown console errors option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no faro sessions found");
    };
    let logs = store
        .console_logs_for_session(&session.id)
        .with_context(|| format!("load console logs for session {}", session.id))?
        .into_iter()
        .filter(|log| matches!(log.level, ConsoleLevel::Error | ConsoleLevel::Fatal))
        .collect::<Vec<_>>();

    if json_output {
        print_json(&logs)?;
    } else {
        for log in logs {
            println!(
                "{} {} {}",
                log.id,
                log.level.as_str(),
                compact_line(&log.message)
            );
        }
    }
    Ok(())
}

pub(crate) fn handle_storage(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    if args.first().map(String::as_str) != Some("get") {
        bail!("usage: faro storage get <localStorage|sessionStorage> <key> [--json]");
    }
    args.remove(0);
    let Some(storage_type) = args.first().cloned() else {
        bail!("usage: faro storage get <localStorage|sessionStorage> <key> [--json]");
    };
    args.remove(0);
    let Some(key) = args.first().cloned() else {
        bail!("usage: faro storage get <localStorage|sessionStorage> <key> [--json]");
    };
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown storage get option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no faro sessions found");
    };
    let items = current_storage_items(&store, &session.id)?
        .into_iter()
        .filter(|item| item.storage_type == storage_type && item.key == key)
        .collect::<Vec<_>>();

    if json_output {
        print_json(&items)?;
    } else {
        for item in items {
            println!("{} {}={}", item.origin, item.key, item.value);
        }
    }
    Ok(())
}

pub(crate) fn handle_cookies(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    if args.first().map(String::as_str) != Some("list") {
        bail!("usage: faro cookies list [--json]");
    }
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown cookies list option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no faro sessions found");
    };
    let cookies = latest_cookies(&store, &session.id)?;
    if json_output {
        print_json(&cookies)?;
    } else {
        for cookie in cookies {
            println!(
                "{}{} {}={}",
                cookie.domain, cookie.path, cookie.name, cookie.value
            );
        }
    }
    Ok(())
}
