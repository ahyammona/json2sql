//! json2sql
//!
//! Reads a JSON file (an array of flat objects, or a single object) and
//! inserts the records into MySQL, PostgreSQL, or SQLite, using one code
//! path via sqlx's `Any` driver. The target database is picked automatically
//! from the connection string:
//!
//!   - `mysql://user:pass@host/db`
//!   - `postgres://user:pass@host/db`
//!   - `sqlite://path/to/file.db`  (or `sqlite::memory:`)
//!
//! Nested objects/arrays inside a record are stored as JSON text in a TEXT
//! column, since this tool targets flat-record import, not relational
//! normalization.

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde_json::Value;
use sqlx::any::{AnyArguments, AnyPoolOptions};
use sqlx::query::Query;
use sqlx::Any;
use std::fs;
use std::path::Path;

/// Inferred column type, independent of database dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColType {
    Integer,
    Real,
    Boolean,
    Text,
}

/// Which SQL dialect we're talking to, detected from the connection URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
}

pub fn detect_dialect(db_url: &str) -> Result<Dialect> {
    if db_url.starts_with("postgres://") || db_url.starts_with("postgresql://") {
        Ok(Dialect::Postgres)
    } else if db_url.starts_with("mysql://") {
        Ok(Dialect::MySql)
    } else if db_url.starts_with("sqlite:") {
        Ok(Dialect::Sqlite)
    } else {
        bail!(
            "Could not detect database type from URL '{db_url}'. \
             Expected it to start with mysql://, postgres://, or sqlite:"
        )
    }
}

fn value_type(v: &Value) -> Option<ColType> {
    match v {
        Value::Null => None,
        Value::Bool(_) => Some(ColType::Boolean),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                Some(ColType::Integer)
            } else {
                Some(ColType::Real)
            }
        }
        Value::String(_) => Some(ColType::Text),
        // Nested structures get JSON-stringified before insert.
        Value::Array(_) | Value::Object(_) => Some(ColType::Text),
    }
}

fn merge(a: ColType, b: ColType) -> ColType {
    use ColType::*;
    if a == b {
        return a;
    }
    match (a, b) {
        (Integer, Real) | (Real, Integer) => Real,
        _ => Text,
    }
}

/// Scan every record and infer one column type per key, preserving the
/// order keys were first seen in (so CREATE TABLE columns come out stable
/// and readable).
pub fn infer_schema(records: &[serde_json::Map<String, Value>]) -> IndexMap<String, ColType> {
    let mut schema: IndexMap<String, ColType> = IndexMap::new();
    for rec in records {
        for (k, v) in rec {
            match value_type(v) {
                Some(t) => {
                    schema
                        .entry(k.clone())
                        .and_modify(|existing| *existing = merge(*existing, t))
                        .or_insert(t);
                }
                None => {
                    schema.entry(k.clone()).or_insert(ColType::Text);
                }
            }
        }
    }
    schema
}

fn col_sql_type(t: ColType, d: Dialect) -> &'static str {
    match (t, d) {
        (ColType::Integer, Dialect::Postgres) => "BIGINT",
        (ColType::Integer, Dialect::MySql) => "BIGINT",
        (ColType::Integer, Dialect::Sqlite) => "INTEGER",

        (ColType::Real, Dialect::Postgres) => "DOUBLE PRECISION",
        (ColType::Real, Dialect::MySql) => "DOUBLE",
        (ColType::Real, Dialect::Sqlite) => "REAL",

        (ColType::Boolean, Dialect::Postgres) => "BOOLEAN",
        (ColType::Boolean, Dialect::MySql) => "TINYINT(1)",
        (ColType::Boolean, Dialect::Sqlite) => "INTEGER",

        (ColType::Text, _) => "TEXT",
    }
}

fn quote_ident(name: &str, d: Dialect) -> String {
    match d {
        Dialect::MySql => format!("`{}`", name.replace('`', "``")),
        Dialect::Postgres | Dialect::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

pub fn build_create_table_sql(
    table: &str,
    schema: &IndexMap<String, ColType>,
    dialect: Dialect,
) -> String {
    let cols: Vec<String> = schema
        .iter()
        .map(|(name, t)| format!("{} {}", quote_ident(name, dialect), col_sql_type(*t, dialect)))
        .collect();
    format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        quote_ident(table, dialect),
        cols.join(", ")
    )
}

fn bind_value<'q>(
    q: Query<'q, Any, AnyArguments<'q>>,
    v: &Value,
) -> Query<'q, Any, AnyArguments<'q>> {
    match v {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.bind(i)
            } else if let Some(f) = n.as_f64() {
                q.bind(f)
            } else {
                q.bind(n.to_string())
            }
        }
        Value::String(s) => q.bind(s.clone()),
        Value::Array(_) | Value::Object(_) => q.bind(v.to_string()),
    }
}

fn load_records(json_path: &Path) -> Result<Vec<serde_json::Map<String, Value>>> {
    let raw = fs::read_to_string(json_path)
        .with_context(|| format!("Failed to read JSON file at {}", json_path.display()))?;
    let parsed: Value = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse JSON in {}", json_path.display()))?;

    match parsed {
        Value::Array(items) => items
            .into_iter()
            .map(|v| {
                v.as_object()
                    .cloned()
                    .ok_or_else(|| anyhow!("Every item in the JSON array must be an object"))
            })
            .collect(),
        Value::Object(obj) => Ok(vec![obj]),
        _ => bail!("Top-level JSON must be an object, or an array of objects"),
    }
}

/// Read `json_path`, and insert every record into `table` on the database
/// at `db_url`. If `create_table` is true, a `CREATE TABLE IF NOT EXISTS`
/// is issued first, with columns inferred from the data. Returns the
/// number of records inserted.
pub async fn import_json_to_sql(
    db_url: &str,
    json_path: &Path,
    table: &str,
    create_table: bool,
    batch_size: usize,
) -> Result<usize> {
    sqlx::any::install_default_drivers();

    let dialect = detect_dialect(db_url)?;
    let records = load_records(json_path)?;

    if records.is_empty() {
        return Ok(0);
    }

    let schema = infer_schema(&records);
    let columns: Vec<String> = schema.keys().cloned().collect();

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .with_context(|| {
            if dialect == Dialect::Sqlite && !db_url.contains("mode=") {
                format!(
                    "Failed to connect to database at {db_url}\n\
                     hint: SQLite refuses to create a new file by default. \
                     Add '?mode=rwc' to the end of the URL, e.g. \
                     \"sqlite://data.db?mode=rwc\", to allow creating it."
                )
            } else {
                format!("Failed to connect to database at {db_url}")
            }
        })?;

    if create_table {
        let ddl = build_create_table_sql(table, &schema, dialect);
        sqlx::query(&ddl)
            .execute(&pool)
            .await
            .with_context(|| format!("Failed to create table with: {ddl}"))?;
    }

    let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_ident(table, dialect),
        columns
            .iter()
            .map(|c| quote_ident(c, dialect))
            .collect::<Vec<_>>()
            .join(", "),
        placeholders.join(", ")
    );

    let batch_size = batch_size.max(1);
    let mut inserted = 0usize;
    let mut tx = pool.begin().await?;

    for (i, rec) in records.iter().enumerate() {
        let mut q = sqlx::query(&insert_sql);
        for col in &columns {
            let v = rec.get(col).cloned().unwrap_or(Value::Null);
            q = bind_value(q, &v);
        }
        q.execute(&mut *tx)
            .await
            .with_context(|| format!("Failed inserting record #{i}"))?;
        inserted += 1;

        if inserted % batch_size == 0 {
            tx.commit().await?;
            tx = pool.begin().await?;
        }
    }
    tx.commit().await?;

    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infers_int_then_real_as_real() {
        let records = vec![
            json!({"age": 30}).as_object().unwrap().clone(),
            json!({"age": 30.5}).as_object().unwrap().clone(),
        ];
        let schema = infer_schema(&records);
        assert_eq!(schema["age"], ColType::Real);
    }

    #[test]
    fn null_then_value_uses_value_type() {
        let records = vec![
            json!({"name": null}).as_object().unwrap().clone(),
            json!({"name": "Ada"}).as_object().unwrap().clone(),
        ];
        let schema = infer_schema(&records);
        assert_eq!(schema["name"], ColType::Text);
    }

    #[test]
    fn detects_dialects() {
        assert_eq!(detect_dialect("mysql://localhost/db").unwrap(), Dialect::MySql);
        assert_eq!(detect_dialect("postgres://localhost/db").unwrap(), Dialect::Postgres);
        assert_eq!(detect_dialect("sqlite://data.db").unwrap(), Dialect::Sqlite);
        assert!(detect_dialect("mongodb://localhost/db").is_err());
    }
}