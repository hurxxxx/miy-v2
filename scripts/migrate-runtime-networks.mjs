import { checkedRuntimeBody, collectApiPage } from "./lib/cli-workflow.mjs";
// Redeploy explicit existing installations unchanged to the current network policy.
// Data is preserved; each selected app may briefly stop during its redeployment.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
const apps = process.argv.slice(2);
if (!apps.length)
  throw Error(
    "Pass explicit app IDs to redeploy; no implicit all-app mutation.",
  );
const origin = "http://localhost:18000";
const response = await fetch(origin + "/auth/development", {
  method: "POST",
  headers: { origin, "content-type": "application/json" },
  body: JSON.stringify({ account: "admin" }),
});
assert(response.ok);
const cookie = response.headers.get("set-cookie").split(";")[0];
const me = await (
  await fetch(origin + "/api/v1/me", { headers: { cookie } })
).json();
async function api(path, body) {
  body = await checkedRuntimeBody(api, path, body);
  const r = await fetch(origin + path, {
    method: body ? "POST" : "GET",
    headers: {
      cookie,
      origin,
      "x-miy-csrf": me.csrf,
      "content-type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const value = await r.json();
  assert(r.ok, value.error?.code);
  return body ? value : collectApiPage(value, path, async next => {
    const response = await fetch(origin + next, { headers: { cookie } });
    assert(response.ok);
    return response.json();
  });
}
function sql(db, query) {
  return execFileSync(
    "docker",
    [
      "exec",
      "miy-v2-validation-postgres-1",
      "psql",
      "-U",
      "postgres",
      "-d",
      db,
      "-At",
      "-c",
      query,
    ],
    { encoding: "utf8" },
  ).trim();
}
function snapshot(inst) {
  const db = "miy_app_" + inst.id.replaceAll("-", "");
  assert(/^miy_app_[a-f0-9]{32}$/.test(db));
  const tables = sql(
    db,
    "SELECT tablename FROM pg_tables WHERE schemaname='app' ORDER BY tablename",
  )
    .split("\n")
    .filter(Boolean);
  const counts = Object.fromEntries(
    tables.map((t) => [
      t,
      sql(db, 'SELECT count(*) FROM app."' + t.replaceAll('"', '""') + '"'),
    ]),
  );
  const history = sql(
    db,
    "SELECT version||':'||success||':'||encode(checksum,'hex') FROM app._sqlx_migrations ORDER BY version",
  );
  return { image: inst.imageId, release: inst.releaseId, counts, history };
}
const pid = readFileSync(".local/serve.pid", "utf8").trim();
const results = [];
for (const app of apps) {
  const inst = (await api("/api/v1/installations")).find(
    (i) =>
      i.appId === app && i.environment === "dev" && i.kind === "docker_app",
  );
  assert(inst, "Missing dev Docker app " + app);
  const before = snapshot(inst);
  const job = await api(`/api/v1/apps/${app}/runtime`, {
    action: "deploy",
    environment: "dev",
    imageId: inst.imageId,
    idempotencyKey: randomUUID(),
  });
  let status;
  for (let i = 0; i < 400; i++) {
    status = await api(`/api/v1/runtime/jobs/${job.id}`);
    if (["succeeded", "failed"].includes(status.status)) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  assert.equal(status.status, "succeeded", status.errorCode);
  const after = (await api("/api/v1/installations")).find(
    (i) => i.id === inst.id,
  );
  assert.equal(after.runtimeStatus, "running");
  assert.deepEqual(snapshot(after), before);
  const network = JSON.parse(
    execFileSync(
      "docker",
      [
        "inspect",
        "--format",
        "{{json .NetworkSettings.Networks}}",
        "miy-app-" + inst.id.replaceAll("-", ""),
      ],
      { encoding: "utf8" },
    ),
  );
  assert.deepEqual(Object.keys(network), [
    "miy-runtime-" + inst.id.replaceAll("-", ""),
  ]);
  results.push({
    app,
    installation: inst.id,
    image: inst.imageId,
    job: job.id,
    dataPreserved: true,
    network: Object.keys(network)[0],
  });
  console.log(
    "PASS " +
      app +
      ": same image/release, exact row counts and migration history retained; isolated network",
  );
}
assert.equal(readFileSync(".local/serve.pid", "utf8").trim(), pid);
writeFileSync(
  ".local/evidence/structural-audit/existing-networks.json",
  JSON.stringify({ passed: results, corePidUnchanged: true }, null, 2),
);
