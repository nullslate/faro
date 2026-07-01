use crate::config;
use crate::config::AppConfig;
use crate::{mcp, tui};
use anyhow::{Context, bail};
use faro_cdp::CaptureOptions;
use std::path::PathBuf;

mod capture;
mod commands;
mod options;
mod output;
mod store;

#[cfg(test)]
mod tests;

pub(crate) use options::{CliOptions, parse_duration};
pub(crate) use store::{current_storage_items, latest_cookies, open_store};

use capture::handle_capture;
use commands::{
    handle_console, handle_cookies, handle_db, handle_replay, handle_request, handle_requests,
    handle_sessions, handle_sql, handle_storage,
};
use options::parse_args;
use output::print_help;
use store::{latest_session_url, show_store};

pub(crate) async fn run() -> anyhow::Result<()> {
    let app_config = config::load_or_create().context("load faro config")?;
    let (options, mut args) = parse_args(std::env::args().skip(1).collect(), &app_config)
        .context("parse command line arguments")?;
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_help();
        return Ok(());
    }

    match args.remove(0).as_str() {
        "mcp" => mcp::run(options),
        "capture" => handle_capture(options, args),
        "requests" => handle_requests(&options.db_path, args),
        "request" => handle_request(&options.db_path, args),
        "console" => handle_console(&options.db_path, args),
        "storage" => handle_storage(&options.db_path, args),
        "cookies" => handle_cookies(&options.db_path, args),
        "sessions" => handle_sessions(&options.db_path, args),
        "db" => handle_db(&options.db_path, args),
        "replay" => handle_replay(&options.db_path, args),
        "sql" => handle_sql(&options.db_path, args),
        "show" => {
            let db_path = args.first().map(PathBuf::from).unwrap_or(options.db_path);
            show_store(&db_path)
        }
        "tui" => run_offline_tui(options.db_path, args, app_config),
        url if url.starts_with("http://") || url.starts_with("https://") => {
            run_capture_tui(options, url, app_config)
        }
        command => {
            print_help();
            bail!("unknown command or URL: {command}")
        }
    }
}

fn run_offline_tui(
    default_db_path: PathBuf,
    args: Vec<String>,
    app_config: AppConfig,
) -> anyhow::Result<()> {
    let db_path = args.first().map(PathBuf::from).unwrap_or(default_db_path);
    let target = latest_session_url(&db_path)
        .with_context(|| format!("load latest session URL from {}", db_path.display()))?
        .unwrap_or_else(|| "offline".to_string());
    tui::run(&db_path, &target, tui::RunConfig::offline(), app_config)
}

fn run_capture_tui(options: CliOptions, url: &str, app_config: AppConfig) -> anyhow::Result<()> {
    let capture_options = CaptureOptions {
        db_path: options.db_path.clone(),
        url: url.to_string(),
        attach_port: options.attach_port,
        launch_port: options.launch_port,
        max_requests_per_session: app_config.retention.max_requests_per_session,
        max_repeated_requests_per_url: app_config.retention.max_repeated_requests_per_url,
        prune_interval_requests: app_config.retention.prune_interval_requests,
    };
    let run_config = if options.attach_port.is_some() || options.launch_on_start {
        tui::RunConfig::capturing(faro_cdp::spawn_capture(capture_options))
    } else {
        tui::RunConfig::lazy(capture_options)
    };
    tui::run(&options.db_path, url, run_config, app_config)
}
