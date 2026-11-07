// Focused security characterization, not a regression declaring the current boundary safe.
// Run from repository root after `cargo build --locked`. No production configuration is read.
import { spawn, execFileSync } from "node:child_process";
import { createCipheriv, createHmac, randomBytes, randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { once } from "node:events";
import { chromium } from "@playwright/test";

const root = process.cwd();
const run = resolve(root, ".local/evidence/browser-boundary-review-" + randomUUID());
mkdirSync(run, { recursive: true });
const key = randomBytes(32);
const gatewaySecret = randomBytes(32).toString("hex");
const baseline = () => execFileSync("docker", ["ps", "--format", "{{.ID}} {{.Names}} {{.Image}}"], { encoding: "utf8" }).trim().split("\n").sort();
const before = baseline();
let harness = readFileSync(join(root, "scripts/runtime-regression/server-ui.mjs"), "utf8");
for (const [from, to] of [
  ['const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");', `const root = ${JSON.stringify(root)};`],
  ['const evidence = resolve(root, ".local/evidence/server-ui");', `const evidence = ${JSON.stringify(join(run, "harness"))};`],
  ["const UI_PORT = 18148;", "const UI_PORT = 18448;"],
  ["const API_PORT = 18149;", "const API_PORT = 18449;"],
  ["const ISSUER_PORT = 18150;", "const ISSUER_PORT = 18450;"],
  ['MIY_ENCRYPTION_KEY: randomBytes(32).toString("hex"),', "MIY_ENCRYPTION_KEY: process.env.MIY_PROBE_ENCRYPTION_KEY,"],
]) {
  if (!harness.includes(from)) throw new Error("harness_source_changed");
  harness = harness.replace(from, to);
}
const harnessPath = join(run, "harness.mjs");
writeFileSync(harnessPath, harness);
const origin = "http://127.0.0.1:18448";
const observations = [];
const observe = (name, actual, expected) => {
  const status = JSON.stringify(actual) === JSON.stringify(expected) ? "PASS" : "FAIL";
  observations.push({ name, status, actual, expected });
  if (status === "FAIL") throw new Error("observation_failed:" + name);
};
const child = spawn(process.execPath, [harnessPath], {
  cwd: root,
  env: { ...process.env, MIY_PROBE_ENCRYPTION_KEY: key.toString("hex"), MIY_STATE_DIR: join(run, "state"), MIY_DATABASE_CONTAINER: "miy-browser-boundary-no-worker" },
  stdio: "ignore",
});
let browser;
let backend;
let observer;
let cookieArrived = false;
let backendCalls = 0;
let failure;
try {
  let ready = false;
  for (let i = 0; i < 600; i++) {
    if (child.exitCode !== null) throw new Error("isolated_harness_start_failed");
    try { ready = (await fetch(origin + "/auth/options")).ok; } catch {}
    if (ready) break;
    await new Promise((r) => setTimeout(r, 300));
  }
  if (!ready) throw new Error("isolated_harness_start_timeout");
  const env = JSON.parse(readFileSync(join(run, "harness/environment.json"), "utf8"));
  if (!/^miy-server-ui-\d+-[a-f0-9]+$/.test(env.database)) throw new Error("invalid_fixture_database");
  const owner = execFileSync("docker", ["inspect", "-f", '{{ index .Config.Labels "miy.server-ui" }}', env.database], { encoding: "utf8" }).trim();
  if (env.database !== "miy-server-ui-" + owner) throw new Error("fixture_database_owner_mismatch");
  function sql(text) {
    execFileSync("docker", ["exec", "-i", env.database, "psql", "-U", "postgres", "-d", "miy_ui_core", "-v", "ON_ERROR_STOP=1"], { input: text, stdio: ["pipe", "ignore", "pipe"] });
  }
  backend = createServer((req, res) => {
    const token = req.headers["x-miy-identity"];
    const parts = typeof token === "string" ? token.split(".") : [];
    const verified = parts.length === 3 && createHmac("sha256", gatewaySecret).update(parts[0] + "." + parts[1]).digest("base64url") === parts[2];
    backendCalls++;
    // Synthetic business ACL denies this resource, even after the core admitted the user.
    res.writeHead(verified ? (req.url === "/api/denied" ? 403 : 200) : 401, { "content-type": "application/json" });
    res.end(JSON.stringify({ verified, synthetic: true }));
  });
  backend.listen(0, "127.0.0.1"); await once(backend, "listening");
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, nonce);
  const encrypted = Buffer.concat([nonce, cipher.update(gatewaySecret), cipher.final(), cipher.getAuthTag()]).toString("base64url");
  const a = randomUUID(), b = randomUUID();
  for (const [app, id] of [["boundary-a", a], ["boundary-b", b]]) {
    const release = randomUUID();
    const manifest = { contractVersion: "miy.app/v1alpha1", id: app, name: app, version: "1.0.0", runtime: { profile: "rust-web-v1" }, ui: { delivery: "module", moduleEntry: "mf-manifest.json", moduleExport: "./AppRoot", contractVersion: "miy.ui/v1alpha1" }, resources: { database: "postgresql" }, migrations: { directory: "migrations" }, requestedPermissions: ["database", "app:boundary-b:read"] };
    sql(`INSERT INTO apps(id,name) VALUES('${app}','${app}');
      INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES('${release}','${app}','1.0.0','${"a".repeat(64)}','${JSON.stringify(manifest)}');
      INSERT INTO installations(id,app_id,environment,release_id,runtime_status,desired_state,runtime_port,runtime_secret) VALUES('${id}','${app}','dev','${release}','running','running',${backend.address().port},'${encrypted}');
      INSERT INTO grants(user_id,app_id,environment,permission) VALUES('00000000-0000-4000-8000-000000000003','${app}','dev','use');`);
  }
  browser = await chromium.launch({ headless: true });
  async function session(account) {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto(origin);
    await page.evaluate(async (account) => {
      const r = await fetch("/auth/development", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ account }) });
      if (!r.ok) throw new Error("fixture_login_failed");
    }, account);
    return page;
  }
  async function createApp(page, id, csrf) {
    return page.evaluate(async ({ id, csrf }) => {
      const me = await (await fetch("/api/v1/me")).json();
      const response = await fetch("/api/v1/apps", { method: "POST", headers: { "content-type": "application/json", ...(csrf ? { "x-miy-csrf": me.csrf } : {}) }, body: JSON.stringify({ id, name: "Synthetic origin boundary probe" }) });
      return response.status;
    }, { id, csrf });
  }
  const admin = await session("admin");
  observe("admin_same_realm_without_csrf_denied", await createApp(admin, "boundary-no-csrf", false), 403);
  observe("admin_same_realm_script_can_read_csrf_and_mutate", await createApp(admin, "boundary-admin-script", true), 200);
  const alice = await session("alice");
  observe("ordinary_user_same_realm_admin_mutation_denied", await createApp(alice, "boundary-alice-script", true), 403);
  const status = (page, path) => page.evaluate(async (path) => (await fetch(path)).status, path);
  observe("a_connection_without_installation_approval_denied", await status(alice, `/api/v1/installations/${a}/connections/boundary-b/api/probe`), 403);
  observe("direct_b_request_bypasses_a_approval_with_user_b_grant", await status(alice, `/api/v1/installations/${b}/api/probe`), 200);
  observe("direct_b_request_still_obeys_synthetic_business_acl", await status(alice, `/api/v1/installations/${b}/api/denied`), 403);
  sql("DELETE FROM grants WHERE user_id='00000000-0000-4000-8000-000000000003' AND app_id='boundary-b';");
  const callsBefore = backendCalls;
  observe("direct_b_request_after_user_b_grant_revoked_denied", await status(alice, `/api/v1/installations/${b}/api/probe`), 403);
  observe("revoked_request_did_not_reach_backend", backendCalls, callsBefore);
  observer = createServer((req, res) => {
    cookieArrived = /(?:^|;\s*)miy_development=/.test(req.headers.cookie ?? "");
    res.end("synthetic observer");
  });
  observer.listen(0, "127.0.0.1"); await once(observer, "listening");
  await admin.evaluate(async (url) => { try { await fetch(url, { credentials: "include" }); } catch {} }, `http://127.0.0.1:${observer.address().port}/probe`);
  observe("cookie_sent_to_same_host_different_port", cookieArrived, true);
} catch (error) {
  // Never include subprocess arguments, SQL, session data or headers in evidence.
  failure = error.message.startsWith("observation_failed:") ? error.message : "probe_execution_failed";
} finally {
  await browser?.close();
  backend?.closeAllConnections(); backend?.close();
  observer?.closeAllConnections(); observer?.close();
  if (child.exitCode === null) {
    const exited = once(child, "exit"); child.kill("SIGTERM");
    await Promise.race([exited, new Promise((r) => setTimeout(r, 20000))]);
  }
}
const cleanupSame = JSON.stringify(baseline()) === JSON.stringify(before);
const result = { sourceCommit: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(), result: !failure && cleanupSame ? "PASS" : "FAIL", meaning: "PASS confirms the recorded capabilities and limitations; it does not declare browser isolation safe.", fixture: "Chromium same-page page.evaluate + actual core HTTP + disposable PostgreSQL + signed-identity synthetic app backend; loopback development login", observations, productionTouched: false, runningContainerInventoryUnchanged: cleanupSame, notRun: ["real Module Federation artifact installation and execution chain", "real OIDC/Keycloak login", "production browser or server", "proposed split-origin implementation"], ...(failure ? { failure } : {}) };
writeFileSync(join(run, "result.json"), JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify({ evidence: join(run, "result.json"), ...result }, null, 2));
if (result.result !== "PASS") process.exitCode = 1;
