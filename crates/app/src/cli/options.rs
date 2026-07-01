use crate::config::{AppConfig, RedactionConfig};
use anyhow::{Context, bail};
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct CliOptions {
    pub(crate) db_path: PathBuf,
    pub(crate) attach_port: Option<u16>,
    pub(crate) launch_port: Option<u16>,
    pub(crate) launch_on_start: bool,
    pub(crate) max_requests_per_session: usize,
    pub(crate) max_repeated_requests_per_url: usize,
    pub(crate) prune_interval_requests: usize,
    pub(crate) mcp_allow_mutation: bool,
    pub(crate) mcp_allow_sensitive: bool,
    pub(crate) redaction: RedactionConfig,
}

pub(super) fn parse_args(
    args: Vec<String>,
    app_config: &AppConfig,
) -> anyhow::Result<(CliOptions, Vec<String>)> {
    let mut options = CliOptions {
        db_path: app_config.db_path.clone(),
        attach_port: None,
        launch_port: None,
        launch_on_start: app_config.launch_on_start,
        max_requests_per_session: app_config.retention.max_requests_per_session,
        max_repeated_requests_per_url: app_config.retention.max_repeated_requests_per_url,
        prune_interval_requests: app_config.retention.prune_interval_requests,
        mcp_allow_mutation: false,
        mcp_allow_sensitive: false,
        redaction: app_config.redaction.clone(),
    };
    let mut parsed = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--db" => {
                let Some(path) = iter.next() else {
                    bail!("usage: faro [--db <db-path>] <http-url>");
                };
                options.db_path = PathBuf::from(path);
            }
            "--attach-port" => {
                let Some(port) = iter.next() else {
                    bail!("usage: faro [--attach-port <port>] <http-url>");
                };
                options.attach_port = Some(
                    port.parse()
                        .with_context(|| format!("parse --attach-port value `{port}`"))?,
                );
            }
            "--cdp-port" => {
                let Some(port) = iter.next() else {
                    bail!("usage: faro [--cdp-port <port>] <http-url>");
                };
                options.launch_port = Some(
                    port.parse()
                        .with_context(|| format!("parse --cdp-port value `{port}`"))?,
                );
            }
            "--launch-on-start" => {
                options.launch_on_start = true;
            }
            "--mcp-allow-mutation" => {
                options.mcp_allow_mutation = true;
            }
            "--mcp-allow-sensitive" => {
                options.mcp_allow_sensitive = true;
            }
            _ => parsed.push(arg),
        }
    }
    if options.attach_port.is_some() && options.launch_port.is_some() {
        bail!("--attach-port and --cdp-port cannot be used together");
    }
    Ok((options, parsed))
}

pub(crate) fn parse_duration(value: &str) -> anyhow::Result<Duration> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("duration cannot be empty");
    }
    let (number, unit) = trimmed
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit())
        .map(|(index, _)| trimmed.split_at(index))
        .unwrap_or((trimmed, "s"));
    let amount = number
        .parse::<u64>()
        .with_context(|| format!("parse duration `{value}`"))?;
    match unit {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => Ok(Duration::from_secs(amount)),
        "ms" | "millisecond" | "milliseconds" => Ok(Duration::from_millis(amount)),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(Duration::from_secs(amount * 60)),
        _ => bail!("unsupported duration unit `{unit}`; use ms, s, or m"),
    }
}
