use crate::Result;
use base64::Engine;
use faro_core::{
    CookieEventRecord, CookieRecord, CookieSnapshotRecord, Run, Session, StorageEntry,
    StorageSnapshotRecord, Tab, cookie_event_observed_event, cookie_observed_event,
    storage_snapshot_created_event,
};
use faro_store::{Store, inline_text_body};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_BODY_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub(super) struct PendingBody {
    pub(super) request_id: String,
    pub(super) mime_type: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PendingStorage {
    pub(super) storage_type: String,
}

pub(super) fn persist_cookie_event(store: &Store, event: CookieEventRecord) -> Result<()> {
    let envelope = cookie_event_observed_event(&event);
    store.insert_cookie_event(&event)?;
    store.append_event(&envelope)?;
    Ok(())
}

pub(super) fn cookie_agent_script() -> &'static str {
    r#"
(() => {
  if (window.__faroCookieAgentInstalled) return;
  window.__faroCookieAgentInstalled = true;
  const descriptor =
    Object.getOwnPropertyDescriptor(Document.prototype, "cookie") ||
    Object.getOwnPropertyDescriptor(HTMLDocument.prototype, "cookie");
  if (!descriptor || !descriptor.configurable || !descriptor.get || !descriptor.set) return;
  Object.defineProperty(Document.prototype, "cookie", {
    configurable: true,
    enumerable: descriptor.enumerable,
    get() {
      return descriptor.get.call(this);
    },
    set(value) {
      try {
        window.faroCookieMutation(JSON.stringify({
          cookie: String(value),
          href: location.href,
          origin: location.origin,
          host: location.hostname,
          ts: Date.now()
        }));
      } catch (_) {}
      return descriptor.set.call(this, value);
    }
  });
})();
"#
}

pub(super) fn persist_body_response(
    store: &Store,
    value: &Value,
    pending: &PendingBody,
) -> Result<()> {
    let Some(result) = value.get("result") else {
        return Ok(());
    };
    let body_text = result
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_base64 = result
        .get("base64Encoded")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if body_text.len() > MAX_BODY_BYTES {
        return Ok(());
    }

    let body = if is_base64 {
        let Some(mime_type) = pending
            .mime_type
            .as_deref()
            .filter(|mime| is_image_mime(mime))
        else {
            return Ok(());
        };
        let decoded_size = base64::engine::general_purpose::STANDARD
            .decode(body_text.as_bytes())
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        if decoded_size > MAX_BODY_BYTES {
            return Ok(());
        }
        inline_text_body(
            Some(mime_type.to_string()),
            format!("data:{mime_type};base64,{body_text}"),
        )
    } else {
        inline_text_body(None, body_text)
    };
    let _ = store.attach_body_to_response_by_browser_request_id(
        &pending.request_id,
        &body,
        body.size as usize >= MAX_BODY_BYTES,
    )?;
    Ok(())
}

pub(super) fn storage_snapshot_expression(storage_type: &str) -> String {
    format!(
        r#"(() => {{
            const storage = window.{storage_type};
            const entries = [];
            for (let index = 0; index < storage.length; index++) {{
                const key = storage.key(index);
                entries.push({{ key, value: storage.getItem(key) }});
            }}
            return {{ origin: location.origin, entries }};
        }})()"#
    )
}

pub(super) fn persist_storage_snapshot(
    store: &Store,
    session: &Session,
    tab: &Tab,
    run: &Run,
    value: &Value,
    pending: PendingStorage,
) -> Result<bool> {
    let Some(result_value) = value
        .get("result")
        .and_then(|result| result.get("result"))
        .and_then(|result| result.get("value"))
    else {
        return Ok(false);
    };

    let origin = result_value
        .get("origin")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let entries = result_value
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(StorageEntry::new(
                        entry.get("key")?.as_str()?.to_string(),
                        entry
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let sha256 = sha256_json(&entries)?;
    let snapshot = StorageSnapshotRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        origin,
        pending.storage_type,
        entries,
        sha256,
    );
    let event = storage_snapshot_created_event(&snapshot);
    store.insert_storage_snapshot(&snapshot)?;
    store.append_event(&event)?;
    Ok(true)
}

pub(super) fn persist_cookie_snapshot(
    store: &Store,
    session: &Session,
    tab: &Tab,
    run: &Run,
    target_url: &str,
    value: &Value,
) -> Result<bool> {
    let cookies = value
        .get("result")
        .and_then(|result| result.get("cookies"))
        .and_then(Value::as_array)
        .map(|cookies| cookies.iter().map(parse_cookie).collect::<Vec<_>>())
        .unwrap_or_default();

    let snapshot = CookieSnapshotRecord::new(
        session.id.clone(),
        Some(tab.id.clone()),
        Some(run.id.clone()),
        Some(target_url.to_string()),
        cookies,
    );
    let event = cookie_observed_event(&snapshot);
    store.insert_cookie_snapshot(&snapshot)?;
    store.append_event(&event)?;
    Ok(true)
}

fn is_image_mime(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
}

fn parse_cookie(value: &Value) -> CookieRecord {
    CookieRecord {
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value: value
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        domain: value
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        path: value
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        expires: value.get("expires").and_then(Value::as_f64),
        http_only: value
            .get("httpOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        secure: value
            .get("secure")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        same_site: value
            .get("sameSite")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn sha256_json<T: serde::Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_vec(value)?;
    let digest = Sha256::digest(&json);
    Ok(digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>())
}
