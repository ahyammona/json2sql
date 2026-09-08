const { insertJsonToSql } = require("./index.js");

async function main() {
  const count = await insertJsonToSql(
    "sqlite://node-test.db?mode=rwc",
    "../example.json",
    "users",
    true,
    500
  );
  console.log(`Inserted ${count} records`);
}

main().catch((err) => {
  console.error("Failed:", err);
  process.exit(1);
});