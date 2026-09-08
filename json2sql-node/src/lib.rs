#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::path::PathBuf;

/// Reads a JSON file and inserts its records into MySQL, PostgreSQL, or
/// SQLite, chosen automatically from `db_url`'s scheme.
///
/// Resolves with the number of records inserted.
#[napi]
pub async fn insert_json_to_sql(
    db_url: String,
    json_path: String,
    table: String,
    create_table: bool,
    batch_size: Option<u32>,
) -> Result<u32> {
    let inserted = json2sql::import_json_to_sql(
        &db_url,
        &PathBuf::from(json_path),
        &table,
        create_table,
        batch_size.unwrap_or(500) as usize,
    )
    .await
    .map_err(|e| Error::from_reason(e.to_string()))?;

    Ok(inserted as u32)
}
