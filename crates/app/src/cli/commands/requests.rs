use crate::query::{RequestListQuery, list_request_rows, request_row};
use crate::services::{
    RequestDetail as CliRequestDetail, latest_session, request_curl_command, request_detail,
};
use anyhow::bail;
use std::path::PathBuf;

use super::super::output::print_json;
use super::super::store::open_store;

pub(crate) fn handle_requests(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    let mut json_output = false;
    let mut filter = None;
    let mut route = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json_output = true,
            "--filter" => {
                let Some(value) = iter.next() else {
                    bail!("usage: faro requests [--route <route>] [--filter <expr>] [--json]");
                };
                filter = Some(value);
            }
            "--route" => {
                let Some(value) = iter.next() else {
                    bail!("usage: faro requests [--route <route>] [--filter <expr>] [--json]");
                };
                route = Some(value);
            }
            unknown => bail!("unknown requests option: {unknown}"),
        }
    }

    let store = open_store(db_path)?;
    let Some(session) = latest_session(&store)? else {
        bail!("no faro sessions found");
    };
    let rows = list_request_rows(
        &store,
        &session.id,
        &RequestListQuery {
            filter,
            route,
            limit: None,
        },
    )?;

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

pub(crate) fn handle_request(db_path: &PathBuf, mut args: Vec<String>) -> anyhow::Result<()> {
    let Some(command) = args.first().cloned() else {
        bail!("usage: faro request <get|curl> <id> [--body] [--json]");
    };
    args.remove(0);
    if command == "curl" {
        return handle_request_curl(db_path, args);
    }
    if command != "get" {
        bail!("usage: faro request <get|curl> <id> [--body] [--json]");
    }
    let Some(request_id) = args.first().cloned() else {
        bail!("usage: faro request get <id> [--body] [--json]");
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
    let detail = request_detail(&store, &request_id, include_body)?;

    if json_output {
        print_json(&detail)?;
    } else if include_body {
        print_request_detail(&detail);
    } else {
        let row = request_row(&detail.request, detail.response.as_ref());
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
        bail!("usage: faro request curl <id> [--json]");
    };
    args.remove(0);
    let json_output = args.iter().any(|arg| arg == "--json");
    for arg in &args {
        if arg != "--json" {
            bail!("unknown request curl option: {arg}");
        }
    }

    let store = open_store(db_path)?;
    let result = request_curl_command(&store, &request_id)?;
    if json_output {
        print_json(&result)?;
    } else {
        println!("{}", result.command);
    }
    Ok(())
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
