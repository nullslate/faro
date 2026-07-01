use anyhow::Context;
use faro_core::CookieRecord;
use faro_store::Store;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub(crate) struct CliStorageItem {
    pub(crate) storage_type: String,
    pub(crate) origin: String,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) updated_at: i64,
}

pub(crate) fn open_store(db_path: &PathBuf) -> anyhow::Result<Store> {
    Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))
}

pub(super) fn show_store(db_path: &PathBuf) -> anyhow::Result<()> {
    let store = open_store(db_path)?;
    println!("database {}", db_path.display());
    println!(
        "events {}",
        store.event_count().context("count stored events")?
    );
    for session in store.sessions().context("load sessions")? {
        println!(
            "session {} {}",
            session.id,
            session.root_url.as_deref().unwrap_or("")
        );
        for request in store
            .requests_for_session(&session.id)
            .with_context(|| format!("load requests for session {}", session.id))?
        {
            let response = store
                .latest_response_for_request(&request.id)
                .with_context(|| format!("load responses for request {}", request.id))?;
            let status = response
                .and_then(|response| response.status_code)
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!("  {} {} -> {}", request.method, request.url, status);
        }
    }
    Ok(())
}

pub(crate) fn current_storage_items(
    store: &Store,
    session_id: &str,
) -> anyhow::Result<Vec<CliStorageItem>> {
    let mut items: HashMap<(String, String, String), CliStorageItem> = HashMap::new();
    for snapshot in store
        .storage_snapshots_for_session(session_id)
        .with_context(|| format!("load storage snapshots for session {session_id}"))?
    {
        for entry in snapshot.entries {
            let item = CliStorageItem {
                storage_type: snapshot.storage_type.clone(),
                origin: snapshot.origin.clone(),
                key: entry.key,
                value: entry.value,
                updated_at: snapshot.ts,
            };
            items.insert(
                (
                    item.storage_type.clone(),
                    item.origin.clone(),
                    item.key.clone(),
                ),
                item,
            );
        }
    }
    for event in store
        .storage_events_for_session(session_id)
        .with_context(|| format!("load storage events for session {session_id}"))?
    {
        if event.operation == "clear" {
            items.retain(|(storage_type, origin, _), _| {
                storage_type != &event.storage_type || origin != &event.origin
            });
            continue;
        }
        let Some(key) = event.key else {
            continue;
        };
        let map_key = (
            event.storage_type.clone(),
            event.origin.clone(),
            key.clone(),
        );
        if event.operation == "remove" {
            items.remove(&map_key);
            continue;
        }
        if let Some(value) = event.new_value {
            items.insert(
                map_key,
                CliStorageItem {
                    storage_type: event.storage_type,
                    origin: event.origin,
                    key,
                    value,
                    updated_at: event.ts,
                },
            );
        }
    }
    let mut items = items.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.storage_type
            .cmp(&b.storage_type)
            .then_with(|| a.origin.cmp(&b.origin))
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(items)
}

pub(crate) fn latest_cookies(store: &Store, session_id: &str) -> anyhow::Result<Vec<CookieRecord>> {
    let snapshots = store
        .cookie_snapshots_for_session(session_id)
        .with_context(|| format!("load cookie snapshots for session {session_id}"))?;
    Ok(snapshots
        .into_iter()
        .max_by_key(|snapshot| snapshot.ts)
        .map(|snapshot| snapshot.cookies)
        .unwrap_or_default())
}

pub(super) fn latest_session_url(db_path: &PathBuf) -> anyhow::Result<Option<String>> {
    let store = open_store(db_path)?;
    Ok(store
        .sessions()
        .context("load sessions")?
        .into_iter()
        .rev()
        .find_map(|session| session.root_url))
}
