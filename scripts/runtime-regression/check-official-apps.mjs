// Run after stack-ui.sh up. Uses the disposable stack, its own CLI context and
// a synthetic planner in an independent Git repository, including two releases.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cliData } from "../lib/cli-result.mjs";
import { fixtureEnv } from "../lib/cli-workflow.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const state = join(root, ".local/stack-ui");
const db = "miy-stack-ui-postgres-1";
const origin = "http://localhost:18300";
const read = (path) => JSON.parse(readFileSync(path, "utf8"));
const write = (path, data) => writeFileSync(path, JSON.stringify(data, null, 2) + "\n");
assert.equal(readFileSync(join(state, ".stack-ui-owner"), "utf8").split("\n")[0], root);
assert.equal(read(join(state, "server.json")).origin, origin);
assert.equal(execFileSync("docker", ["inspect", "--format", '{{index .Config.Labels "com.docker.compose.project"}}', db], {encoding:"utf8"}).trim(), "miy-stack-ui");
const work = mkdtempSync(join(state, "official-"));
const env = fixtureEnv(work, { MIY_DATABASE_CONTAINER: db, MIY_STATE_DIR: state, CI: "true" });
const binary = join(root, "target/debug/miy");
env.MIY_BINARY = binary;
const product = "ui5-planner" + basename(work).slice(-6).toLowerCase();
const project = join(work, product);
const catalogFile = join(work, "catalog.json");
const catalog = { schemaVersion: 1, apps: [{ appId: product, name: "격리 플래너", defaultInstall: true, releases: [] }] };
const context = "catalog-local";
function run(command, args, cwd = root, environment = env) {
  try {
    return execFileSync(command, args, { cwd, env: environment, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  } catch (error) {
    let code;
    try { code = JSON.parse(error.stdout).error?.code; } catch { /* operator helper uses stderr */ }
    const safe = error.stderr?.match(/(?:state_changed|[a-z_]+_(?:required|mismatch|failed|unavailable|denied))\b/)?.[0];
    throw new Error(`${basename(command)} failed (${code ?? safe ?? error.status})`);
  }
}
const cli = (args, cwd = work, selected = context) => cliData(run(binary, ["--output", "json", "--context", selected, ...args], cwd));
const appCli = (...args) => cli(["--app", product, "app", ...args], project);
const sql = (database, query) => run("docker", ["exec", db, "psql", "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", database, "-Atc", query]);
function manage(action, extra = []) {
  return JSON.parse(run("python3", ["scripts/official-apps.py", action, "--catalog", catalogFile,
    "--context", context, "--miy", binary, ...extra]));
}
function pin() {
  run("git", ["add", "."], project);
  run("git", ["-c", "user.name=miy test", "-c", "user.email=fixture@localhost", "commit", "-qm", "Test release"], project);
  const { appId, ...pin } = appCli("source-pin");
  assert.equal(appId, product);
  catalog.apps[0].releases.push(pin);
  write(catalogFile, catalog);
  run("bash", ["scripts/run-service.sh", "catalog-sync", "--file", catalogFile]);
}
function database(environment) {
  const name = sql("miy_core", `SELECT database_name FROM db_resources WHERE app_id='${product}' AND environment='${environment}'`);
  assert.match(name, /^[a-z][a-z0-9_]+$/);
  return name;
}
function acceptAndPromote(id) {
  const dev = appCli("status").installations.find((i) => i.environment === "dev");
  assert.equal(dev.releaseId, id);
  assert.equal(dev.distribution, "default_official");
  assert.equal(dev.runtimeStatus, "running");
  assert.equal(sql(database("dev"), "SELECT count(*) FROM app.plans"), "1");
  const report = join(work, "acceptance.json");
  write(report, { releaseId: id, checks: [{ scope: "dev release and data", actor: "isolated test", result: "passed",
    evidence: "Verified the running release UUID, publisher classification and application database row." }] });
  appCli("accept", "--note", "Verified the isolated planner release and its row", "--report", report);
  assert.equal(manage("promote", ["--app", product, "--release", id, "--confirm-prod"]).status, "promoted");
  const prod = appCli("status").installations.find((i) => i.environment === "prod");
  assert.equal(prod.releaseId, id);
  assert.equal(prod.imageId, dev.imageId);
  assert.equal(prod.distribution, "default_official");
  assert.equal(prod.canOpen, false);
  return prod;
}

cli(["context", "add", context, "--url", origin]);
cli(["auth", "login", "--account", "admin"]);
cli(["app", "init", project, "--id", product, "--name", "격리 플래너"]);
cli(["component", "sync"], project);
run("git", ["init", "-q"], project);
writeFileSync(join(project, "migrations/0002_plans.sql"), "CREATE TABLE app.plans(id integer PRIMARY KEY, title text NOT NULL);\n");
pin();
console.log("Building and installing the first pinned planner in dev");
const first = manage("install", ["--sources", work]);
assert.equal(first.status, "awaiting_acceptance");
sql(database("dev"), "INSERT INTO app.plans VALUES(1,'dev plan')");
// The new developer has no local project or build receipt. Reuse the image
// published by the first developer, with an independently authenticated context.
const successorContext = "catalog-successor";
cli(["context", "add", successorContext, "--url", origin], work, successorContext);
const successor = (args) => cli(args, work, successorContext);
successor(["auth", "login", "--account", "bob"]);
const successorId = successor(["auth", "status"]).userId;
const grantDev = (action) => cli(["--app", product, "grant", action, "--user", successorId, "--permission", "develop", "--environment", "dev"]);
const redeploy = ["--app", product, "app", "deploy", "--environment", "dev", "--release", first.releaseId];
const original = appCli("status").installations.find((i) => i.environment === "dev");
grantDev("set");
cli(["--yes", "--app", product, "app", "remove", "--environment", "dev"], project);
const planFile = join(work, "redeploy-plan.json");
const plan = successor([...redeploy, "--dry-run"]);
assert.equal(plan.canSubmit, true);
assert.equal(plan.input.releaseId, first.releaseId);
write(planFile, plan);
assert.equal(successor([...redeploy, "--if-current", "@" + planFile]).releaseId, first.releaseId);
const handedOver = appCli("status").installations.find((i) => i.environment === "dev");
assert.equal(handedOver.id, original.id);
assert.equal(handedOver.imageId, original.imageId);
const credential = join(work, "agent-credential.json");
const issued = successor(["auth", "delegations", "create", "--app", product, "--environment", "dev",
  "--action", "read", "--action", "deploy", "--name", "Planner handoff", "--expires-in", "600", "--credential-file", credential]);
const agentEnv = fixtureEnv(join(work, "agent"), { CI: "true" });
const delegated = (args) => cliData(run(binary, ["--output", "json", "--context", "agent", ...args], work, agentEnv));
delegated(["context", "add", "agent", "--url", origin]);
delegated(["auth", "delegations", "import", "--credential-file", credential]);
const agentPlan = delegated([...redeploy, "--dry-run"]);
assert.equal(agentPlan.canSubmit, true);
write(planFile, agentPlan);
assert.equal(delegated([...redeploy, "--if-current", "@" + planFile]).releaseId, first.releaseId);
successor(["--yes", "auth", "delegations", "revoke", issued.delegation.id]);
grantDev("revoke");
const installed = acceptAndPromote(first.releaseId);
const prodDb = database("prod");
assert.equal(sql(prodDb, "SELECT count(*) FROM app.plans"), "0");
sql(prodDb, "INSERT INTO app.plans VALUES(1,'retained plan')");
const manifestFile = join(project, "manifest.json");
write(manifestFile, { ...read(manifestFile), version: "0.2.0" });
writeFileSync(join(project, "migrations/0003_plan_status.sql"), "ALTER TABLE app.plans ADD COLUMN status text NOT NULL DEFAULT 'open';\n");
pin();
console.log("Updating the planner and checking installation and data continuity");
const second = manage("update", ["--app", product, "--source", project]);
assert.equal(second.status, "awaiting_acceptance");
assert.notEqual(second.releaseId, first.releaseId);
const updated = acceptAndPromote(second.releaseId);
assert.equal(updated.id, installed.id);
assert.equal(sql(prodDb, "SELECT title||':'||status FROM app.plans WHERE id=1"), "retained plan:open");
const reader = "catalog-reader";
cli(["context", "add", reader, "--url", origin], work, reader);
const employee = (args) => cli(args, work, reader);
employee(["auth", "login", "--account", "alice"]);
const user = employee(["auth", "status"]).userId;
const visible = () => employee(["app", "list"]).apps.some((app) => app.id === product);
const grant = (action) => cli(["--app", product, "grant", action, "--user", user, "--permission", "use", "--environment", "prod"]);
assert.equal(visible(), false);
grant("set");
assert.equal(visible(), true);
const opened = employee(["--app", product, "app", "status"]).installations;
assert.equal(opened.length, 1);
assert.equal(opened[0].canOpen, true);
assert.equal(opened[0].canManage, false);
assert.equal(opened[0].distribution, "default_official");
grant("revoke");
assert.equal(visible(), false);
cli(["--app", product, "--yes", "app", "stop", "--environment", "prod"], project);
assert.equal(manage("install", ["--sources", work]).status, "already_installed");
assert.equal(manage("promote", ["--app", product, "--release", second.releaseId, "--confirm-prod"]).status, "preserved");
assert.equal(appCli("status").installations.find((i) => i.environment === "prod").runtimeStatus, "stopped");
console.log("PASS developer and delegated handoff with the original image, two pinned versions, checked promotions, same production database, employee grant and revocation, preserved stop");
