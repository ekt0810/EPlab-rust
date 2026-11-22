import { pathToFileURL } from "node:url";
import path from "node:path";
const root = process.env.BASELINE_ROOT;
if (!root) throw new Error("Set BASELINE_ROOT to the original EPlab checkout.");
const load = (rel) =>
  import(pathToFileURL(path.join(root, "apps/api/dist", rel)).href);
const { migrate } = await load("migrate.js");
await migrate();
const { seed } = await load("seed.js");
await seed();
const { buildApp } = await load("server.js");
const app = await buildApp();
await app.listen({
  host: "127.0.0.1",
  port: Number(process.env.API_PORT || 4201),
});
console.log("Production Fastify baseline ready.");
for (const signal of ["SIGINT", "SIGTERM"])
  process.on(signal, async () => {
    await app.close();
    const { pool } = await load("db.js");
    await pool.end();
    process.exit(0);
  });
