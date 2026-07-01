use super::request_filter::request_matches_filter;
use super::routes::request_matches_route;
use anyhow::Context;
use faro_core::{RequestRecord, ResponseRecord};
use faro_store::Store;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RequestRow {
    pub(crate) id: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) status_code: Option<i64>,
    pub(crate) resource_type: Option<String>,
    pub(crate) started_at: i64,
    pub(crate) completed_at: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) body_size: Option<i64>,
    pub(crate) mime_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RequestListQuery {
    pub(crate) filter: Option<String>,
    pub(crate) route: Option<String>,
    pub(crate) limit: Option<usize>,
}

pub(crate) fn request_rows_for_session(
    store: &Store,
    session_id: &str,
) -> anyhow::Result<Vec<RequestRow>> {
    let responses = latest_responses_by_request(store, session_id)?;
    store
        .requests_for_session(session_id)
        .with_context(|| format!("load requests for session {session_id}"))?
        .into_iter()
        .map(|request| {
            let response = responses.get(&request.id);
            Ok(request_row(&request, response))
        })
        .collect()
}

pub(crate) fn list_request_rows(
    store: &Store,
    session_id: &str,
    query: &RequestListQuery,
) -> anyhow::Result<Vec<RequestRow>> {
    let mut rows = request_rows_for_session(store, session_id)?
        .into_iter()
        .filter(|row| {
            query
                .filter
                .as_deref()
                .map(|filter| request_matches_filter(row, filter))
                .unwrap_or(true)
        })
        .filter(|row| {
            query
                .route
                .as_deref()
                .map(|route| request_matches_route(&row.url, route))
                .unwrap_or(true)
        });

    match query.limit {
        Some(limit) => Ok(rows.by_ref().take(limit).collect()),
        None => Ok(rows.collect()),
    }
}

pub(crate) fn latest_responses_by_request(
    store: &Store,
    session_id: &str,
) -> anyhow::Result<HashMap<String, ResponseRecord>> {
    let mut responses = HashMap::new();
    for response in store
        .responses_for_session(session_id)
        .with_context(|| format!("load responses for session {session_id}"))?
    {
        responses.insert(response.request_id.clone(), response);
    }
    Ok(responses)
}

pub(crate) fn request_row(
    request: &RequestRecord,
    response: Option<&ResponseRecord>,
) -> RequestRow {
    RequestRow {
        id: request.id.clone(),
        method: request.method.clone(),
        url: request.url.clone(),
        status_code: response.and_then(|response| response.status_code),
        resource_type: request.resource_type.clone(),
        started_at: request.started_at,
        completed_at: request.completed_at,
        duration_ms: request
            .completed_at
            .map(|completed_at| completed_at.saturating_sub(request.started_at)),
        body_size: response.and_then(|response| response.body_size),
        mime_type: response.and_then(|response| response.mime_type.clone()),
    }
}
