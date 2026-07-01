use super::editor::write_temp_file;
use super::state::{
    DetailTab, FocusPane, RequestView, WorkbenchState, formatted_request_body,
    formatted_response_body,
};
use super::util::{append_audit_event, command_exists};
use crate::config::RedactionConfig;
use anyhow::Context;
use std::io::Write;
use std::process::{Command, Stdio};

pub(super) fn save_selected_exchange(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    if app.detail_tab == DetailTab::Replay
        && app.focus == FocusPane::Detail
        && let Some(replay) = app.selected_replay_export_text()
    {
        match write_temp_file("faro-replay", "txt", &replay) {
            Ok(path) => app.status = format!("saved replay {}", path.display()),
            Err(error) => app.status = format!("save replay failed: {error}"),
        }
        return;
    }
    let Some(request) = app.selected_request() else {
        app.status = "no request selected".to_string();
        return;
    };
    let exchange = format_exchange(request);
    match write_temp_file("faro-exchange", "http", &exchange) {
        Ok(path) => app.status = format!("saved exchange {}", path.display()),
        Err(error) => app.status = format!("save failed: {error}"),
    }
}

pub(super) fn copy_curl(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    let Some(curl) = app.copy_curl_text() else {
        app.status = "no request selected".to_string();
        return;
    };

    match copy_to_clipboard(&curl) {
        Ok(tool) => app.status = format!("copied full request as curl with {tool}"),
        Err(error) => match write_temp_file("faro-curl", "sh", &curl) {
            Ok(path) => {
                app.status = format!("clipboard unavailable ({error}); wrote {}", path.display())
            }
            Err(write_error) => {
                app.status =
                    format!("clipboard unavailable ({error}); temp write failed: {write_error}")
            }
        },
    }
    append_audit_event(
        "tui.copy_curl_raw",
        serde_json::json!({ "target_url": app.target_url }),
    );
}

pub(super) fn copy_body(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    let Some(text) = app.copy_body_text() else {
        app.status = "no response body selected".to_string();
        return;
    };
    match copy_to_clipboard(&text) {
        Ok(tool) => app.status = format!("copied body selection with {tool}"),
        Err(error) => match write_temp_file("faro-body", "txt", &text) {
            Ok(path) => {
                app.status = format!("clipboard unavailable ({error}); wrote {}", path.display())
            }
            Err(write_error) => {
                app.status =
                    format!("clipboard unavailable ({error}); temp write failed: {write_error}")
            }
        },
    }
    append_audit_event(
        "tui.copy_body_raw",
        serde_json::json!({ "target_url": app.target_url }),
    );
}

pub(super) fn copy_share_bundle(app: &mut WorkbenchState) {
    app.hydrate_selected_request();
    let Some(request) = app.selected_request() else {
        app.status = "no request selected".to_string();
        return;
    };
    let bundle = format_share_bundle(request, &app.config.redaction);
    match copy_to_clipboard(&bundle) {
        Ok(tool) => app.status = format!("copied redacted share bundle with {tool}"),
        Err(error) => match write_temp_file("faro-share", "md", &bundle) {
            Ok(path) => {
                app.status = format!("clipboard unavailable ({error}); wrote {}", path.display())
            }
            Err(write_error) => {
                app.status =
                    format!("clipboard unavailable ({error}); temp write failed: {write_error}")
            }
        },
    }
    append_audit_event(
        "tui.copy_share_bundle_redacted",
        serde_json::json!({ "target_url": app.target_url }),
    );
}

fn format_exchange(request: &RequestView) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "{} {}\n",
        request.request.method, request.request.url
    ));
    for header in &request.request.request_headers {
        text.push_str(&format!("{}: {}\n", header.name, header.value));
    }
    text.push('\n');

    if request.request_body.is_some() {
        text.push_str(&formatted_request_body(request));
        text.push('\n');
    }

    text.push_str("\n### response\n");
    if let Some(response) = &request.response {
        text.push_str(&format!(
            "HTTP {}\n",
            response
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        for header in &response.response_headers {
            text.push_str(&format!("{}: {}\n", header.name, header.value));
        }
        text.push('\n');
        if request.response_body.is_some() {
            text.push_str(&formatted_response_body(request));
            text.push('\n');
        }
    } else {
        text.push_str("No response captured yet.\n");
    }

    text
}

fn format_share_bundle(request: &RequestView, redaction: &RedactionConfig) -> String {
    let mut text = String::new();
    text.push_str("# Faro request bundle\n\n");
    text.push_str("## Request\n\n");
    text.push_str(&format!("- id: `{}`\n", request.request.id));
    text.push_str(&format!("- method: `{}`\n", request.request.method));
    text.push_str(&format!("- url: `{}`\n", request.request.url));
    text.push_str(&format!(
        "- type: `{}`\n",
        request.request.resource_type.as_deref().unwrap_or("-")
    ));
    if let Some(duration) = request.duration_ms() {
        text.push_str(&format!("- duration: `{duration}ms`\n"));
    }
    text.push_str("\n### Request headers\n\n```http\n");
    for header in &request.request.request_headers {
        text.push_str(&format!(
            "{}: {}\n",
            header.name,
            redacted_header_value(&header.name, &header.value, redaction)
        ));
    }
    text.push_str("```\n\n");
    if let Some(body) = request.request_body.as_deref() {
        text.push_str("### Request body\n\n```text\n");
        text.push_str(&redacted_body_preview(body, redaction));
        text.push_str("\n```\n\n");
    }

    text.push_str("## Response\n\n");
    if let Some(response) = &request.response {
        text.push_str(&format!(
            "- status: `{}`\n",
            response
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        text.push_str(&format!(
            "- mime: `{}`\n",
            response.mime_type.as_deref().unwrap_or("-")
        ));
        text.push_str(&format!(
            "- body size: `{}`\n",
            response
                .body_size
                .map(share_format_bytes)
                .unwrap_or_else(|| "-".to_string())
        ));
        text.push_str("\n### Response headers\n\n```http\n");
        for header in &response.response_headers {
            text.push_str(&format!(
                "{}: {}\n",
                header.name,
                redacted_header_value(&header.name, &header.value, redaction)
            ));
        }
        text.push_str("```\n\n");
        if let Some(body) = request.response_body.as_deref() {
            text.push_str("### Response body preview\n\n```text\n");
            text.push_str(&redacted_body_preview(body, redaction));
            text.push_str("\n```\n\n");
        }
    } else {
        text.push_str("No response captured.\n\n");
    }

    if !request.replays.is_empty() {
        text.push_str("## Replays\n\n");
        for replay in request.replays.iter().rev().take(5) {
            text.push_str(&format!(
                "- `{}` status={} exit={} ts={}\n",
                replay.record.id,
                replay
                    .record
                    .status_code
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                replay
                    .record
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                replay.record.ts
            ));
        }
    }

    text
}

fn redacted_header_value(name: &str, value: &str, redaction: &RedactionConfig) -> String {
    let lower = name.to_ascii_lowercase();
    if redaction.header_names.contains(&lower) {
        return "[redacted]".to_string();
    }
    compact_text(value, 1_000)
}

fn redacted_body_preview(body: &str, redaction: &RedactionConfig) -> String {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(body) {
        redact_json_value(&mut value, redaction);
        if let Ok(pretty) = serde_json::to_string_pretty(&value) {
            return compact_text(&pretty, 4_000);
        }
    }
    compact_text(&redact_sensitive_text(body, redaction), 4_000)
}

fn redact_json_value(value: &mut serde_json::Value, redaction: &RedactionConfig) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if is_sensitive_key(key, redaction) {
                    *value = serde_json::Value::String("[redacted]".to_string());
                } else {
                    redact_json_value(value, redaction);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value, redaction);
            }
        }
        serde_json::Value::String(value) => {
            *value = redact_sensitive_text(value, redaction);
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str, redaction: &RedactionConfig) -> bool {
    let lower = key.to_ascii_lowercase();
    redaction
        .json_key_patterns
        .iter()
        .any(|marker| lower.contains(marker))
}

fn redact_sensitive_text(value: &str, redaction: &RedactionConfig) -> String {
    if value.to_ascii_lowercase().starts_with("bearer ") {
        return "Bearer [redacted]".to_string();
    }
    value
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if redaction
                .text_patterns
                .iter()
                .any(|pattern| lower.starts_with(pattern))
                || looks_like_email(part)
                || looks_like_jwt(part)
            {
                "[redacted]".to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_email(value: &str) -> bool {
    let trimmed = value.trim_matches(|character: char| {
        matches!(
            character,
            '"' | '\'' | ',' | ';' | ':' | '<' | '>' | '(' | ')'
        )
    });
    let Some((local, domain)) = trimmed.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
}

fn looks_like_jwt(value: &str) -> bool {
    let trimmed = value.trim_matches(|character: char| {
        matches!(
            character,
            '"' | '\'' | ',' | ';' | ':' | '<' | '>' | '(' | ')'
        )
    });
    let mut parts = trimmed.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c), None)
            if a.len() >= 8 && b.len() >= 8 && c.len() >= 8
    )
}

fn compact_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut compact = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    compact.push_str("...");
    compact
}

fn share_format_bytes(bytes: i64) -> String {
    let abs = bytes.unsigned_abs() as f64;
    if abs >= 1024.0 * 1024.0 {
        format!("{:.1}mb", abs / 1024.0 / 1024.0)
    } else if abs >= 1024.0 {
        format!("{:.1}kb", abs / 1024.0)
    } else {
        format!("{bytes}b")
    }
}

struct ClipboardTool {
    label: &'static str,
    command: &'static str,
    args: &'static [&'static str],
}

#[cfg(target_os = "macos")]
const CLIPBOARD_TOOLS: &[ClipboardTool] = &[
    ClipboardTool {
        label: "pbcopy",
        command: "pbcopy",
        args: &[],
    },
    ClipboardTool {
        label: "wl-copy",
        command: "wl-copy",
        args: &[],
    },
    ClipboardTool {
        label: "xclip",
        command: "xclip",
        args: &["-selection", "clipboard"],
    },
    ClipboardTool {
        label: "xsel",
        command: "xsel",
        args: &["--clipboard", "--input"],
    },
];

#[cfg(target_os = "windows")]
const CLIPBOARD_TOOLS: &[ClipboardTool] = &[
    ClipboardTool {
        label: "clip.exe",
        command: "clip.exe",
        args: &[],
    },
    ClipboardTool {
        label: "powershell Set-Clipboard",
        command: "powershell.exe",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
    ClipboardTool {
        label: "pwsh Set-Clipboard",
        command: "pwsh.exe",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
];

#[cfg(all(unix, not(target_os = "macos")))]
const CLIPBOARD_TOOLS: &[ClipboardTool] = &[
    ClipboardTool {
        label: "wl-copy",
        command: "wl-copy",
        args: &[],
    },
    ClipboardTool {
        label: "xclip",
        command: "xclip",
        args: &["-selection", "clipboard"],
    },
    ClipboardTool {
        label: "xsel",
        command: "xsel",
        args: &["--clipboard", "--input"],
    },
    ClipboardTool {
        label: "clip.exe",
        command: "clip.exe",
        args: &[],
    },
    ClipboardTool {
        label: "powershell Set-Clipboard",
        command: "powershell.exe",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
    ClipboardTool {
        label: "pwsh Set-Clipboard",
        command: "pwsh",
        args: &["-NoProfile", "-Command", "Set-Clipboard"],
    },
];

fn copy_to_clipboard(text: &str) -> anyhow::Result<&'static str> {
    for tool in CLIPBOARD_TOOLS {
        if command_exists(tool.command) {
            let mut child = Command::new(tool.command)
                .args(tool.args)
                .stdin(Stdio::piped())
                .spawn()?;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin
                    .write_all(text.as_bytes())
                    .context("write text to clipboard command stdin")?;
            }
            let status = child.wait().context("wait for clipboard command")?;
            if status.success() {
                return Ok(tool.label);
            }
        }
    }
    anyhow::bail!("no supported clipboard command found or all clipboard commands failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::{Header, RequestRecord, ResponseRecord};

    fn request_view_with_sensitive_headers() -> RequestView {
        let mut request = RequestRecord::started(
            "session".to_string(),
            Some("tab".to_string()),
            Some("run".to_string()),
            "POST",
            "https://example.test/api/login",
        );
        request
            .request_headers
            .push(Header::new("authorization", "Bearer secret-token"));
        request
            .request_headers
            .push(Header::new("content-type", "application/json"));

        let mut response = ResponseRecord::received(request.id.clone());
        response.status_code = Some(401);
        response.mime_type = Some("application/json".to_string());
        response
            .response_headers
            .push(Header::new("set-cookie", "sid=secret"));

        RequestView {
            request,
            response: Some(response),
            request_body: Some(
                r#"{"email":"test@example.com","password":"secret-password","profile":{"apiToken":"secret-token"}}"#
                    .to_string(),
            ),
            response_body: Some(
                r#"{"error":"unauthorized","jwt":"abcdefgh.ijklmnop.qrstuvwx"}"#.to_string(),
            ),
            replays: Vec::new(),
            details_loaded: true,
        }
    }

    #[test]
    fn share_bundle_redacts_sensitive_headers() {
        let bundle = format_share_bundle(
            &request_view_with_sensitive_headers(),
            &RedactionConfig::default(),
        );

        assert!(bundle.contains("authorization: [redacted]"));
        assert!(bundle.contains("set-cookie: [redacted]"));
        assert!(!bundle.contains("secret-token"));
        assert!(!bundle.contains("secret-password"));
        assert!(!bundle.contains("test@example.com"));
        assert!(!bundle.contains("abcdefgh.ijklmnop.qrstuvwx"));
        assert!(!bundle.contains("sid=secret"));
        assert!(bundle.contains("\"email\": \"[redacted]\""));
        assert!(bundle.contains("\"password\": \"[redacted]\""));
        assert!(bundle.contains("\"apiToken\": \"[redacted]\""));
        assert!(bundle.contains("content-type: application/json"));
    }
}
