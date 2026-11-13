//! Reading the source and writing the targets, both as JSON rows.
//!
//! A row travels as a JSON object whose keys are the target's column names: the source query
//! renames and reshapes (`SELECT space_id AS organisation_id ...`), Rust adjusts what SQL cannot (a
//! secret encrypted again, a document of preferences reshaped), and the target takes the rows with
//! `json_populate_recordset`, which casts each value to its column's type. So one helper writes
//! every table, and the types stay PostgreSQL's on both sides (`bytea` travels as its `\x` text,
//! arrays as JSON arrays).

use serde_json::Value;
use tokio_postgres::Client;

use crate::migrate::report::Report;

/// Rows written per statement.
const BATCH: usize = 500;

/// The monolith's database, read only.
pub struct Source<'a> {
    client: &'a Client,
}

impl<'a> Source<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// The rows of `query`, each a JSON object of its columns.
    pub async fn rows(&self, query: &str) -> Result<Vec<Value>, String> {
        rows(self.client, query).await
    }
}

/// The rows of `query` on `client`, each a JSON object of its columns.
pub async fn rows(client: &Client, query: &str) -> Result<Vec<Value>, String> {
    let sql = format!("SELECT coalesce(json_agg(t), '[]')::text FROM ({query}) t");
    let row = client
        .query_one(&sql, &[])
        .await
        .map_err(|e| failed(&e, query))?;
    let text: String = row.get(0);
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(match value {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    })
}

/// One number from `query` (`SELECT count(*) ...`).
pub async fn count(client: &Client, query: &str) -> Result<i64, String> {
    let row = client
        .query_one(query, &[])
        .await
        .map_err(|e| failed(&e, query))?;
    Ok(row.get(0))
}

/// Run statements that return nothing.
pub async fn execute(client: &Client, sql: &str) -> Result<(), String> {
    client.batch_execute(sql).await.map_err(|e| failed(&e, sql))
}

/// Start the transaction a part writes in.
pub async fn begin(client: &Client) -> Result<(), String> {
    execute(client, "BEGIN").await
}

/// End it: committed, or rolled back in a dry run.
pub async fn finish(client: &Client, dry_run: bool) -> Result<(), String> {
    execute(client, if dry_run { "ROLLBACK" } else { "COMMIT" }).await
}

/// Write `rows` into `table` on `target`, keeping what is there (`ON CONFLICT DO NOTHING`), and
/// count both in `report` under `name` (`drive.files`).
pub async fn insert(
    target: &Client,
    table: &str,
    rows: &[Value],
    report: &mut Report,
    name: &str,
) -> Result<(), String> {
    let line = report.line(name);
    line.read += rows.len() as u64;
    if rows.is_empty() {
        return Ok(());
    }
    // Every column any row names. A column no row names takes its default; one that only some rows
    // name is null in the others (not its default).
    let mut names: Vec<&str> = Vec::new();
    for row in rows {
        for key in row.as_object().into_iter().flat_map(|o| o.keys()) {
            if !names.contains(&key.as_str()) {
                names.push(key);
            }
        }
    }
    let columns: Vec<String> = names.iter().map(|c| format!("\"{c}\"")).collect();
    let columns = columns.join(", ");
    let sql = format!(
        "WITH written AS (INSERT INTO public.{table} ({columns}) \
         SELECT {columns} FROM json_populate_recordset(NULL::public.{table}, $1::text::json) \
         ON CONFLICT DO NOTHING RETURNING 1) SELECT count(*) FROM written"
    );
    for batch in rows.chunks(BATCH) {
        let body = Value::Array(batch.to_vec()).to_string();
        let row = target
            .query_one(&sql, &[&body])
            .await
            .map_err(|e| failed(&e, &format!("writing {name}")))?;
        let written: i64 = row.get(0);
        let line = report.line(name);
        line.written += written as u64;
        line.kept += (batch.len() as i64 - written) as u64;
    }
    Ok(())
}

/// A database error, with what was being done and PostgreSQL's own words.
fn failed(error: &tokio_postgres::Error, doing: &str) -> String {
    let said = error
        .as_db_error()
        .map(|e| {
            let mut said = e.message().to_owned();
            if let Some(detail) = e.detail() {
                said.push_str(": ");
                said.push_str(detail);
            }
            said
        })
        .unwrap_or_else(|| error.to_string());
    let doing: String = doing.split_whitespace().collect::<Vec<_>>().join(" ");
    let doing: String = doing.chars().take(160).collect();
    format!("{said} ({doing})")
}
