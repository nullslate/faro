use super::editor::run_editor_with_env;
use super::state::WorkbenchState;
use anyhow::Context;
use faro_core::config_dir;
use faro_store::Store;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn edit_sql_query(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut WorkbenchState,
) -> anyhow::Result<()> {
    let workspace = create_sql_workspace(app).context("create SQL editor workspace")?;
    let path = workspace.query_path;
    let database_url = format!("sqlite://{}", app.db_path.display());
    let editor_env = [
        ("DATABASE_URL", database_url),
        ("SQLITE_DATABASE_PATH", app.db_path.display().to_string()),
        (
            "FARO_SQL_SCHEMA",
            workspace.schema_path.display().to_string(),
        ),
        ("FARO_SQL_WORKSPACE", workspace.dir.display().to_string()),
    ];
    app.status =
        run_editor_with_env(terminal, &path, &editor_env).context("run editor for SQL query")?;
    let query = fs::read_to_string(&path)
        .with_context(|| format!("read SQL query file {}", path.display()))?;
    match Store::query_readonly(&app.db_path, &query) {
        Ok(result) => {
            let persisted_query = sql_query_body(&query);
            save_last_sql_query(&persisted_query).context("save last SQL query")?;
            let request_ids = sql_request_ids(app, &result);
            app.apply_sql_request_filter(persisted_query, request_ids);
        }
        Err(error) => app.show_sql_error(query, error.to_string()),
    }
    Ok(())
}

struct SqlEditorWorkspace {
    dir: PathBuf,
    query_path: PathBuf,
    schema_path: PathBuf,
}

fn create_sql_workspace(app: &WorkbenchState) -> anyhow::Result<SqlEditorWorkspace> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let dir = env::temp_dir().join(format!("faro-sql-{}-{now}", std::process::id()));
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let schema_path = dir.join("schema.sql");
    let query_path = dir.join("query.sql");
    let schema = sql_schema_sidecar(app).context("load SQL schema sidecar")?;
    fs::write(&schema_path, schema).with_context(|| format!("write {}", schema_path.display()))?;
    fs::write(dir.join(".sqllsrc.json"), sql_language_server_config(app))
        .with_context(|| format!("write {}", dir.join(".sqllsrc.json").display()))?;
    fs::write(dir.join(".sqls.yml"), sqls_config(app))
        .with_context(|| format!("write {}", dir.join(".sqls.yml").display()))?;
    fs::write(&query_path, sql_editor_template(app, &schema_path))
        .with_context(|| format!("write {}", query_path.display()))?;
    Ok(SqlEditorWorkspace {
        dir,
        query_path,
        schema_path,
    })
}

fn sql_schema_sidecar(app: &WorkbenchState) -> anyhow::Result<String> {
    let schema = Store::schema_sql(&app.db_path)
        .with_context(|| format!("load schema from {}", app.db_path.display()))?;
    Ok(schema)
}

fn sql_language_server_config(app: &WorkbenchState) -> String {
    format!(
        r#"{{
  "connections": [
    {{
      "name": "faro",
      "adapter": "sqlite3",
      "filename": "{}"
    }}
  ]
}}
"#,
        json_escape(&app.db_path.display().to_string())
    )
}

fn sqls_config(app: &WorkbenchState) -> String {
    format!(
        "connections:\n  - alias: faro\n    driver: sqlite3\n    dataSourceName: \"{}\"\n",
        yaml_double_quote_escape(&app.db_path.display().to_string())
    )
}

fn sql_editor_template(app: &WorkbenchState, schema_path: &Path) -> String {
    let query = if app.last_sql_query.trim().is_empty() {
        "SELECT
    r.id AS request_id,
    r.method,
    r.url,
    responses.status_code,
    responses.mime_type,
    responses.body_size,
    r.started_at
FROM requests r
LEFT JOIN responses ON responses.request_id = r.id
ORDER BY r.started_at DESC
LIMIT 50;"
    } else {
        app.last_sql_query.trim()
    };
    let database_url = format!("sqlite://{}", app.db_path.display());
    [
        "-- Faro SQL Query",
        "-- Read-only, single-statement queries only. SELECT, WITH, VALUES, and EXPLAIN are allowed.",
        "-- Filetype is .sql so your editor/LSP should attach normally.",
        "--",
        &format!("-- Database: {}", app.db_path.display()),
        &format!("-- Database URL: {database_url}"),
        &format!("-- Schema sidecar: {}", schema_path.display()),
        "-- Env while editor is open: DATABASE_URL, SQLITE_DATABASE_PATH, FARO_SQL_SCHEMA.",
        "-- Workspace also includes .sqllsrc.json and .sqls.yml for common SQL LSPs.",
        "--",
        "-- Recent requests:",
        "-- SELECT r.id AS request_id, r.method, r.url, responses.status_code, responses.body_size",
        "-- FROM requests r LEFT JOIN responses ON responses.request_id = r.id",
        "-- ORDER BY r.started_at DESC LIMIT 50;",
        "--",
        "-- Console errors:",
        "-- SELECT ts, level, source, message FROM console_logs WHERE level IN ('error', 'fatal') ORDER BY ts DESC LIMIT 50;",
        "--",
        "-- Slow requests:",
        "-- SELECT id AS request_id, method, url, completed_at - started_at AS duration_ms FROM requests WHERE completed_at IS NOT NULL ORDER BY duration_ms DESC LIMIT 50;",
        "--",
        "-- Cookies/storage:",
        "-- SELECT ts, name, domain, path, value FROM cookie_events ORDER BY ts DESC LIMIT 50;",
        "-- SELECT ts, origin, storage_type, key, new_value FROM storage_events ORDER BY ts DESC LIMIT 50;",
        "",
        query,
        "",
    ]
    .join("\n")
}

fn sql_request_ids(app: &WorkbenchState, result: &faro_store::SqlQueryResult) -> HashSet<String> {
    let known_ids = app
        .requests
        .iter()
        .map(|request| request.request.id.as_str())
        .collect::<HashSet<_>>();
    let known_urls = app
        .requests
        .iter()
        .map(|request| (request.request.url.as_str(), request.request.id.as_str()))
        .collect::<std::collections::HashMap<_, _>>();

    if let Some(column_index) = result.columns.iter().position(|column| {
        let normalized = column.trim_matches('"').to_ascii_lowercase();
        matches!(
            normalized.as_str(),
            "request_id" | "source_request_id" | "id"
        )
    }) {
        let ids = result
            .rows
            .iter()
            .filter_map(|row| row.get(column_index))
            .filter(|value| known_ids.contains(value.as_str()))
            .cloned()
            .collect::<HashSet<_>>();
        if !ids.is_empty() || result.rows.is_empty() {
            return ids;
        }
    }

    result
        .rows
        .iter()
        .flat_map(|row| row.iter())
        .filter_map(|value| {
            if known_ids.contains(value.as_str()) {
                Some(value.clone())
            } else {
                known_urls.get(value.as_str()).map(|id| (*id).to_string())
            }
        })
        .collect()
}

fn sql_query_body(query: &str) -> String {
    let lines = query.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| {
            let trimmed = line.trim_start();
            !trimmed.is_empty() && !trimmed.starts_with("--")
        })
        .unwrap_or(lines.len());
    lines[start..].join("\n").trim().to_string()
}

pub(super) fn load_last_sql_query() -> anyhow::Result<String> {
    let Some(path) = sql_query_path() else {
        return Ok(String::new());
    };
    match fs::read_to_string(&path) {
        Ok(query) => Ok(query),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn save_last_sql_query(query: &str) -> anyhow::Result<()> {
    let path =
        sql_query_path().ok_or_else(|| anyhow::anyhow!("Faro config directory is unavailable"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&path, query).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn sql_query_path() -> Option<PathBuf> {
    config_dir("faro").map(|path| path.join("last.sql"))
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn yaml_double_quote_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
