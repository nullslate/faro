use anyhow::{Context, bail};
use faro_store::Store;
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

use super::super::output::{compact_cell, print_json};

#[derive(Debug, Serialize)]
struct CliSqlResult {
    columns: Vec<String>,
    rows: Vec<Map<String, Value>>,
    row_count: usize,
    duration_ms: u128,
}

pub(crate) fn handle_sql(db_path: &PathBuf, args: Vec<String>) -> anyhow::Result<()> {
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
        bail!("usage: faro sql <readonly-query> [--json]");
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
