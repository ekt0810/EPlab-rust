import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import EmbeddedPostgres from "embedded-postgres";
import pg from "pg";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const work = path.join(root, "work", "local-db");
await fs.mkdir(work, { recursive: true });
const configPath = path.join(work, "private.json");
let conf = JSON.parse(
  await fs.readFile(configPath, "utf8").catch(() => "null"),
);
if (!conf) {
  conf = {
    port: Number(process.env.DEV_PG_PORT || 55439),
    password: randomBytes(24).toString("hex"),
  };
  await fs.writeFile(configPath, JSON.stringify(conf), { mode: 0o600 });
}
const databaseDir = path.join(work, "postgres");
const require = createRequire(import.meta.url);
const ctl =
  process.platform === "win32"
    ? path.join(
        root,
        "node_modules",
        "@embedded-postgres",
        "windows-x64",
        "native",
        "bin",
        "pg_ctl.exe",
      )
    : null;
const run = (file, args) =>
  new Promise((resolve, reject) => {
    const child = spawn(file, args, { windowsHide: true, stdio: "ignore" });
    child.once("error", reject);
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(
            new Error("pg_ctl failed; inspect work/local-db/postgres.log"),
          ),
    );
  });
const options = {
  host: "127.0.0.1",
  port: conf.port,
  user: "postgres",
  password: conf.password,
  database: "postgres",
  connectionTimeoutMillis: 3000,
};
if (process.argv[2] === "stop") {
  if (ctl)
    await run(ctl, ["stop", "-D", databaseDir, "-m", "fast", "-w", "-t", "30"]);
  else
    throw new Error(
      "Use pg_ctl stop -D work/local-db/postgres -m fast on this platform.",
    );
  console.log("Stopped the dedicated development database.");
} else {
  const probe = new pg.Client(options);
  let running = false;
  try {
    await probe.connect();
    const actual = (await probe.query("SHOW data_directory")).rows[0]
      .data_directory;
    if (
      path.resolve(actual).toLowerCase() !==
      path.resolve(databaseDir).toLowerCase()
    )
      throw new Error(
        "Port belongs to a different database; choose DEV_PG_PORT.",
      );
    running = true;
  } catch (e) {
    if (e.code !== "ECONNREFUSED") throw e;
  } finally {
    await probe.end().catch(() => {});
  }
  if (!running) {
    const embedded = new EmbeddedPostgres({
      databaseDir,
      user: "postgres",
      password: conf.password,
      port: conf.port,
      persistent: true,
      authMethod: "scram-sha-256",
      initdbFlags: ["--encoding=UTF8", "--locale=C"],
      postgresFlags: [
        "-h",
        "127.0.0.1",
        "-c",
        "max_connections=80",
        "-c",
        "shared_buffers=64MB",
      ],
      onLog: () => {},
      onError: () => {},
    });
    if (
      !(await fs.stat(path.join(databaseDir, "PG_VERSION")).catch(() => null))
    )
      await embedded.initialise();
    if (ctl)
      await run(ctl, [
        "start",
        "-D",
        databaseDir,
        "-l",
        path.join(work, "postgres.log"),
        "-w",
        "-t",
        "30",
        "-o",
        `-p ${conf.port} -h 127.0.0.1 -c max_connections=80 -c shared_buffers=64MB`,
      ]);
    else await embedded.start();
  }
  const client = new pg.Client(options);
  await client.connect();
  if (
    !(await client.query("SELECT 1 FROM pg_roles WHERE rolname='eplab_rust'"))
      .rowCount
  )
    await client.query(
      `CREATE ROLE eplab_rust LOGIN PASSWORD '${conf.password}' NOSUPERUSER NOCREATEDB NOCREATEROLE`,
    );
  for (const name of ["eplab_rust", "eplab_rust_test", "eplab_baseline"]) {
    if (
      !(
        await client.query("SELECT 1 FROM pg_database WHERE datname=$1", [name])
      ).rowCount
    )
      await client.query(`CREATE DATABASE ${name} OWNER eplab_rust`);
  }
  await client.end();
  const env = `DATABASE_URL=postgresql://eplab_rust:${conf.password}@127.0.0.1:${conf.port}/eplab_rust\nAPP_URL=http://127.0.0.1:3200\nBIND_ADDR=127.0.0.1:3200\nDATA_DIR=data\nMAIL_MODE=local\n`;
  const envPath = path.join(root, ".env");
  if (!(await fs.stat(envPath).catch(() => null)))
    await fs.writeFile(envPath, env, { mode: 0o600 });
  console.log(
    "Dedicated database is ready. Private configuration: .env (not committed).",
  );
}
