# json2sql

Read a JSON file and insert its records into **MySQL**, **PostgreSQL**, or **SQLite** —
one tool, one code path, no per-database code to write. The target database
is auto-detected from the connection string's scheme (`mysql://`,
`postgres://`, `sqlite:`), using `sqlx`'s `Any` driver under the hood.

Two ways to use it:
- **Rust crate + CLI** (`json2sql/`)
- **npm package** (`json2sql-node/`) — a native Node addon built with `napi-rs`,
  same core logic, callable from JavaScript/TypeScript.

> ⚠️ **Honesty note:** this was written offline (no internet access in the
> environment that generated it), so it has **not been compiled or run**.
> The code follows the documented `sqlx` 0.7 / `napi` 2 / `clap` 4 APIs
> carefully, but treat `cargo build` as your first real test, and expect to
> fix minor version-pinning issues.

## What it does

1. Reads your JSON file — either a top-level array of objects, or a single object.
2. Infers a column type per field by scanning all records (integer → real →
   text as it needs to generalize; nested objects/arrays are stored as JSON text).
3. Optionally runs `CREATE TABLE IF NOT EXISTS` with those inferred columns.
4. Inserts every record in batched transactions.

It does **not** attempt relational normalization — nested structures become
a single TEXT column holding JSON. That's a deliberate scope limit, not an
oversight; see "Ideas for extending" below.

## 1. Rust CLI

```bash
cd json2sql
cargo build --release

# SQLite — note the ?mode=rwc, without it sqlx refuses to CREATE a new
# database file and only opens ones that already exist.
./target/release/json2sql \
  --db-url "sqlite://data.db?mode=rwc" \
  --json ../example.json \
  --table users \
  --create-table

# PostgreSQL
./target/release/json2sql \
  --db-url "postgres://user:pass@localhost/mydb" \
  --json ../example.json \
  --table users \
  --create-table

# MySQL
./target/release/json2sql \
  --db-url "mysql://user:pass@localhost/mydb" \
  --json ../example.json \
  --table users \
  --create-table
```

Flags:
- `--db-url` — connection string (required)
- `--json` — path to the JSON file (required)
- `--table` — target table name (required)
- `--create-table` — create the table first if it's missing (optional flag)
- `--batch-size` — records per commit, default 500 (optional)

Run tests with `cargo test` (covers schema inference and dialect detection;
no live database needed for those).

## 2. Node.js / npm package

```bash
cd json2sql-node
npm install
npm run build   # compiles the Rust addon for your current platform via napi-rs
```

Usage:

```js
const { insertJsonToSql } = require("json2sql");

const count = await insertJsonToSql(
  "sqlite://data.db",   // or mysql://... / postgres://...
  "./example.json",
  "users",
  true,                 // create table if missing
  500                   // batch size (optional)
);

console.log(`Inserted ${count} records`);
```

TypeScript types are in `index.d.ts`.

### Publishing

- **crates.io**: from `json2sql/`, `cargo publish` (after `cargo login`).
- **npm**: `napi-rs` needs a build per target platform (Linux/macOS/Windows,
  x64/arm64). The typical setup is a GitHub Actions matrix that runs
  `napi build --platform --release` on each OS/arch and publishes the
  resulting `.node` binaries alongside the JS wrapper. `napi new` can
  scaffold that CI config if you start a fresh repo with `@napi-rs/cli`.

## Project layout

```
json2sql-project/
├── Cargo.toml              # workspace
├── example.json            # sample data to test with
├── json2sql/                # core crate + CLI
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           # schema inference, dialect detection, import logic
│       └── bin/cli.rs        # clap-based CLI
└── json2sql-node/            # Node.js bindings
    ├── Cargo.toml
    ├── build.rs
    ├── package.json
    ├── index.d.ts
    └── src/lib.rs             # napi-rs wrapper around the core crate
```

## Ideas for extending (the "is this an idea worth building on" part)

- **Relational normalization**: detect arrays of nested objects and spin up
  child tables with a foreign key, instead of flattening to JSON text.
- **Upsert mode**: `INSERT ... ON CONFLICT` / `ON DUPLICATE KEY UPDATE` for
  re-running imports without duplicating rows.
- **Schema diffing**: on subsequent runs, `ALTER TABLE` to add new columns
  the JSON has grown instead of failing.
- **Streaming large files**: switch to a streaming JSON parser so multi-GB
  files don't need to load fully into memory.
- **Sync mode**: watch a JSON file/endpoint and keep the table up to date,
  rather than a one-shot import.