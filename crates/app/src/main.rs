mod config;
mod tui;

use anyhow::{Context, bail};
use config::AppConfig;
use devbench_cdp::CaptureOptions;
use devbench_store::Store;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app_config = config::load_or_create().context("load devbench config")?;
    let (options, mut args) = parse_args(std::env::args().skip(1).collect(), &app_config)
        .context("parse command line arguments")?;
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_help();
        return Ok(());
    }

    match args.remove(0).as_str() {
        "show" => {
            let db_path = args.first().map(PathBuf::from).unwrap_or(options.db_path);
            show_store(&db_path)
        }
        "tui" => {
            let db_path = args.first().map(PathBuf::from).unwrap_or(options.db_path);
            let target = latest_session_url(&db_path)
                .with_context(|| format!("load latest session URL from {}", db_path.display()))?
                .unwrap_or_else(|| "offline".to_string());
            tui::run(&db_path, &target, tui::RunConfig::offline(), app_config)
        }
        url if url.starts_with("http://") || url.starts_with("https://") => {
            let capture_options = CaptureOptions {
                db_path: options.db_path.clone(),
                url: url.to_string(),
                attach_port: options.attach_port,
                launch_port: options.launch_port,
            };
            let run_config = if options.attach_port.is_some() || options.launch_on_start {
                tui::RunConfig::capturing(devbench_cdp::spawn_capture(capture_options))
            } else {
                tui::RunConfig::lazy(capture_options)
            };
            tui::run(&options.db_path, url, run_config, app_config)
        }
        command => {
            print_help();
            bail!("unknown command or URL: {command}")
        }
    }
}

struct CliOptions {
    db_path: PathBuf,
    attach_port: Option<u16>,
    launch_port: Option<u16>,
    launch_on_start: bool,
}

fn parse_args(
    args: Vec<String>,
    app_config: &AppConfig,
) -> anyhow::Result<(CliOptions, Vec<String>)> {
    let mut options = CliOptions {
        db_path: app_config.db_path.clone(),
        attach_port: None,
        launch_port: None,
        launch_on_start: app_config.launch_on_start,
    };
    let mut parsed = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--db" => {
                let Some(path) = iter.next() else {
                    bail!("usage: devbench [--db <db-path>] <http-url>");
                };
                options.db_path = PathBuf::from(path);
            }
            "--attach-port" => {
                let Some(port) = iter.next() else {
                    bail!("usage: devbench [--attach-port <port>] <http-url>");
                };
                options.attach_port = Some(
                    port.parse()
                        .with_context(|| format!("parse --attach-port value `{port}`"))?,
                );
            }
            "--cdp-port" => {
                let Some(port) = iter.next() else {
                    bail!("usage: devbench [--cdp-port <port>] <http-url>");
                };
                options.launch_port = Some(
                    port.parse()
                        .with_context(|| format!("parse --cdp-port value `{port}`"))?,
                );
            }
            "--launch-on-start" => {
                options.launch_on_start = true;
            }
            _ => parsed.push(arg),
        }
    }
    if options.attach_port.is_some() && options.launch_port.is_some() {
        bail!("--attach-port and --cdp-port cannot be used together");
    }
    Ok((options, parsed))
}

fn show_store(db_path: &PathBuf) -> anyhow::Result<()> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
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
                .responses_for_request(&request.id)
                .with_context(|| format!("load responses for request {}", request.id))?
                .pop();
            let status = response
                .and_then(|response| response.status_code)
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!("  {} {} -> {}", request.method, request.url, status);
        }
    }
    Ok(())
}

fn latest_session_url(db_path: &PathBuf) -> anyhow::Result<Option<String>> {
    let store =
        Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))?;
    Ok(store
        .sessions()
        .context("load sessions")?
        .into_iter()
        .rev()
        .find_map(|session| session.root_url))
}

fn print_help() {
    println!("devbench");
    println!();
    println!("usage:");
    println!("  devbench [--db <db-path>] [--cdp-port <port>] <http-url>");
    println!("  devbench [--db <db-path>] --attach-port <port> <http-url>");
    println!("  devbench [--db <db-path>] --launch-on-start <http-url>");
    println!("  devbench tui [db-path]");
    println!("  devbench show [db-path]");
    println!();
    println!("keys:");
    println!("  q/esc quit");
    println!("  tab   switch focus");
    println!("  o     open browser / start capture");
    println!("  j/k   move focused selection");
    println!("  /     filter requests");
    println!("  c     clear request filter / console");
}
