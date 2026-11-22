import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import http from "node:http";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomBytes, randomUUID, createHash } from "node:crypto";
import { chromium } from "playwright";
import pg from "pg";
import assert from "node:assert/strict";
import { startEnvironment, freePort, root } from "../tests/environment.mjs";
const exec = promisify(execFile);
const baselineRoot = process.env.BASELINE_ROOT;
if (!baselineRoot)
  throw new Error(
    "Set BASELINE_ROOT to a clean, production-built EPlab TypeScript checkout. Build Next with EP_API_URL=http://127.0.0.1:4201.",
  );
const rust = await startEnvironment("benchmark");
let baseline, web, baselineClient, baselineName, baselineWebOrigin;
const count = Number(process.env.BENCH_REQUESTS || 600),
  rounds = 3;
const result = {
  date: new Date().toISOString(),
  hardware: {
    os: os.platform() + " " + os.release(),
    cpu: os.cpus()[0].model,
    logicalCpus: os.cpus().length,
    ramGiB: os.totalmem() / 2 ** 30,
  },
  versions: { node: process.version },
  method: {
    requestsPerSample: count,
    rounds,
    concurrency: [1, 8, 32],
    databasePool: 8,
    identities: 1000,
    compression: "identity (both API servers)",
    auth: "Real database sessions; all production permission, CSRF and rate controls retained.",
    timing:
      "Loopback, completed body, JSON validation, keep-alive. Alternate implementation order each round.",
    limitations: [
      "Different application architectures and payload shapes; this is not an isolated language benchmark.",
      "Same curated 100 lessons, 36 exercises and synthetic learners. Rust supports editable classroom lessons in PostgreSQL; baseline classroom content is compiled TypeScript.",
      "No external email, AI, video service or internet latency is measured.",
      "Working set excludes PostgreSQL and benchmark driver. Browser totals report API + Next for TypeScript and one Rust process.",
    ],
  },
  samples: [],
  browser: [],
  login: [],
};
async function runChild(file, args, env, cwd) {
  const child = spawn(file, args, {
    cwd,
    env,
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.logs = "";
  child.stdout.on("data", (d) => (child.logs += d));
  child.stderr.on("data", (d) => (child.logs += d));
  return child;
}
async function waitHealthy(url, child) {
  for (let i = 0; i < 300; i++) {
    if (child.exitCode !== null)
      throw new Error("Baseline exited: " + child.logs);
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("Health timeout: " + child.logs);
}
async function sampleProcess(ids) {
  const { stdout } = await exec(
    process.env.PYTHON || "python",
    [path.join(root, "scripts/process-metrics.py"), ...ids.map(String)],
    { windowsHide: true },
  );
  return JSON.parse(stdout);
}
const hash = (s) => createHash("sha256").update(s).digest("hex");
const identities = Array.from({ length: 1000 }, (_, i) => ({
  id: randomUUID(),
  email: "bench-" + String(i).padStart(4, "0") + "@example.test",
  name: "Benchmark learner " + i,
  sid: randomBytes(32).toString("hex"),
  csrf: randomBytes(32).toString("hex"),
}));
const catalog = JSON.parse(
  await fs.readFile(path.join(root, "content/courses.json"), "utf8"),
);
const first = catalog.embedded.lessons[0];
const workloads = [
  {
    name: "course-index",
    path: "/api/classroom",
    valid: (d) =>
      Array.isArray(d) && d.reduce((n, c) => n + c.lessonCount, 0) === 100,
  },
  {
    name: "course-detail",
    path: "/api/classroom/embedded",
    valid: (d) => d.lessons?.length === 32,
  },
  {
    name: "lesson-detail",
    path: "/api/classroom/embedded/lessons/" + first.id,
    valid: (d) => d.lesson?.title === first.title && d.progress?.scene === 1,
  },
  {
    name: "personal-progress",
    path: "/api/classroom/progress",
    valid: (d) => Array.isArray(d) && d.length === 3,
  },
];
const agents = {
  rust: new http.Agent({ keepAlive: true, maxSockets: 64 }),
  typescript: new http.Agent({ keepAlive: true, maxSockets: 64 }),
};
function request(base, route, identity, agent, method = "GET", payload) {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const headers = {
      cookie: "ep_session=" + identity.sid,
      origin: agent === agents.typescript ? baselineWebOrigin : base,
      "x-csrf-token": identity.csrf,
      "accept-encoding": "identity",
    };
    let body;
    if (payload) {
      body = Buffer.from(JSON.stringify(payload));
      headers["content-type"] = "application/json";
      headers["content-length"] = body.length;
    }
    const req = http.request(
      new URL(route, base),
      { agent, method, headers },
      (res) => {
        const chunks = [];
        res.on("data", (b) => chunks.push(b));
        res.on("end", () => {
          const buffer = Buffer.concat(chunks);
          let data;
          try {
            data = JSON.parse(buffer.toString());
          } catch {
            data = null;
          }
          resolve({
            status: res.statusCode,
            ms: performance.now() - start,
            bytes: buffer.length,
            data,
          });
        });
      },
    );
    req.setTimeout(15000, () => req.destroy(new Error("Request timeout")));
    req.on("error", reject);
    if (body) req.write(body);
    req.end();
  });
}
const percentile = (values, q) =>
  [...values].sort((a, b) => a - b)[
    Math.min(values.length - 1, Math.ceil(values.length * q) - 1)
  ];
async function load(impl, base, workload, concurrency, n) {
  let next = 0;
  const times = [],
    sizes = [],
    errors = [];
  const start = performance.now();
  await Promise.all(
    Array.from({ length: concurrency }, async () => {
      while (true) {
        const index = next++;
        if (index >= n) break;
        const r = await request(
          base,
          workload.path,
          identities[index % identities.length],
          agents[impl],
        );
        if (r.status !== 200 || !workload.valid(r.data))
          errors.push({
            status: r.status,
            message: r.data?.error || "invalid body",
          });
        times.push(r.ms);
        sizes.push(r.bytes);
      }
    }),
  );
  const elapsed = performance.now() - start;
  if (errors.length)
    throw new Error(
      impl +
        " " +
        workload.name +
        " invalid responses: " +
        JSON.stringify(errors.slice(0, 3)),
    );
  return {
    requests: n,
    errors: errors.length,
    elapsedMs: elapsed,
    rps: n / (elapsed / 1000),
    p50Ms: percentile(times, 0.5),
    p95Ms: percentile(times, 0.95),
    p99Ms: percentile(times, 0.99),
    meanBodyBytes: sizes.reduce((a, b) => a + b, 0) / n,
  };
}
try {
  const source = new URL(rust.env.DATABASE_URL);
  source.pathname = "/postgres";
  const admin = new pg.Client({ connectionString: source.href });
  await admin.connect();
  baselineName = "eplab_node_test_" + randomBytes(6).toString("hex");
  await admin.query("CREATE DATABASE " + baselineName);
  await admin.end();
  const dbURL = new URL(source);
  dbURL.pathname = "/" + baselineName;
  const webPort = await freePort(),
    webOrigin = "http://127.0.0.1:" + webPort;
  baselineWebOrigin = webOrigin;
  const baselineEnv = {
    ...process.env,
    BASELINE_ROOT: baselineRoot,
    DATABASE_URL: dbURL.href,
    DATA_DIR: path.join(rust.data, "typescript"),
    APP_URL: webOrigin,
    API_PORT: "4201",
    API_HOST: "127.0.0.1",
    SECRET_KEY: randomBytes(32).toString("hex"),
    MAIL_MODE: "local",
    PUBLIC_MODE: "false",
    TRUST_PROXY: "false",
    LOG_LEVEL: "silent",
    EP_API_URL: "http://127.0.0.1:4201",
    NODE_ENV: "production",
  };
  baseline = await runChild(
    process.execPath,
    [path.join(root, "scripts/baseline-server.mjs")],
    baselineEnv,
    root,
  );
  await waitHealthy("http://127.0.0.1:4201/api/health", baseline);
  baselineClient = new pg.Client({ connectionString: dbURL.href });
  await baselineClient.connect();
  const pw = (
    await rust.client.query(
      "SELECT password_hash FROM users WHERE role='admin'",
    )
  ).rows[0].password_hash;
  const records = JSON.stringify(
    identities.map((u) => ({ ...u, sid: hash(u.sid) })),
  );
  for (const client of [rust.client, baselineClient]) {
    await client.query(
      "INSERT INTO users(id,email,name,phone,password_hash,verified) SELECT id,email,name,'0900000000',$2,true FROM jsonb_to_recordset($1::jsonb) AS u(id uuid,email text,name text)",
      [records, pw],
    );
    await client.query(
      "INSERT INTO lab_memberships(user_id,status,tracks,message,reviewed_at) SELECT id,'approved',ARRAY['embedded','kicad','solidworks'],'Synthetic benchmark membership',now() FROM jsonb_to_recordset($1::jsonb) AS u(id uuid)",
      [records],
    );
    await client.query(
      "INSERT INTO sessions(id,user_id,csrf,user_agent,expires_at) SELECT sid,id,csrf,'benchmark',now()+interval '1 day' FROM jsonb_to_recordset($1::jsonb) AS u(id uuid,sid text,csrf text)",
      [records],
    );
    for (const track of ["embedded", "kicad", "solidworks"])
      await client.query(
        "INSERT INTO classroom_progress(user_id,track,lesson_id,scene,note) SELECT id,$2,$3,1,'Synthetic benchmark note' FROM jsonb_to_recordset($1::jsonb) AS u(id uuid)",
        [records, track, catalog[track].lessons[0].id],
      );
  }
  for (const client of [rust.client, baselineClient])
    await client.query("ANALYZE");
  result.versions.rustBinarySha256 = hash(
    await fs.readFile(
      process.env.EPLAB_BIN ||
        path.join(
          root,
          "target/release",
          process.platform === "win32" ? "eplab-rust.exe" : "eplab-rust",
        ),
    ),
  );
  result.versions.postgres = (
    await rust.client.query("SHOW server_version")
  ).rows[0].server_version;
  result.versions.baselineCommit = (
    await exec("git", ["rev-parse", "HEAD"], { cwd: baselineRoot })
  ).stdout.trim();
  result.versions.fastify = JSON.parse(
    await fs.readFile(
      path.join(baselineRoot, "node_modules/fastify/package.json"),
      "utf8",
    ),
  ).version;
  result.versions.next = JSON.parse(
    await fs.readFile(
      path.join(baselineRoot, "node_modules/next/package.json"),
      "utf8",
    ),
  ).version;
  result.versions.rust = (
    await exec("rustc", ["--version"], { env: process.env })
  ).stdout.trim();
  const origins = { rust: rust.origin, typescript: "http://127.0.0.1:4201" },
    pids = { rust: rust.child.pid, typescript: baseline.pid };
  console.log(
    "Both production API servers ready with 1,000 identical synthetic learners. Warming...",
  );
  for (const w of workloads)
    for (const impl of ["rust", "typescript"])
      await load(impl, origins[impl], w, 8, 100);
  for (let round = 1; round <= rounds; round++) {
    for (const workload of [...workloads].sort((a, b) =>
      round % 2 ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name),
    )) {
      for (const concurrency of [1, 8, 32]) {
        for (const impl of round % 2
          ? ["typescript", "rust"]
          : ["rust", "typescript"]) {
          const before = (await sampleProcess([pids[impl]]))[0];
          const measured = await load(
            impl,
            origins[impl],
            workload,
            concurrency,
            count,
          );
          const after = (await sampleProcess([pids[impl]]))[0];
          const row = {
            implementation: impl,
            workload: workload.name,
            round,
            concurrency,
            ...measured,
            cpuMs: after.cpuMs - before.cpuMs,
            rssMiB: after.rssMiB,
            privateMiB: after.privateMiB,
          };
          result.samples.push(row);
          console.log(
            impl +
              " " +
              workload.name +
              " c" +
              concurrency +
              " r" +
              round +
              ": " +
              measured.rps.toFixed(0) +
              " req/s; p95 " +
              measured.p95Ms.toFixed(2) +
              " ms",
          );
        }
      }
    }
  }
  // 16 real password verifications per implementation, below the unchanged 20/15-minute route quota.
  for (const impl of ["typescript", "rust"]) {
    for (const concurrency of [1, 8]) {
      let index = 0;
      const values = [];
      await Promise.all(
        Array.from({ length: concurrency }, async () => {
          while (index < 8) {
            const i = index++;
            const identity = identities[900 + i];
            const r = await request(
              origins[impl],
              "/api/auth/login",
              { sid: "", csrf: "" },
              agents[impl],
              "POST",
              { email: identity.email, password: rust.password },
            );
            assert.equal(r.status, 200, JSON.stringify(r.data));
            values.push(r.ms);
          }
        }),
      );
      result.login.push({
        implementation: impl,
        concurrency,
        samples: values.length,
        p50Ms: percentile(values, 0.5),
        p95Ms: percentile(values, 0.95),
      });
    }
  }
  web = await runChild(
    process.execPath,
    [
      path.join(baselineRoot, "node_modules/next/dist/bin/next"),
      "start",
      "--hostname",
      "127.0.0.1",
      "--port",
      String(webPort),
    ],
    baselineEnv,
    path.join(baselineRoot, "apps/web"),
  );
  await waitHealthy(webOrigin, web);
  const browser = await chromium.launch({
    headless: true,
    channel: process.platform === "win32" ? "msedge" : undefined,
  });
  result.versions.browser = browser.version();
  try {
    for (let round = 0; round < 6; round++) {
      for (const impl of round % 2
        ? ["typescript", "rust"]
        : ["rust", "typescript"]) {
        const origin = impl === "rust" ? rust.origin : webOrigin;
        const context = await browser.newContext({
          viewport: { width: 1365, height: 900 },
        });
        await context.addCookies([
          {
            name: "ep_session",
            value: identities[700 + round].sid,
            url: origin,
            httpOnly: true,
            sameSite: "Lax",
          },
        ]);
        await context.route("**/*", (route) =>
          new URL(route.request().url()).origin === origin
            ? route.continue()
            : route.abort(),
        );
        const page = await context.newPage();
        const errors = [];
        page.on("pageerror", (err) => errors.push(err.message));
        const start = performance.now();
        const response = await page.goto(
          origin +
            (impl === "rust" ? "/lop-hoc/embedded/" : "/nhung-c/lop-hoc/") +
            first.id,
        );
        assert.equal(response.status(), 200);
        await page
          .locator(impl === "rust" ? "main h1" : ".cr-room-header")
          .filter({ hasText: first.title })
          .waitFor();
        const readyMs = performance.now() - start;
        await page.waitForLoadState("networkidle");
        assert.equal(errors.length, 0, errors.join("\\n"));
        const entries = await page.evaluate(() => ({
          navigation: performance.getEntriesByType("navigation")[0].toJSON(),
          resources: performance
            .getEntriesByType("resource")
            .map((e) => e.toJSON()),
          paint: performance
            .getEntriesByType("paint")
            .map((e) => ({ name: e.name, startTime: e.startTime })),
        }));
        if (round > 0) {
          const same = entries.resources.filter(
            (e) => new URL(e.name).origin === origin,
          );
          result.browser.push({
            implementation: impl,
            round,
            lessonVisibleMs: readyMs,
            ttfbMs: entries.navigation.responseStart,
            domContentLoadedMs: entries.navigation.domContentLoadedEventEnd,
            transferBytes:
              entries.navigation.transferSize +
              same.reduce((n, e) => n + e.transferSize, 0),
            decodedBytes:
              entries.navigation.decodedBodySize +
              same.reduce((n, e) => n + e.decodedBodySize, 0),
            jsDecodedBytes: same
              .filter((e) => e.name.includes(".js"))
              .reduce((n, e) => n + e.decodedBodySize, 0),
            requests: 1 + same.length,
            firstContentfulPaintMs: entries.paint.find(
              (e) => e.name === "first-contentful-paint",
            )?.startTime,
          });
        }
        await context.close();
      }
    }
  } finally {
    await browser.close();
  }
  const memory = await sampleProcess([rust.child.pid, baseline.pid, web.pid]);
  result.wholeApplicationMemory = {
    rustRssMiB: memory[0].rssMiB,
    typescriptApiRssMiB: memory[1].rssMiB,
    typescriptWebRssMiB: memory[2].rssMiB,
    typescriptTotalRssMiB: memory[1].rssMiB + memory[2].rssMiB,
  };
  await fs.mkdir(path.join(root, "docs"), { recursive: true });
  await fs.writeFile(
    path.join(root, "docs/benchmark-results.json"),
    JSON.stringify(result, null, 2),
  );
  console.log(
    "Saved docs/benchmark-results.json. All measured requests had valid successful responses.",
  );
} finally {
  for (const agent of Object.values(agents)) agent.destroy();
  for (const child of [web, baseline])
    if (child && child.exitCode === null) {
      child.kill();
      await new Promise((r) => child.once("exit", r));
    }
  if (baselineClient) await baselineClient.end();
  if (baselineName) {
    const source = new URL(rust.env.DATABASE_URL);
    source.pathname = "/postgres";
    const admin = new pg.Client({ connectionString: source.href });
    await admin.connect();
    assert.match(baselineName, /^eplab_node_test_[a-f0-9]{12}$/);
    await admin.query("DROP DATABASE " + baselineName + " WITH (FORCE)");
    await admin.end();
  }
  await rust.stop();
}
