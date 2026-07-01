use anyhow::Context;
use faro_core::Session;
use faro_store::Store;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct SessionSummary {
    pub(crate) session: Session,
    pub(crate) request_count: usize,
    pub(crate) console_error_count: usize,
    pub(crate) replay_count: usize,
    pub(crate) websocket_count: usize,
    pub(crate) storage_count: usize,
    pub(crate) cookie_count: usize,
}

pub(crate) fn latest_session(store: &Store) -> anyhow::Result<Option<Session>> {
    Ok(store
        .sessions()
        .context("load sessions")?
        .into_iter()
        .max_by_key(|session| session.created_at))
}

pub(crate) fn session_summaries(store: &Store) -> anyhow::Result<Vec<SessionSummary>> {
    store
        .sessions()
        .context("load sessions")?
        .into_iter()
        .map(|session| session_summary(store, session))
        .collect()
}

pub(crate) fn session_summary(store: &Store, session: Session) -> anyhow::Result<SessionSummary> {
    let counts = store
        .session_summary_counts(&session.id)
        .with_context(|| format!("load session summary for {}", session.id))?;
    Ok(SessionSummary {
        session,
        request_count: counts.requests,
        console_error_count: counts.console_errors,
        replay_count: counts.replays,
        websocket_count: counts.websocket_frames,
        storage_count: counts.storage_events,
        cookie_count: counts.cookie_events,
    })
}
