import fs from "node:fs/promises";
import path from "node:path";
import net from "node:net";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { fileURLToPath } from "node:url";
import pg from "pg";
export const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
export const binary =
  process.env.EPLAB_BIN ||
  path.join(
    root,
    "target",
    "release",
    process.platform === "win32" ? "eplab-rust.exe" : "eplab-rust",
  );
export async function freePort() {
  const server = net.createServer();
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const p = server.address().port;
  await new Promise((r) => server.close(r));
  return p;
}
export async function startEnvironment(label = "test") {
  const name = "eplab_rust_test_" + randomBytes(6).toString("hex");
  const local = process.env.TEST_DATABASE_URL
    ? null
    : JSON.parse(
        await fs.readFile(
          path.join(root, "work/local-db/private.json"),
          "utf8",
        ),
      );
  const source =
    process.env.TEST_DATABASE_URL ||
    "postgresql://postgres:" +
      local.password +
      "@127.0.0.1:" +
      local.port +
      "/postgres";
  const sourceURL = new URL(source);
  if (!["127.0.0.1", "localhost", "::1"].includes(sourceURL.hostname))
    throw new Error("Tests require a dedicated local PostgreSQL.");
  const admin = new pg.Client({ connectionString: source });
  await admin.connect();
  await admin.query("CREATE DATABASE " + name);
  await admin.end();
  const dbURL = new URL(source);
  dbURL.pathname = "/" + name;
  const data = path.join(root, "work", label + "-" + name);
  await fs.mkdir(data, { recursive: true });
  const port = await freePort();
  const origin = "http://127.0.0.1:" + port;
  const env = {
    ...process.env,
    DATABASE_URL: dbURL.href,
    APP_URL: origin,
    BIND_ADDR: "127.0.0.1:" + port,
    DATA_DIR: data,
    MAIL_MODE: "local",
    RUST_LOG: "eplab_rust=warn",
  };
  const password = randomBytes(24).toString("base64url");
  const adminEmail = "host@example.test";
  const run = async (args, extra = {}) =>
    new Promise((resolve, reject) => {
      let output = "";
      const child = spawn(binary, args, {
        cwd: root,
        env: { ...env, ...extra },
        windowsHide: true,
        stdio: ["ignore", "pipe", "pipe"],
      });
      child.stdout.on("data", (b) => (output += b));
      child.stderr.on("data", (b) => (output += b));
      child.once("error", reject);
      child.once("exit", (code) =>
        code === 0
          ? resolve(output)
          : reject(new Error("CLI " + args.join(" ") + " failed: " + output)),
      );
    });
  await run(["admin"], {
    ADMIN_EMAIL: adminEmail,
    ADMIN_NAME: "Người chủ trì kiểm thử",
    ADMIN_PASSWORD: password,
  });
  const child = spawn(binary, ["serve"], {
    cwd: root,
    env,
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let logs = "";
  child.stdout.on("data", (b) => (logs += b));
  child.stderr.on("data", (b) => (logs += b));
  for (let i = 0; i < 100; i++) {
    if (child.exitCode !== null) throw new Error("Server failed: " + logs);
    try {
      if ((await fetch(origin + "/api/health")).ok) break;
    } catch {}
    if (i === 99) throw new Error("Server start timeout: " + logs);
    await new Promise((r) => setTimeout(r, 100));
  }
  const client = new pg.Client({ connectionString: dbURL.href });
  await client.connect();
  async function mailToken(email) {
    const files = await fs.readdir(path.join(data, "mail"));
    const mails = await Promise.all(
      files.map(async (f) => ({
        ...JSON.parse(await fs.readFile(path.join(data, "mail", f), "utf8")),
        mtime: (await fs.stat(path.join(data, "mail", f))).mtimeMs,
      })),
    );
    mails.sort((a, b) => a.mtime - b.mtime);
    const match = mails.filter((m) => m.to === email).at(-1);
    if (!match) throw new Error("No test mail for " + email);
    return match.text.match(/#token=([a-f0-9]{64})/)[1];
  }
  return {
    origin,
    port,
    env,
    data,
    client,
    password,
    adminEmail,
    run,
    child,
    mailToken,
    async stop() {
      if (child.exitCode === null) {
        child.kill();
        await new Promise((r) => child.once("exit", r));
      }
      await fs.writeFile(path.join(data, "server.log"), logs);
      await client.end();
      const cleanup = new pg.Client({ connectionString: source });
      await cleanup.connect();
      if (!/^eplab_rust_test_[a-f0-9]{12}$/.test(name))
        throw new Error("Unsafe test database name");
      await cleanup.query("DROP DATABASE " + name + " WITH (FORCE)");
      await cleanup.end();
    },
    logs: () => logs,
  };
}
export function api(origin) {
  let cookie = "",
    csrf = "";
  return {
    async call(method, url, body, expected = 200, extra = {}) {
      const headers = { origin, cookie, "x-csrf-token": csrf, ...extra };
      const init = { method, headers };
      if (body !== undefined) {
        if (body instanceof FormData) init.body = body;
        else {
          headers["content-type"] = "application/json";
          init.body = JSON.stringify(body);
        }
      }
      const response = await fetch(origin + url, init);
      const raw = await response.text();
      let data;
      try {
        data = JSON.parse(raw);
      } catch {
        data = raw;
      }
      if (response.status !== expected)
        throw new Error(
          method +
            " " +
            url +
            " expected " +
            expected +
            " got " +
            response.status +
            ": " +
            raw.slice(0, 500),
        );
      const set = response.headers.get("set-cookie");
      if (set) cookie = set.split(";")[0];
      if (data.csrf) csrf = data.csrf;
      return { data, response, raw };
    },
    get cookie() {
      return cookie;
    },
    get csrf() {
      return csrf;
    },
  };
}
export async function createLearner(
  e,
  name = "Học viên kiểm thử",
  suffix = randomBytes(4).toString("hex"),
) {
  const user = api(e.origin);
  const email = "learner-" + suffix + "@example.test";
  await user.call("POST", "/api/auth/register", {
    name,
    email,
    phone: "0900000001",
    password: e.password,
  });
  const token = await e.mailToken(email);
  await user.call("POST", "/api/auth/verify", { token });
  await user.call("POST", "/api/auth/login", { email, password: e.password });
  const me = (await user.call("GET", "/api/auth/me")).data;
  return { user, email, id: me.user.id, token };
}
