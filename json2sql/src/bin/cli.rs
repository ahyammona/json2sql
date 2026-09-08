use clap::Parser;
use std::path::PathBuf;

/// Import a JSON file into MySQL, PostgreSQL, or SQLite.
///
/// Example:
///   json2sql --db-url "sqlite://data.db" --json users.json --table users --create-table
#[derive(Parser)]
#[command(name = "json2sql", version, about)]
struct Args {
    /// Connection string, e.g. mysql://user:pass@host/db, postgres://..., sqlite://file.db
    #[arg(long)]
    db_url: String,

    /// Path to the JSON file to import
    #[arg(long)]
    json: PathBuf,

    /// Target table name
    #[arg(long)]
    table: String,

    /// Create the table (inferring column types) if it doesn't already exist
    #[arg(long, default_value_t = false)]
    create_table: bool,

    /// Number of records to commit per transaction batch
    #[arg(long, default_value_t = 500)]
    batch_size: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let inserted = json2sql::import_json_to_sql(
        &args.db_url,
        &args.json,
        &args.table,
        args.create_table,
        args.batch_size,
    )
    .await?;

    println!("Inserted {inserted} record(s) into '{}'", args.table);
    Ok(())
}
