use crate::{Result, Store, StoreError};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SqlQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub duration_ms: u128,
}

impl Store {
    pub fn query_readonly(path: impl AsRef<Path>, sql: &str) -> Result<SqlQueryResult> {
        validate_readonly_sql(sql)?;
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.pragma_update(None, "query_only", "ON")?;
        let mut stmt = conn.prepare(sql)?;
        if !stmt.readonly() {
            return Err(StoreError::QueryRejected(
                "statement is not read-only".to_string(),
            ));
        }
        let columns = stmt
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let column_count = stmt.column_count();
        let started = Instant::now();
        let mut rows = stmt.query([])?;
        let mut result_rows = Vec::new();
        while let Some(row) = rows.next()? {
            let mut values = Vec::with_capacity(column_count);
            for index in 0..column_count {
                values.push(sql_value_to_string(row.get_ref(index)?));
            }
            result_rows.push(values);
        }
        Ok(SqlQueryResult {
            columns,
            rows: result_rows,
            duration_ms: started.elapsed().as_millis(),
        })
    }

    pub fn schema_sql(path: impl AsRef<Path>) -> Result<String> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut stmt = conn.prepare(
            "SELECT sql
             FROM sqlite_schema
             WHERE sql IS NOT NULL
               AND type IN ('table', 'index', 'view', 'trigger')
               AND name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )?;
        let statements = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(statements.join(";\n\n") + ";\n")
    }
}

pub(crate) fn validate_readonly_sql(sql: &str) -> Result<()> {
    let stripped = strip_sql_comments(sql);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return Err(StoreError::QueryRejected("query is empty".to_string()));
    }
    if has_multiple_sql_statements(trimmed) {
        return Err(StoreError::QueryRejected(
            "only one SQL statement is allowed".to_string(),
        ));
    }
    let keyword = first_sql_keyword(trimmed);
    match keyword.as_deref() {
        Some("select" | "with" | "values" | "explain") => Ok(()),
        Some(other) => Err(StoreError::QueryRejected(format!(
            "{other} statements are not allowed"
        ))),
        None => Err(StoreError::QueryRejected(
            "query must start with a SQL keyword".to_string(),
        )),
    }
}

fn sql_value_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(value) => String::from_utf8_lossy(value).to_string(),
        ValueRef::Blob(value) => format!("BLOB {} bytes", value.len()),
    }
}

fn first_sql_keyword(sql: &str) -> Option<String> {
    let keyword = sql
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    if keyword.is_empty() {
        None
    } else {
        Some(keyword)
    }
}

fn has_multiple_sql_statements(sql: &str) -> bool {
    let mut seen_statement_end = false;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = sql.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\'' if !in_double_quote => {
                if in_single_quote && chars.peek() == Some(&'\'') {
                    let _escaped_quote = chars.next();
                } else {
                    in_single_quote = !in_single_quote;
                }
            }
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ';' if !in_single_quote && !in_double_quote => seen_statement_end = true,
            _ if seen_statement_end && !character.is_whitespace() => return true,
            _ => {}
        }
    }
    false
}

fn strip_sql_comments(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    while let Some(character) = chars.next() {
        match character {
            '\'' if !in_double_quote => {
                output.push(character);
                if in_single_quote && chars.peek() == Some(&'\'') {
                    if let Some(escaped_quote) = chars.next() {
                        output.push(escaped_quote);
                    }
                } else {
                    in_single_quote = !in_single_quote;
                }
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                output.push(character);
            }
            '-' if !in_single_quote && !in_double_quote && chars.peek() == Some(&'-') => {
                let _second_dash = chars.next();
                for comment_character in chars.by_ref() {
                    if comment_character == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            '/' if !in_single_quote && !in_double_quote && chars.peek() == Some(&'*') => {
                let _asterisk = chars.next();
                let mut previous = '\0';
                for comment_character in chars.by_ref() {
                    if previous == '*' && comment_character == '/' {
                        break;
                    }
                    previous = comment_character;
                }
                output.push(' ');
            }
            _ => output.push(character),
        }
    }
    output
}
