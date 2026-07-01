use crate::config::RedactionConfig;
use faro_core::{Header, RequestRecord};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct CurlCommand {
    pub(crate) request_id: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ShareableCurlCommand {
    pub(crate) request_id: String,
    pub(crate) redacted: bool,
    pub(crate) body_included: bool,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
}

pub(crate) fn build_curl_args(request: &RequestRecord, request_body: Option<&str>) -> Vec<String> {
    let mut parts = vec![
        "-sS".to_string(),
        "-i".to_string(),
        "--compressed".to_string(),
        "-X".to_string(),
        request.method.clone(),
        request.url.clone(),
    ];
    for header in &request.request_headers {
        push_header_arg(&mut parts, header);
    }
    if let Some(body) = request_body {
        parts.push("--data-raw".to_string());
        parts.push(body.to_string());
    }
    parts
}

pub(crate) fn build_curl_command(args: &[String]) -> String {
    format!(
        "curl {}",
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

pub(crate) fn build_curl_argv(args: &[String]) -> Vec<String> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push("curl".to_string());
    argv.extend(args.iter().cloned());
    argv
}

pub(crate) fn redact_headers(request: &mut RequestRecord, redaction: &RedactionConfig) {
    request.request_headers = request
        .request_headers
        .drain(..)
        .map(|header| redacted_header(header, redaction))
        .collect();
}

fn push_header_arg(parts: &mut Vec<String>, header: &Header) {
    parts.push("-H".to_string());
    parts.push(format!("{}: {}", header.name, header.value));
}

fn redacted_header(header: Header, redaction: &RedactionConfig) -> Header {
    if is_sensitive_header_name(&header.name, redaction) {
        Header::new(header.name, "[redacted]")
    } else {
        header
    }
}

fn is_sensitive_header_name(name: &str, redaction: &RedactionConfig) -> bool {
    let lower = name.to_ascii_lowercase();
    redaction.header_names.contains(&lower)
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
