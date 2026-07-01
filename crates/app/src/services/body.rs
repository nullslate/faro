use anyhow::Context;
use faro_core::BodyRecord;
use faro_store::Store;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct BodyText {
    pub(crate) id: String,
    pub(crate) content_type: Option<String>,
    pub(crate) encoding: String,
    pub(crate) size: i64,
    pub(crate) text: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct LimitedBodyText {
    pub(crate) id: String,
    pub(crate) content_type: Option<String>,
    pub(crate) encoding: String,
    pub(crate) size: i64,
    pub(crate) text: String,
    pub(crate) truncated: bool,
    pub(crate) returned_bytes: usize,
    pub(crate) limit_bytes: usize,
}

pub(crate) fn load_body(store: &Store, body_id: Option<&str>) -> anyhow::Result<Option<BodyText>> {
    let Some(body_id) = body_id else {
        return Ok(None);
    };
    let Some(body) = store
        .response_body(body_id)
        .with_context(|| format!("load body {body_id}"))?
    else {
        return Ok(None);
    };
    Ok(Some(body_text(body)))
}

pub(crate) fn load_body_text(
    store: &Store,
    body_id: Option<&str>,
) -> anyhow::Result<Option<String>> {
    Ok(load_body(store, body_id)?.map(|body| body.text))
}

pub(crate) fn limited_body(
    store: &Store,
    body_id: Option<&str>,
    limit_bytes: usize,
) -> anyhow::Result<Option<LimitedBodyText>> {
    let Some(mut body) = load_body(store, body_id)? else {
        return Ok(None);
    };
    let original_len = body.text.len();
    let truncated = original_len > limit_bytes;
    if truncated {
        let end = body
            .text
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= limit_bytes)
            .last()
            .unwrap_or(0);
        body.text.truncate(end);
    }
    Ok(Some(LimitedBodyText {
        id: body.id,
        content_type: body.content_type,
        encoding: body.encoding,
        size: body.size,
        returned_bytes: body.text.len(),
        text: body.text,
        truncated,
        limit_bytes,
    }))
}

fn body_text(body: BodyRecord) -> BodyText {
    BodyText {
        id: body.id,
        content_type: body.content_type,
        encoding: body.encoding,
        size: body.size,
        text: String::from_utf8_lossy(&body.data).to_string(),
    }
}
