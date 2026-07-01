use anyhow::Context;
use serde::Serialize;

pub(super) fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("serialize JSON output")?
    );
    Ok(())
}

pub(super) fn compact_cell(value: &str) -> String {
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

pub(super) fn compact_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn output_status_text(status: Option<i64>) -> String {
    status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn print_help() {
    println!("faro");
    println!();
    println!("usage:");
    println!("  faro [--db <db-path>] [--cdp-port <port>] <http-url>");
    println!("  faro [--db <db-path>] --attach-port <port> <http-url>");
    println!("  faro [--db <db-path>] --launch-on-start <http-url>");
    println!("  faro mcp [--mcp-allow-mutation] [--mcp-allow-sensitive]");
    println!("  faro capture <http-url> [--for <duration>] [--json]");
    println!("  faro tui [db-path]");
    println!("  faro show [db-path]");
    println!("  faro requests [--route <route>] [--filter <expr>] [--json]");
    println!("  faro request get <id> [--body] [--json]");
    println!("  faro request curl <id> [--json]");
    println!("  faro console errors [--json]");
    println!("  faro storage get <localStorage|sessionStorage> <key> [--json]");
    println!("  faro cookies list [--json]");
    println!("  faro sessions list [--json]");
    println!("  faro sessions compact [--vacuum] [--json]");
    println!("  faro sessions nuke --yes [--vacuum] [--json]");
    println!("  faro replay <request-id> [--json]");
    println!("  faro sql <readonly-query> [--json]");
    println!();
    println!("keys:");
    println!("  q/esc quit");
    println!("  tab   switch focus");
    println!("  o     open browser / start capture");
    println!("  j/k   move focused selection");
    println!("  /     filter requests");
    println!("  c     clear request filter / console");
}
