mod config;
mod mcp;
mod tui;

use anyhow::{Context, bail};
use config::AppConfig;
use devbench_cdp::{CaptureOptions, CaptureUpdate};
use devbench_core::{
    BodyRecord, ConsoleLevel, CookieRecord, Header, ReplayRecord, RequestRecord, ResponseRecord,
    Session, request_replayed_event,
};
use devbench_store::{Store, inline_text_body};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

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
        "mcp" => mcp::run(options),
        "capture" => handle_capture(options, args),
        "requests" => handle_requests(&options.db_path, args),
        "request" => handle_request(&options.db_path, args),
        "console" => handle_console(&options.db_path, args),
        "storage" => handle_storage(&options.db_path, args),
        "cookies" => handle_cookies(&options.db_path, args),
        "replay" => handle_replay(&options.db_path, args),
        "sql" => handle_sql(&options.db_path, args),
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

#[derive(Debug, Clone, Serialize)]
struct CliRequestRow {
    id: String,
    method: String,
    url: String,
    status_code: Option<i64>,
    resource_type: Option<String>,
    started_at: i64,
    completed_at: Option<i64>,
    duration_ms: Option<i64>,
    body_size: Option<i64>,
    mime_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct CliRequestDetail {
    request: RequestRecord,
    response: Option<ResponseRecord>,
    request_body: Option<CliBody>,
    response_body: Option<CliBody>,
}

#[derive(Debug, Serialize)]
struct CliBody {
    id: String,
    content_type: Option<String>,
    encoding: String,
    size: i64,
    text: String,
}

#[derive(Debug, Serialize)]
struct CliSqlResult {
    columns: Vec<String>,
    rows: Vec<Map<String, Value>>,
    row_count: usize,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
struct CliStorageItem {
    storage_type: String,
    origin: String,
    key: String,
    value: String,
    updated_at: i64,
}

#[derive(Debug, Serialize)]
struct CliReplayResult {
    replay: ReplayRecord,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct CliCurlCommand {
    request_id: String,
    command: String,
    args: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CliCaptureEvent {
    kind: String,
    db_path: String,
    session_id: Option<String>,
    url: Option<String>,
    websocket_url: Option<String>,
    message: Option<String>,
}

fn handle_capture(options: CliOptions, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut duration = None;
    let mut positional = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json_output = true,
            "--for" | "--duration" => {
                let Some(value) = iter.next() else {
                    bail!("usage: devbench capture <url> [--for <duration>] [--json]");
                };
                duration = Some(parse_duration(&value)?);
            }
            _ => positional.push(arg),
        }
    }
    let Some(url) = positional.first().cloned() else {
        bail!("usage: devbench capture <url> [--for <duration>] [--json]");
    };
    if positional.len() > 1 {
        bail!("usage: devbench capture <url> [--for <duration>] [--json]");
    }

    let capture_options = CaptureOptions {
        db_path: options.db_path.clone(),
        url,
        attach_port: options.attach_port,
        launch_port: options.launch_port,
    };
    let updates = devbench_cdp::spawn_capture(capture_options);
    let deadline = duration.map(|duration| Instant::now() + duration);
    if !json_output {
        println!("capturing into {}", options.db_path.display());
    }

    loop {
        let wait = deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|| Duration::from_millis(250));
        if wait.is_zero() {
            break;
        }
        match updates.recv_timeout(wait.min(Duration::from_millis(250))) {
            Ok(update) => print_capture_update(update, &options.db_path, json_output)?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

fn print_capture_update(
    update: CaptureUpdate,
    db_path: &std::path::Path,
    json_output: bool,
) -> anyhow::Result<()> {
    let db_path = db_path.display().to_string();
    let event = match update {
        CaptureUpdate::SessionStarted { session_id, url } => CliCaptureEvent {
            kind: "session_started".to_string(),
            db_path,
            session_id: Some(session_id),
            url: Some(url),
            websocket_url: None,
            message: None,
        },
        CaptureUpdate::Attached { url, websocket_url } => CliCaptureEvent {
            kind: "attached".to_string(),
            db_path,
            session_id: None,
            url: Some(url),
            websocket_url: Some(websocket_url),
            message: None,
        },
        CaptureUpdate::Status(message) => CliCaptureEvent {
            kind: "status".to_string(),
            db_path,
            session_id: None,
            url: None,
            websocket_url: None,
            message: Some(message),
        },
        CaptureUpdate::StoreChanged => CliCaptureEvent {
            kind: "store_changed".to_string(),
            db_path,
            session_id: None,
            url: None,
            websocket_url: None,
            message: None,
        },
        CaptureUpdate::Error(message) => CliCaptureEvent {
            kind: "error".to_string(),
            db_path,
            session_id: None,
            url: None,
            websocket_url: None,
            message: Some(message),
        },
    };
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&event).context("serialize capture JSON event")?
        );
    } else {
        print_capture_text(&event);
    }
    Ok(())
}

fn print_capture_text(event: &CliCaptureEvent) {
    match event.kind.as_str() {
        "session_started" => {
            println!(
                "session {} {}",
                event.session_id.as_deref().unwrap_or("-"),
                event.url.as_deref().unwrap_or("-")
            );
        }
        "attached" => {
            println!(
                "attached {} {}",
                event.url.as_deref().unwrap_or("-"),
                event.websocket_url.as_deref().unwrap_or("-")
            );
        }
        "status" => println!("{}", event.message.as_deref().unwrap_or("status")),
        "store_changed" => println!("store changed"),
        "error" => eprintln!("capture error: {}", event.message.as_deref().unwrap_or("-")),
        _ => {}
    }
}

fn handle_requests(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut filter = None;
    let mut route = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json_output = true,
            "--filter" => {
                let Some(value) = iter.next() else {
                    bail!("usage: devbench requests [--route <route>] [--filter <expr>] [--json]");
                };
                filter = Some(value);
            }
            "--route" => {
                let Some(value) = iter.next() else {
                    bail!("usage: devbench requests [--route <route>] [--filter <expr>] [--json]");
                };
                route = Some(value);
            }
            unknown => bail!("unknown requests option: {unknown}"),
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no devbench sessions found");
    };
    let rows = request_rows_for_session(&store, &session.id)?
        .into_iter()
        .filter(|row| match filter.as_deref() {
            Some(expr) => request_matches_filter(row, expr),
            None => true,
        })
        .filter(|row| match route.as_deref() {
            Some(route) => request_matches_route(&row.url, route),
            None => true,
        })
        .collect::<Vec<_>>();

    if json_output {
        print_json(&rows)?;
    } else {
        for row in rows {
            let status = row
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!("{} {} {} {}", row.id, row.method, status, row.url);
        }
    }
    Ok(())
}

fn handle_request(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    let Some(command) = args.first().cloned() else {
        bail!("usage: devbench request <get|curl> <id> [--body] [--json]");
    };
    args.remove(0);
    if command == "curl" {
        return handle_request_curl(db_path, args);
    }
    if command != "get" {
        bail!("usage: devbench request <get|curl> <id> [--body] [--json]");
    }
    let Some(request_id) = args.first().cloned() else {
        bail!("usage: devbench request get <id> [--body] [--json]");
    };
    args.remove(0);

    let include_body = args.iter().any(|arg| arg == "--body");
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if !matches!(arg.as_str(), "--body" | "--json") {
            bail!("unknown request get option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let (request, response) = find_request_with_response(&store, &request_id)?;
    let detail = CliRequestDetail {
        request: request.clone(),
        response: response.clone(),
        request_body: if include_body {
            load_body(&store, request.request_body_ref.as_deref())?
        } else {
            None
        },
        response_body: if include_body {
            load_body(
                &store,
                response
                    .as_ref()
                    .and_then(|response| response.body_ref.as_deref()),
            )?
        } else {
            None
        },
    };

    if json_output {
        print_json(&detail)?;
    } else if include_body {
        print_request_detail(&detail);
    } else {
        let row = request_row(&request, response.as_ref());
        let status = row
            .status_code
            .map(|status| status.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!("{} {} {} {}", row.id, row.method, status, row.url);
    }
    Ok(())
}

fn handle_request_curl(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    let Some(request_id) = args.first().cloned() else {
        bail!("usage: devbench request curl <id> [--json]");
    };
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown request curl option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let (request, _) = find_request_with_response(&store, &request_id)?;
    let request_body = load_body_text(&store, request.request_body_ref.as_deref())?;
    let args = build_curl_args(&request, request_body.as_deref());
    let command = build_curl_command(&args);
    let result = CliCurlCommand {
        request_id: request.id,
        command,
        args,
    };
    if json_output {
        print_json(&result)?;
    } else {
        println!("{}", result.command);
    }
    Ok(())
}

fn handle_console(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    if args.first().map(String::as_str) != Some("errors") {
        bail!("usage: devbench console errors [--json]");
    }
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown console errors option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no devbench sessions found");
    };
    let logs = store
        .console_logs_for_session(&session.id)
        .with_context(|| format!("load console logs for session {}", session.id))?
        .into_iter()
        .filter(|log| matches!(log.level, ConsoleLevel::Error | ConsoleLevel::Fatal))
        .collect::<Vec<_>>();

    if json_output {
        print_json(&logs)?;
    } else {
        for log in logs {
            println!(
                "{} {} {}",
                log.id,
                log.level.as_str(),
                compact_line(&log.message)
            );
        }
    }
    Ok(())
}

fn handle_storage(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    if args.first().map(String::as_str) != Some("get") {
        bail!("usage: devbench storage get <localStorage|sessionStorage> <key> [--json]");
    }
    args.remove(0);
    let Some(storage_type) = args.first().cloned() else {
        bail!("usage: devbench storage get <localStorage|sessionStorage> <key> [--json]");
    };
    args.remove(0);
    let Some(key) = args.first().cloned() else {
        bail!("usage: devbench storage get <localStorage|sessionStorage> <key> [--json]");
    };
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown storage get option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no devbench sessions found");
    };
    let items = current_storage_items(&store, &session.id)?
        .into_iter()
        .filter(|item| item.storage_type == storage_type && item.key == key)
        .collect::<Vec<_>>();

    if json_output {
        print_json(&items)?;
    } else {
        for item in items {
            println!("{} {}={}", item.origin, item.key, item.value);
        }
    }
    Ok(())
}

fn handle_cookies(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    if args.first().map(String::as_str) != Some("list") {
        bail!("usage: devbench cookies list [--json]");
    }
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown cookies list option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no devbench sessions found");
    };
    let cookies = latest_cookies(&store, &session.id)?;
    if json_output {
        print_json(&cookies)?;
    } else {
        for cookie in cookies {
            println!(
                "{}{} {}={}",
                cookie.domain, cookie.path, cookie.name, cookie.value
            );
        }
    }
    Ok(())
}

fn handle_sql(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut query_parts = Vec::new();
    for arg in args {
        if arg == "--json" {
            json_output = true;
        } else {
            query_parts.push(arg);
        }
    }
    if query_parts.is_empty() {
        bail!("usage: devbench sql <readonly-query> [--json]");
    }

    let query = query_parts.join(" ");
    let result = Store::query_readonly(db_path, &query)
        .with_context(|| format!("run read-only SQL against {}", db_path.display()))?;
    let rows = result
        .rows
        .into_iter()
        .map(|row| {
            result
                .columns
                .iter()
                .cloned()
                .zip(row.into_iter().map(Value::String))
                .collect::<Map<_, _>>()
        })
        .collect::<Vec<_>>();
    let result = CliSqlResult {
        columns: result.columns,
        row_count: rows.len(),
        rows,
        duration_ms: result.duration_ms,
    };

    if json_output {
        print_json(&result)?;
    } else {
        print_sql_table(&result);
    }
    Ok(())
}

fn handle_replay(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut positional = Vec::new();
    for arg in args {
        if arg == "--json" {
            json_output = true;
        } else {
            positional.push(arg);
        }
    }
    let Some(request_id) = positional.first().cloned() else {
        bail!("usage: devbench replay <request-id> [--json]");
    };
    if positional.len() > 1 {
        bail!("usage: devbench replay <request-id> [--json]");
    }

    if !command_exists("curl") {
        bail!("cannot replay: curl not found");
    }

    let store = open_store(db_path)?;
    let (request, _) = find_request_with_response(&store, &request_id)?;
    let request_body = load_body_text(&store, request.request_body_ref.as_deref())?;
    let args = build_curl_args(&request, request_body.as_deref());
    let command = build_curl_command(&args);
    let mut replay = ReplayRecord::new(
        request.session_id.clone(),
        request.tab_id.clone(),
        request.run_id.clone(),
        request.id.clone(),
        command,
    );
    let output = Command::new("curl")
        .args(&args)
        .output()
        .context("run curl replay")?;
    replay.exit_code = output.status.code().map(i64::from);
    replay.status_code = parse_http_status(&output.stdout);
    let body_text = split_http_body(&output.stdout);
    if !body_text.is_empty() {
        let body = inline_text_body(None, body_text);
        replay.response_body_ref = Some(body.id.clone());
        store
            .insert_body(&body)
            .context("insert replay response body")?;
    }
    store
        .insert_replay(&replay)
        .context("insert replay record")?;
    store
        .append_event(&request_replayed_event(&replay))
        .context("append replay event")?;

    let result = CliReplayResult {
        replay,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };
    if json_output {
        print_json(&result)?;
    } else {
        println!(
            "replay {} exit={} status={}",
            result.replay.id,
            output_status_text(result.replay.exit_code),
            output_status_text(result.replay.status_code)
        );
        if !result.stderr.is_empty() {
            eprintln!("{}", result.stderr);
        }
        print!("{}", result.stdout);
    }
    Ok(())
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

fn open_store(db_path: &PathBuf) -> anyhow::Result<Store> {
    Store::open(db_path).with_context(|| format!("open database {}", db_path.display()))
}

fn latest_session(store: &Store) -> anyhow::Result<Option<Session>> {
    Ok(store
        .sessions()
        .context("load sessions")?
        .into_iter()
        .max_by_key(|session| session.created_at))
}

fn request_rows_for_session(store: &Store, session_id: &str) -> anyhow::Result<Vec<CliRequestRow>> {
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

fn latest_responses_by_request(
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

fn request_row(request: &RequestRecord, response: Option<&ResponseRecord>) -> CliRequestRow {
    CliRequestRow {
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

fn find_request_with_response(
    store: &Store,
    request_id: &str,
) -> anyhow::Result<(RequestRecord, Option<ResponseRecord>)> {
    for session in store.sessions().context("load sessions")? {
        let responses = latest_responses_by_request(store, &session.id)?;
        let request = store
            .requests_for_session(&session.id)
            .with_context(|| format!("load requests for session {}", session.id))?
            .into_iter()
            .find(|request| request.id == request_id);
        if let Some(request) = request {
            let response = responses.get(&request.id).cloned();
            return Ok((request, response));
        }
    }
    bail!("request not found: {request_id}");
}

fn load_body(store: &Store, body_id: Option<&str>) -> anyhow::Result<Option<CliBody>> {
    let Some(body_id) = body_id else {
        return Ok(None);
    };
    let Some(body) = store
        .response_body(body_id)
        .with_context(|| format!("load body {body_id}"))?
    else {
        return Ok(None);
    };
    Ok(Some(cli_body(body)))
}

fn load_body_text(store: &Store, body_id: Option<&str>) -> anyhow::Result<Option<String>> {
    Ok(load_body(store, body_id)?.map(|body| body.text))
}

fn cli_body(body: BodyRecord) -> CliBody {
    CliBody {
        id: body.id,
        content_type: body.content_type,
        encoding: body.encoding,
        size: body.size,
        text: String::from_utf8_lossy(&body.data).to_string(),
    }
}

fn print_request_detail(detail: &CliRequestDetail) {
    let status = detail
        .response
        .as_ref()
        .and_then(|response| response.status_code)
        .map(|status| status.to_string())
        .unwrap_or_else(|| "-".to_string());
    println!(
        "{} {} {} {}",
        detail.request.id, detail.request.method, status, detail.request.url
    );
    if let Some(body) = &detail.request_body {
        println!("\n--- request body {} bytes ---\n{}", body.size, body.text);
    }
    if let Some(body) = &detail.response_body {
        println!("\n--- response body {} bytes ---\n{}", body.size, body.text);
    }
}

fn request_matches_filter(row: &CliRequestRow, expr: &str) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return true;
    }
    if let Some((field, op, value)) = parse_filter_expr(expr) {
        return match field.as_str() {
            "status" | "status_code" => compare_i64(row.status_code, &op, &value),
            "duration" | "duration_ms" => compare_i64(row.duration_ms, &op, &value),
            "size" | "body_size" => compare_i64(row.body_size, &op, &value),
            "method" => compare_str(&row.method, &op, &value),
            "url" => compare_str(&row.url, &op, &value),
            "type" | "resource_type" => {
                compare_optional_str(row.resource_type.as_deref(), &op, &value)
            }
            "mime" | "mime_type" => compare_optional_str(row.mime_type.as_deref(), &op, &value),
            _ => request_contains(row, expr),
        };
    }
    request_contains(row, expr)
}

fn request_matches_route(url: &str, route: &str) -> bool {
    let request_path = cli_path_for_url(url);
    let route_path = cli_path_for_url(route);
    if route_path.contains(':') || route_path.contains('*') {
        return route_pattern_matches(&request_path, &route_path);
    }
    request_path == route_path
        || request_path
            .strip_prefix(&route_path)
            .map(|tail| tail.starts_with('/'))
            .unwrap_or(false)
}

fn route_pattern_matches(request_path: &str, route_path: &str) -> bool {
    let request_segments = route_segments(request_path);
    let route_segments = route_segments(route_path);
    let mut request_index = 0;
    for route_segment in &route_segments {
        if *route_segment == "*" {
            return true;
        }
        let Some(request_segment) = request_segments.get(request_index) else {
            return false;
        };
        if route_segment.starts_with(':') {
            request_index += 1;
            continue;
        }
        if route_segment != request_segment {
            return false;
        }
        request_index += 1;
    }
    request_index == request_segments.len()
}

fn route_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn cli_path_for_url(value: &str) -> String {
    let without_fragment = value.split('#').next().unwrap_or(value);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let path = if let Some(without_scheme) = without_query
        .strip_prefix("http://")
        .or_else(|| without_query.strip_prefix("https://"))
    {
        without_scheme
            .split_once('/')
            .map(|(_, path)| format!("/{path}"))
            .unwrap_or_else(|| "/".to_string())
    } else if without_query.starts_with('/') {
        without_query.to_string()
    } else {
        format!("/{without_query}")
    };
    normalize_route_path(&path)
}

fn normalize_route_path(path: &str) -> String {
    let trimmed = path.trim();
    let with_leading_slash = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    let without_trailing = with_leading_slash.trim_end_matches('/');
    if without_trailing.is_empty() {
        "/".to_string()
    } else {
        without_trailing.to_string()
    }
}

fn parse_filter_expr(expr: &str) -> Option<(String, String, String)> {
    let operators = [">=", "<=", "==", "!=", ">", "<", "=", "contains", "~"];
    if let [field, op, value @ ..] = expr.split_whitespace().collect::<Vec<_>>().as_slice()
        && operators.contains(op)
    {
        return Some((
            field.to_ascii_lowercase(),
            (*op).to_string(),
            value
                .join(" ")
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        ));
    }
    for op in operators {
        if matches!(op, "contains" | "~") {
            continue;
        }
        if let Some((field, value)) = expr.split_once(op) {
            let field = field.trim();
            let value = value.trim();
            if !field.is_empty() && !value.is_empty() {
                return Some((
                    field.to_ascii_lowercase(),
                    op.to_string(),
                    value.trim_matches('"').trim_matches('\'').to_string(),
                ));
            }
        }
    }
    None
}

fn compare_i64(actual: Option<i64>, op: &str, expected: &str) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let Ok(expected) = expected.parse::<i64>() else {
        return false;
    };
    match op {
        ">" => actual > expected,
        ">=" => actual >= expected,
        "<" => actual < expected,
        "<=" => actual <= expected,
        "!=" => actual != expected,
        "=" | "==" => actual == expected,
        _ => false,
    }
}

fn compare_optional_str(actual: Option<&str>, op: &str, expected: &str) -> bool {
    actual
        .map(|actual| compare_str(actual, op, expected))
        .unwrap_or(false)
}

fn compare_str(actual: &str, op: &str, expected: &str) -> bool {
    let actual_lower = actual.to_ascii_lowercase();
    let expected_lower = expected.to_ascii_lowercase();
    match op {
        "=" | "==" => actual_lower == expected_lower,
        "!=" => actual_lower != expected_lower,
        "contains" | "~" => actual_lower.contains(&expected_lower),
        _ => false,
    }
}

fn request_contains(row: &CliRequestRow, needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    [
        row.id.as_str(),
        row.method.as_str(),
        row.url.as_str(),
        row.resource_type.as_deref().unwrap_or(""),
        row.mime_type.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .any(|value| value.to_ascii_lowercase().contains(&needle))
        || row
            .status_code
            .map(|status| status.to_string().contains(&needle))
            .unwrap_or(false)
}

fn current_storage_items(store: &Store, session_id: &str) -> anyhow::Result<Vec<CliStorageItem>> {
    let mut items: HashMap<(String, String, String), CliStorageItem> = HashMap::new();
    for snapshot in store
        .storage_snapshots_for_session(session_id)
        .with_context(|| format!("load storage snapshots for session {session_id}"))?
    {
        for entry in snapshot.entries {
            let item = CliStorageItem {
                storage_type: snapshot.storage_type.clone(),
                origin: snapshot.origin.clone(),
                key: entry.key,
                value: entry.value,
                updated_at: snapshot.ts,
            };
            items.insert(
                (
                    item.storage_type.clone(),
                    item.origin.clone(),
                    item.key.clone(),
                ),
                item,
            );
        }
    }
    for event in store
        .storage_events_for_session(session_id)
        .with_context(|| format!("load storage events for session {session_id}"))?
    {
        if event.operation == "clear" {
            items.retain(|(storage_type, origin, _), _| {
                storage_type != &event.storage_type || origin != &event.origin
            });
            continue;
        }
        let Some(key) = event.key else {
            continue;
        };
        let map_key = (
            event.storage_type.clone(),
            event.origin.clone(),
            key.clone(),
        );
        if event.operation == "remove" {
            items.remove(&map_key);
            continue;
        }
        if let Some(value) = event.new_value {
            items.insert(
                map_key,
                CliStorageItem {
                    storage_type: event.storage_type,
                    origin: event.origin,
                    key,
                    value,
                    updated_at: event.ts,
                },
            );
        }
    }
    let mut items = items.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.storage_type
            .cmp(&b.storage_type)
            .then_with(|| a.origin.cmp(&b.origin))
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(items)
}

fn latest_cookies(store: &Store, session_id: &str) -> anyhow::Result<Vec<CookieRecord>> {
    let snapshots = store
        .cookie_snapshots_for_session(session_id)
        .with_context(|| format!("load cookie snapshots for session {session_id}"))?;
    Ok(snapshots
        .into_iter()
        .max_by_key(|snapshot| snapshot.ts)
        .map(|snapshot| snapshot.cookies)
        .unwrap_or_default())
}

fn print_sql_table(result: &CliSqlResult) {
    if result.columns.is_empty() {
        println!("{} rows in {}ms", result.row_count, result.duration_ms);
        return;
    }
    println!("{}", result.columns.join("\t"));
    for row in &result.rows {
        let values = result
            .columns
            .iter()
            .map(|column| {
                row.get(column)
                    .and_then(Value::as_str)
                    .map(compact_cell)
                    .unwrap_or_else(|| "NULL".to_string())
            })
            .collect::<Vec<_>>();
        println!("{}", values.join("\t"));
    }
    eprintln!("{} rows in {}ms", result.row_count, result.duration_ms);
}

fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("serialize JSON output")?
    );
    Ok(())
}

fn compact_cell(value: &str) -> String {
    const LIMIT: usize = 120;
    let value = compact_line(value);
    if value.chars().count() <= LIMIT {
        return value;
    }
    let mut compact = value
        .chars()
        .take(LIMIT.saturating_sub(1))
        .collect::<String>();
    compact.push_str("...");
    compact
}

fn compact_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_curl_args(request: &RequestRecord, request_body: Option<&str>) -> Vec<String> {
    let mut parts = vec![
        "-sS".to_string(),
        "-i".to_string(),
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

fn build_curl_command(args: &[String]) -> String {
    format!(
        "curl {}",
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn push_header_arg(parts: &mut Vec<String>, header: &Header) {
    parts.push("-H".to_string());
    parts.push(format!("{}: {}", header.name, header.value));
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn parse_http_status(output: &[u8]) -> Option<i64> {
    let text = String::from_utf8_lossy(output);
    let mut status = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(protocol) = parts.next() else {
            continue;
        };
        if !protocol.starts_with("HTTP/") {
            continue;
        }
        let Some(value) = parts.next() else {
            continue;
        };
        let Ok(parsed) = value.parse::<i64>() else {
            continue;
        };
        status = Some(parsed);
    }
    status
}

fn split_http_body(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.rsplit_once("\r\n\r\n")
        .or_else(|| text.rsplit_once("\n\n"))
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(command).exists()))
        .unwrap_or(false)
}

fn output_status_text(status: Option<i64>) -> String {
    status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn parse_duration(value: &str) -> anyhow::Result<Duration> {
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
    println!("  devbench mcp");
    println!("  devbench capture <http-url> [--for <duration>] [--json]");
    println!("  devbench tui [db-path]");
    println!("  devbench show [db-path]");
    println!("  devbench requests [--route <route>] [--filter <expr>] [--json]");
    println!("  devbench request get <id> [--body] [--json]");
    println!("  devbench request curl <id> [--json]");
    println!("  devbench console errors [--json]");
    println!("  devbench storage get <localStorage|sessionStorage> <key> [--json]");
    println!("  devbench cookies list [--json]");
    println!("  devbench replay <request-id> [--json]");
    println!("  devbench sql <readonly-query> [--json]");
    println!();
    println!("keys:");
    println!("  q/esc quit");
    println!("  tab   switch focus");
    println!("  o     open browser / start capture");
    println!("  j/k   move focused selection");
    println!("  /     filter requests");
    println!("  c     clear request filter / console");
}

#[cfg(test)]
mod tests {
    use super::{cli_path_for_url, parse_duration, request_matches_route};
    use std::time::Duration;

    #[test]
    fn request_route_filter_matches_plain_route_and_descendants() {
        assert!(request_matches_route(
            "https://example.com/api/users",
            "/api/users"
        ));
        assert!(request_matches_route(
            "https://example.com/api/users/123",
            "/api/users"
        ));
        assert!(!request_matches_route(
            "https://example.com/api/user-settings",
            "/api/users"
        ));
    }

    #[test]
    fn request_route_filter_matches_param_and_wildcard_patterns() {
        assert!(request_matches_route(
            "https://example.com/api/users/123",
            "/api/users/:id"
        ));
        assert!(!request_matches_route(
            "https://example.com/api/users/123/profile",
            "/api/users/:id"
        ));
        assert!(request_matches_route(
            "https://example.com/api/users/123/profile",
            "/api/users/*"
        ));
    }

    #[test]
    fn cli_path_for_url_normalizes_urls_paths_and_queries() {
        assert_eq!(cli_path_for_url("https://example.com"), "/");
        assert_eq!(
            cli_path_for_url("https://example.com/api/users?x=1"),
            "/api/users"
        );
        assert_eq!(cli_path_for_url("api/users/"), "/api/users");
    }

    #[test]
    fn parse_duration_accepts_agent_friendly_units() {
        assert_eq!(parsed_duration("500ms"), Duration::from_millis(500));
        assert_eq!(parsed_duration("5s"), Duration::from_secs(5));
        assert_eq!(parsed_duration("2m"), Duration::from_secs(120));
    }

    fn parsed_duration(value: &str) -> Duration {
        match parse_duration(value) {
            Ok(duration) => duration,
            Err(error) => panic!("duration parse failed: {error}"),
        }
    }
}
