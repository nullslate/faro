use super::body::{BodyText, LimitedBodyText, limited_body, load_body, load_body_text};
use super::curl::{
    CurlCommand, ShareableCurlCommand, build_curl_args, build_curl_argv, build_curl_command,
    redact_headers,
};
use crate::config::RedactionConfig;
use anyhow::Context;
use faro_core::{RequestRecord, ResponseRecord};
use faro_store::Store;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct RequestDetail {
    pub(crate) request: RequestRecord,
    pub(crate) response: Option<ResponseRecord>,
    pub(crate) request_body: Option<BodyText>,
    pub(crate) response_body: Option<BodyText>,
}

pub(crate) fn request_detail(
    store: &Store,
    request_id: &str,
    include_body: bool,
) -> anyhow::Result<RequestDetail> {
    let (request, response) = request_with_latest_response(store, request_id)?;
    let request_body = if include_body {
        load_body(store, request.request_body_ref.as_deref())?
    } else {
        None
    };
    let response_body = if include_body {
        load_body(
            store,
            response
                .as_ref()
                .and_then(|response| response.body_ref.as_deref()),
        )?
    } else {
        None
    };

    Ok(RequestDetail {
        request,
        response,
        request_body,
        response_body,
    })
}

pub(crate) fn request_with_latest_response(
    store: &Store,
    request_id: &str,
) -> anyhow::Result<(RequestRecord, Option<ResponseRecord>)> {
    let request = store
        .request_by_id(request_id)
        .with_context(|| format!("load request {request_id}"))?
        .with_context(|| format!("request not found: {request_id}"))?;
    let response = store
        .latest_response_for_request(request_id)
        .with_context(|| format!("load latest response for request {request_id}"))?;
    Ok((request, response))
}

pub(crate) fn response_body_for_request(
    store: &Store,
    request_id: &str,
    limit_bytes: usize,
) -> anyhow::Result<Option<LimitedBodyText>> {
    let (_, response) = request_with_latest_response(store, request_id)?;
    limited_body(
        store,
        response
            .as_ref()
            .and_then(|response| response.body_ref.as_deref()),
        limit_bytes,
    )
}

pub(crate) fn request_curl_command(store: &Store, request_id: &str) -> anyhow::Result<CurlCommand> {
    let (request, _) = request_with_latest_response(store, request_id)?;
    let request_body = load_body_text(store, request.request_body_ref.as_deref())?;
    let args = build_curl_args(&request, request_body.as_deref());
    Ok(CurlCommand {
        request_id: request.id,
        command: build_curl_command(&args),
        args: build_curl_argv(&args),
    })
}

pub(crate) fn shareable_curl_command(
    store: &Store,
    request_id: &str,
    include_sensitive: bool,
    redaction: &RedactionConfig,
) -> anyhow::Result<ShareableCurlCommand> {
    let (mut request, _) = request_with_latest_response(store, request_id)?;
    let request_body = if include_sensitive {
        load_body_text(store, request.request_body_ref.as_deref())?
    } else {
        redact_headers(&mut request, redaction);
        None
    };
    let args = build_curl_args(&request, request_body.as_deref());
    Ok(ShareableCurlCommand {
        request_id: request.id,
        redacted: !include_sensitive,
        body_included: request_body.is_some(),
        command: build_curl_command(&args),
        args: build_curl_argv(&args),
    })
}
