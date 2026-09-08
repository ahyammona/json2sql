/**
 * Reads a JSON file and inserts its records into MySQL, PostgreSQL, or
 * SQLite, chosen automatically from dbUrl's scheme (mysql://, postgres://,
 * sqlite://).
 *
 * @param dbUrl - e.g. "sqlite://data.db", "mysql://user:pass@host/db"
 * @param jsonPath - path to the JSON file (array of objects, or one object)
 * @param table - target table name
 * @param createTable - if true, CREATE TABLE IF NOT EXISTS with inferred columns
 * @param batchSize - records committed per transaction batch (default 500)
 * @returns the number of records inserted
 */
export function insertJsonToSql(
  dbUrl: string,
  jsonPath: string,
  table: string,
  createTable: boolean,
  batchSize?: number
): Promise<number>;
