# json2sql
Read a JSON file and insert its records into **MySQL**, **PostgreSQL**, or **SQLite** — one tool, one code path, no per-database code to write. The target database is auto-detected from the connection string's scheme (`mysql://`, `postgres://`, `sqlite:`), using `sqlx`'s `Any` driver under the hood.
