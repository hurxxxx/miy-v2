// Real OIDC/server-DB acceptance. Only this run's qa-core-* app is mutated.
// Raw output, passwords, cookies, device codes and preview URLs are never logged.
// --check: read-only local prerequisites
// --run --phase auth --cli .local/bin/miy-server: login/register/admin approval
// --run --resume <evidence directory>: continue after helper publication
// --run --cli <binary>: complete fresh working-tree (nonpublishable) acceptance
import { spawn } from "node:child_process";
import {
  appendFileSync, chmodSync, copyFileSync, createReadStream, existsSync, mkdirSync,
  readFileSync, readdirSync, writeFileSync,
} from "node:fs";
import { createHash, randomBytes } from "node:crypto";
import { homedir } from "node:os";
import { delimiter, dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const evidenceRoot = join(root, ".local/evidence/server-development-flow");
const suffix = process.platform === "win32" ? ".exe" : "";
const options = { run: false, check: false, phase: "all", cli: join(root, "target/debug/miy" + suffix) };
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i];
  if (arg === "--run") options.run = true;
  else if (arg === "--check") options.check = true;
  else if (["--cli", "--phase", "--resume"].includes(arg) && process.argv[i + 1]) {
    options[arg.slice(2)] = process.argv[++i];
    if (arg === "--cli") options.cliSupplied = true;
  }
  else {
    console.error("Usage: node scripts/check-server-development-flow.mjs --check | --run [--phase auth|all] [--cli PATH] [--resume EVIDENCE_DIR]");
    process.exit(2);
  }
}
if (options.run === options.check || !["auth", "all"].includes(options.phase)) {
  console.error("Choose --check or --run, with optional --phase auth|all.");
  process.exit(2);
}

let step = "prerequisites", evidence, state, browser, preview, app, binary, users;
const contexts = [], loggedIn = new Set();
const envBase = { ...process.env, CI: "true" };
for (const name of Object.keys(envBase)) if (name.startsWith("MIY_")) delete envBase[name];
envBase.PATH = join(homedir(), ".cargo/bin") + delimiter + (envBase.PATH || "");
envBase.CARGO_TARGET_DIR = join(root, ".local/acceptance-cargo-target");

function fail(code) { const error = new Error(code); error.safeCode = code; throw error; }
function requireThat(condition, code) { if (!condition) fail(code); }
function safeCode(value, fallback = "unexpected_failure") {
  return typeof value === "string" && /^[a-zA-Z0-9_.:-]{1,160}$/.test(value) ? value : fallback;
}
function readJson(path) { return JSON.parse(readFileSync(path, "utf8")); }
function writeJson(path, value) {
  writeFileSync(path, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
  if (process.platform !== "win32") chmodSync(path, 0o600);
}
function privateDir(path) {
  mkdirSync(path, { recursive: true, mode: 0o700 });
  if (process.platform !== "win32") chmodSync(path, 0o700);
}
function event(type, details = {}) {
  if (evidence) appendFileSync(join(evidence, "events.jsonl"), JSON.stringify({ at: new Date().toISOString(), type, step, ...details }) + "\n", { mode: 0o600 });
}
function save() { if (state && evidence) writeJson(join(evidence, "result.json"), state); }
function passed(name, details = {}) {
  if (!state.passed.includes(name)) state.passed.push(name);
  event("passed", { name, ...details }); save();
  console.log("PASS " + name);
}
async function hashFile(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}
function environment(account) {
  return { ...envBase, MIY_CONFIG_DIR: join(evidence, "private", account), MIY_CREDENTIAL_BACKEND: "file" };
}
function signalOwned(child, signal) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  try {
    if (process.platform !== "win32") process.kill(-child.pid, signal);
    else child.kill(signal);
  } catch { /* This owned process may have just exited. */ }
}
function start(program, args, { cwd = root, account = "developer", timeout = 1800000, onStderrLine } = {}) {
  const child = spawn(program, args, {
    cwd, env: evidence ? environment(account) : envBase,
    stdio: ["ignore", "pipe", "pipe"], detached: process.platform !== "win32",
  });
  let stdout = "", stderr = "", lineBuffer = "", timedOut = false, outputOverflow = false, escalation;
  const timer = setTimeout(() => {
    timedOut = true; signalOwned(child, "SIGINT");
    escalation = setTimeout(() => signalOwned(child, "SIGKILL"), 5000);
  }, timeout);
  child.stdout.on("data", chunk => {
    stdout += chunk.toString();
    if (stdout.length > 16 * 1024 * 1024) { outputOverflow = true; stdout = stdout.slice(-16 * 1024 * 1024); }
  });
  child.stderr.on("data", chunk => {
    stderr = (stderr + chunk.toString()).slice(-4 * 1024 * 1024);
    if (onStderrLine) {
      lineBuffer += chunk.toString();
      let boundary;
      while ((boundary = lineBuffer.indexOf("\n")) >= 0) {
        onStderrLine(lineBuffer.slice(0, boundary)); lineBuffer = lineBuffer.slice(boundary + 1);
      }
      if (lineBuffer.length > 1024 * 1024) lineBuffer = "";
    }
  });
  const done = new Promise(resolveDone => {
    child.once("error", () => {
      clearTimeout(timer); clearTimeout(escalation);
      resolveDone({ code: null, processError: true, stdout: "", stderr: "" });
    });
    child.once("close", (code, signal) => {
      clearTimeout(timer); clearTimeout(escalation);
      resolveDone({ code, signal, stdout, stderr, timedOut, outputOverflow });
    });
  });
  return { child, done };
}
async function command(program, args, config = {}) {
  const started = Date.now();
  event("command_started", { program: program === binary ? "miy" : program, account: config.account || "developer" });
  const result = await start(program, args, config).done;
  event("command_finished", { exitCode: result.code, durationMs: Date.now() - started, timedOut: !!result.timedOut });
  if (result.processError) fail("process_start_failed");
  if (result.timedOut) fail("process_timeout");
  if (result.outputOverflow) fail("process_output_limit");
  return result;
}
async function checked(program, args, config = {}) {
  const result = await command(program, args, config);
  requireThat(result.code === 0, "external_command_failed");
  return result.stdout.trim();
}
function cliArgs(args) { return ["--output", "json", "--no-input", "--context", "default", ...args]; }
function parseCli(result, expected) {
  let response;
  try { response = JSON.parse(result.stdout); } catch { fail("invalid_cli_envelope"); }
  requireThat(response.schemaVersion === 1 && typeof response.command === "string" && response.meta && Object.hasOwn(response, "data") && Object.hasOwn(response, "error"), "invalid_cli_envelope");
  event("cli_result", { command: response.command, exitCode: result.code, errorCode: response.error ? safeCode(response.error.code, "invalid_error_code") : null });
  if (result.code !== expected) fail(response.error ? safeCode(response.error.code, "unexpected_cli_exit") : "unexpected_cli_exit");
  requireThat(response.ok === (expected === 0), "cli_exit_envelope_mismatch");
  return response.ok ? response.data : response.error;
}
async function cli(account, args, { expected = 0, cwd = app || root, timeout = 1800000, project = !!app } = {}) {
  const executable = project ? join(app, ".miy-tools/miy" + suffix) : binary;
  return parseCli(await command(executable, cliArgs(args), { account, cwd, timeout }), expected);
}
async function poll(test, timeout = 60000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await test();
    if (value) return value;
    await new Promise(resolveWait => setTimeout(resolveWait, 400));
  }
  fail("observation_timeout");
}
function selectedInstallation(value, environment) {
  const found = value.installations?.find(row => row.appId === state.appId && row.environment === environment);
  requireThat(found && typeof found.id === "string" && typeof found.releaseId === "string", "installation_missing");
  return found;
}
function installationEvidence(value) {
  return Object.fromEntries(["id", "appId", "environment", "releaseId", "imageId", "sourceCommit", "dbStatus", "runtimeStatus", "desiredState"].map(key => [key, value[key]]));
}

const backend = `use axum::{Extension, Json, Router, extract::{Path, State}, http::StatusCode, routing::{delete, get}};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

type ApiResult = Result<Json<Value>, StatusCode>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record { key: String, value: String }
fn owned_key(who: &Identity, key: &str) -> Result<String, StatusCode> {
    if key.is_empty() || key.len() > 80 || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(format!("{}:{}", who.user_id, key))
}
async fn list(State(db): State<PgPool>, Extension(who): Extension<Identity>) -> ApiResult {
    let prefix = format!("{}:", who.user_id);
    let rows = sqlx::query_as::<_, (String, String)>("SELECT key,value FROM app.app_metadata WHERE left(key,length($1))=$1 ORDER BY key")
        .bind(&prefix).fetch_all(&db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let records: Vec<Value> = rows.into_iter().map(|(key, value)| json!({"key": &key[prefix.len()..], "value": value})).collect();
    Ok(Json(json!({"userId": who.user_id, "database": database, "records": records})))
}
async fn save(State(db): State<PgPool>, Extension(who): Extension<Identity>, Json(record): Json<Record>) -> ApiResult {
    let key = owned_key(&who, &record.key)?;
    if record.value.trim().is_empty() || record.value.len() > 500 { return Err(StatusCode::BAD_REQUEST); }
    sqlx::query("INSERT INTO app.app_metadata(key,value) VALUES($1,$2) ON CONFLICT(key) DO UPDATE SET value=EXCLUDED.value")
        .bind(key).bind(&record.value).execute(&db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({"key": record.key, "value": record.value, "owner": who.user_id})))
}
async fn remove(State(db): State<PgPool>, Extension(who): Extension<Identity>, Path(key): Path<String>) -> ApiResult {
    let changed = sqlx::query("DELETE FROM app.app_metadata WHERE key=$1").bind(owned_key(&who, &key)?)
        .execute(&db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.rows_affected();
    if changed != 1 { return Err(StatusCode::NOT_FOUND); }
    Ok(Json(json!({"deleted": key})))
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db = miy_app_sdk::database().await?;
    miy_app_sdk::serve(Router::new().route("/records", get(list).post(save)).route("/records/{key}", delete(remove)).with_state(db)).await
}
`;
const frontend = `import {useEffect, useState} from "react";
import type {AppProps} from "@miy/sdk";
type Records = {userId:string; database:string; records:{key:string; value:string}[]};
export default function AppRoot({runtime}:AppProps) {
  const [data,setData] = useState<Records>();
  const [key,setKey] = useState("");
  const [value,setValue] = useState("");
  const [error,setError] = useState("");
  const load = () => runtime.api<Records>("/records").then(setData);
  useEffect(() => {void load().catch(() => setError("Load failed"))},[runtime.installationId]);
  return <section><h1>Core flow acceptance</h1>
    <form onSubmit={event => {event.preventDefault(); setError(""); void runtime.api("/records",{method:"POST",body:JSON.stringify({key,value})}).then(load).catch(() => setError("Save failed"))}}>
      <label>Record key<input value={key} onChange={event => setKey(event.target.value)}/></label>
      <label>Record value<input value={value} onChange={event => setValue(event.target.value)}/></label>
      <button type="submit">Save record</button>
    </form>
    <button onClick={() => {void load().catch(() => setError("Load failed"))}}>Refresh records</button>
    <pre data-testid="records">{JSON.stringify(data)}</pre><p role="alert">{error}</p>
  </section>;
}
`;

async function loginBrowser(account, password, origin) {
  step = account + "_oidc_login";
  const context = await browser.newContext();
  contexts.push(context);
  context.setDefaultTimeout(45000);
  const page = await context.newPage();
  await page.goto(origin + "/auth/login");
  await page.locator("input[name=username]").fill(account);
  await page.locator("input[name=password]").fill(password);
  await page.locator("#kc-login").click();
  await page.waitForURL(url => url.origin === origin && !url.pathname.startsWith("/auth/"));
  const response = await context.request.get(origin + "/api/v1/me");
  requireThat(response.status() === 200, "oidc_me_failed");
  const me = await response.json();
  requireThat(typeof me.id === "string" && me.admin === (account === "admin") && me.development === false, "ordinary_oidc_identity_required");
  passed(account + "_normal_oidc_login", { userId: me.id, admin: me.admin });
  return { context, page, me };
}
async function loginCli(account, user, origin) {
  step = account + "_cli_device_login";
  const pending = await cli(account, ["auth", "login", "--device-code", "--no-open", "--no-wait"], { project: false, cwd: root, timeout: 60000 });
  requireThat(pending.status === "pending" && typeof pending.loginId === "string" && new URL(pending.verificationUrl).origin === origin, "invalid_device_login_target");
  await user.page.goto(pending.verificationUrl);
  await user.page.locator("input[name=userCode]").fill(pending.userCode);
  await user.page.getByRole("button", { name: "이 계정으로 CLI 연결 승인", exact: true }).click();
  await user.page.getByRole("status").filter({ hasText: "CLI 연결을 승인했습니다" }).waitFor();
  const authenticated = await cli(account, ["auth", "wait", pending.loginId], { project: false, cwd: root, timeout: 120000 });
  requireThat(authenticated.status === "authenticated", "device_login_not_authenticated");
  await cli(account, ["auth", "status"], { project: false, cwd: root, timeout: 60000 });
  loggedIn.add(account);
  passed(account + "_cli_device_ui_approval");
}
async function fixtureApi(user, installation, path, method = "GET", data) {
  const response = await user.context.request.fetch(state.origin + "/api/v1/installations/" + installation.id + "/api" + path, {
    method, data, headers: { origin: state.origin, "x-miy-csrf": user.me.csrf, "x-miy-release": installation.releaseId },
  });
  requireThat(response.status() === 200, "fixture_api_failed");
  return response.json();
}
async function runtimeJob(account, args) {
  const queued = await cli(account, ["--app", state.appId, "--yes", ...args, "--no-wait"]);
  requireThat(queued.appId === state.appId && typeof queued.id === "string", "job_target_mismatch");
  event("runtime_job", { id: queued.id, appId: queued.appId, environment: queued.environment, action: queued.action });
  const result = await cli(account, ["job", "watch", queued.id, "--timeout", "900"], { timeout: 930000 });
  requireThat(result.id === queued.id && result.status === "succeeded", "runtime_job_not_succeeded");
  return { id: result.id, status: result.status, environment: result.environment, action: result.action };
}
async function browserRecords(user, installation) {
  const page = await user.context.newPage();
  await page.goto(state.origin + "/environments/" + installation.environment + "/apps/" + state.appId);
  await page.getByRole("heading", { name: "Core flow acceptance", exact: true }).waitFor();
  await poll(async () => (await page.getByTestId("records").textContent())?.includes('"records"'));
  return { page, data: JSON.parse(await page.getByTestId("records").textContent()) };
}
async function stopPreview() {
  if (!app || (!preview && !existsSync(join(app, ".miy/remote-dev.json")) && !existsSync(join(app, ".miy/preview-run.json")))) return;
  step = "local_preview_stop";
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      await cli("developer", ["app", "dev-stop"], { timeout: 150000 });
      if (preview) {
        const result = await Promise.race([preview.done, new Promise(resolveWait => setTimeout(() => resolveWait(null), 10000))]);
        if (!result) fail("preview_owner_still_running");
        requireThat([0, 130].includes(result.code), "preview_owner_failed");
      }
      preview = undefined;
      requireThat(!existsSync(join(app, ".miy/remote-dev.json")), "development_session_not_closed");
      passed("local_preview_stopped_before_build_and_deploy");
      return;
    } catch (error) {
      if (attempt === 2) throw error;
      await new Promise(resolveWait => setTimeout(resolveWait, 1000));
    }
  }
}

async function upgradeSnapshot(sourcePath) {
  step = "resume_candidate_cli_setup";
  requireThat(!existsSync(join(app, ".miy/preview-run.json")) && !existsSync(join(app, ".miy/remote-dev.json")), "stop_development_before_cli_update");
  const source = resolve(root, sourcePath), nextHash = await hashFile(source);
  const beforeBundle = await hashFile(join(app, "miy.bundle.json"));
  requireThat(readJson(join(app, "miy.bundle.json")).bundleVersion === state.bundleVersion, "resume_bundle_changed");
  const beforeConfigs = {};
  for (const account of ["developer", "admin"])
    beforeConfigs[account] = await hashFile(join(evidence, "private", account, "config.json"));
  const candidateDir = join(evidence, "private", "candidates", nextHash);
  privateDir(candidateDir);
  const candidate = join(candidateDir, "miy" + suffix);
  copyFileSync(source, candidate);
  if (process.platform !== "win32") chmodSync(candidate, 0o700);
  requireThat(await hashFile(candidate) === nextHash, "candidate_changed_during_snapshot");
  const version = await checked(candidate, ["--version"]);
  requireThat(/^miy \d+\.\d+\.\d+$/.test(version), "unexpected_cli_version");
  const provenancePath = join(dirname(source), "release.json");
  let provenance;
  if (existsSync(provenancePath)) {
    const supplied = readJson(provenancePath);
    requireThat(supplied.schemaVersion === 1 && supplied.binarySha256 === nextHash && /^[a-f0-9]{40}$/.test(supplied.commit), "candidate_provenance_mismatch");
    provenance = { commit: supplied.commit, target: supplied.target, version: supplied.version, binarySha256: supplied.binarySha256 };
  }
  // app setup verifies the existing bundle and applies one project transaction.
  // A different bundle must use the separate explicit upgrade workflow.
  parseCli(await command(candidate, cliArgs(["app", "setup"]), { cwd: app, timeout: 120000 }), 0);
  requireThat(await hashFile(join(app, "miy.bundle.json")) === beforeBundle, "setup_changed_sdk_bundle");
  await checked("bash", [join(root, "scripts/install-cli.sh"), "--from", candidate, "--prefix", dirname(binary), "--server", state.origin, "--no-input"], { timeout: 120000 });
  requireThat(await hashFile(binary) === nextHash && await hashFile(join(app, ".miy-tools/miy" + suffix)) === nextHash, "candidate_install_hash_mismatch");
  for (const account of ["developer", "admin"])
    requireThat(await hashFile(join(evidence, "private", account, "config.json")) === beforeConfigs[account], "resume_changed_existing_context");
  const update = { oldCliSha256: state.source.cliSha256, newCliSha256: nextHash, oldCliVersion: state.cliVersion, newCliVersion: version.slice(4), bundleVersion: state.bundleVersion, contextsPreserved: true, provenance };
  (state.cliUpdates ||= []).push(update);
  state.source.cliSha256 = nextHash;
  state.cliVersion = version.slice(4);
  state.currentCliProvenance = provenance;
  passed("resume_installed_candidate_and_updated_project_without_changing_bundle_or_contexts", update);
}

async function run(settings, credentials) {
  const resumed = !!options.resume;
  if (resumed) {
    evidence = resolve(options.resume);
    requireThat(evidence.startsWith(evidenceRoot + sep), "resume_directory_outside_harness");
    state = readJson(join(evidence, "result.json"));
    requireThat(state.harness === "server-development-flow-v1" && state.stage === "registered" && state.origin === settings.origin && /^qa-core-[a-z0-9-]+$/.test(state.appId), "invalid_resume_state");
  } else {
    const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").toLowerCase() + "-" + randomBytes(3).toString("hex");
    evidence = join(evidenceRoot, stamp);
    requireThat(!existsSync(evidence), "evidence_directory_exists");
    privateDir(evidence);
    state = {
      harness: "server-development-flow-v1", kind: "working-tree-snapshot", publishable: false,
      appId: "qa-core-" + stamp, origin: settings.origin, startedAt: new Date().toISOString(),
      stage: "starting", passed: [], cleanup: [],
      notRun: ["Actual Windows PowerShell 5.1/Docker Desktop/ARM64", "Actual Codex or Claude project trust and live agent tool use", "Published signed release installation", "UI/UX improvement work"],
    };
  }
  app = join(evidence, "app");
  binary = join(evidence, "bin/miy" + suffix);
  privateDir(join(evidence, "private"));
  for (const account of ["admin", "developer"]) privateDir(join(evidence, "private", account));
  (state.executions ||= []).push({ phase: options.phase, resumed, startedAt: new Date().toISOString(), scriptSha256: await hashFile(fileURLToPath(import.meta.url)) });
  save();
  console.log("Evidence: " + evidence);
  if (!resumed) {
    step = "install_explicit_server";
    const source = resolve(root, options.cli), sourceHash = await hashFile(source);
    await checked("bash", [join(root, "scripts/install-cli.sh"), "--from", source, "--prefix", dirname(binary), "--server", settings.origin, "--no-input"], { timeout: 120000 });
    requireThat(await hashFile(binary) === sourceHash, "cli_snapshot_changed");
    const commit = await checked("git", ["rev-parse", "HEAD"]);
    const diff = await checked("git", ["diff", "--binary", "HEAD"]);
    state.source = { commit, workingTreeDiffSha256: createHash("sha256").update(diff).digest("hex"), cliSha256: sourceHash, scriptSha256: await hashFile(fileURLToPath(import.meta.url)) };
    const version = await checked(binary, ["--version"]);
    requireThat(/^miy \d+\.\d+\.\d+$/.test(version), "unexpected_cli_version");
    state.cliVersion = version.slice(4);
    await cli("admin", ["context", "init", "--url", settings.origin], { project: false, cwd: root, timeout: 60000 });
    passed("installed_snapshot_with_explicit_configured_origin");
  } else {
    requireThat(await hashFile(binary) === state.source.cliSha256, "resumed_cli_snapshot_changed");
    if (options.cliSupplied) await upgradeSnapshot(options.cli);
  }
  browser = await chromium.launch({ headless: true });
  users = {};
  for (const account of ["developer", "admin"]) {
    users[account] = await loginBrowser(account, credentials[account], settings.origin);
    if (!resumed) await loginCli(account, users[account], settings.origin);
    else {
      await cli(account, ["auth", "status"], { project: false, cwd: root, timeout: 60000 });
      loggedIn.add(account);
    }
  }
  requireThat(users.developer.me.id !== users.admin.me.id, "separate_accounts_required");
  state.users = { developer: users.developer.me.id, admin: users.admin.me.id };
  if (!resumed) {
    step = "init_and_register_fixture";
    const initialized = await cli("developer", ["app", "init", app, "--id", state.appId, "--name", "Core flow " + state.appId], { project: false, cwd: root });
    requireThat(initialized.appId === state.appId, "scaffold_target_mismatch");
    state.bundleVersion = initialized.bundleVersion;
    writeFileSync(join(app, "src/main.rs"), backend);
    writeFileSync(join(app, "web/src/AppRoot.tsx"), frontend);
    const registration = await cli("developer", ["app", "register"]);
    requireThat(registration.appId === state.appId && typeof registration.id === "string", "registration_target_mismatch");
    state.registrationId = registration.id;
    await cli("developer", ["registration", "approve", registration.id], { expected: 4 });
    passed("ordinary_developer_cannot_approve_registration");
    await cli("admin", ["registration", "approve", registration.id]);
    passed("admin_approved_unique_developer_app", { appId: state.appId, registrationId: registration.id });
    state.stage = "registered"; save();
  }
  if (options.phase === "auth") {
    state.status = "partial";
    state.pending = ["helper availability", "server DB local CRUD", "dev deployment persistence", "acceptance and production isolation", "logs/status/cleanup"];
    save();
    console.log("Authentication and registration complete; resume this evidence directory for the remaining checks.");
    return;
  }
  await developmentFlow(settings);
}

async function developmentFlow(settings) {
  step = "published_helper_discovery";
  const installResponse = await users.developer.context.request.get(settings.origin + "/api/v1/cli/install");
  requireThat(installResponse.status() === 200, "install_guide_unavailable");
  const guide = await installResponse.json();
  requireThat(guide.serverUrl === settings.origin, "install_guide_origin_mismatch");
  // A published candidate identifies each native binary, even if its version
  // string is unchanged. Never resume a preceding working-tree snapshot silently.
  const candidate = guide.downloads?.find(row => row.binarySha256 === state.source.cliSha256);
  requireThat(candidate && candidate.commit === guide.sourceCommit && candidate.version === state.cliVersion, "candidate_cli_update_required");
  state.currentCliProvenance = { commit: candidate.commit, target: candidate.target, version: candidate.version, binarySha256: candidate.binarySha256 };
  const docker = JSON.parse(await checked("docker", ["info", "--format", "{{json .}}"], { timeout: 60000 }));
  const arch = { x86_64: "amd64", amd64: "amd64", aarch64: "arm64", arm64: "arm64" }[docker.Architecture];
  requireThat(docker.OSType === "linux" && arch, "linux_docker_required");
  const platform = "linux/" + arch;
  const helper = guide.tunnelImages?.find(row => row.platform === platform);
  requireThat(helper && /^sha256:[0-9a-f]{64}$/.test(helper.imageId) && /^[0-9a-f]{64}$/.test(helper.sha256), "development_helper_not_published");
  const cached = await command("docker", ["image", "inspect", helper.imageId], { timeout: 60000 });
  state.helper = { platform, imageId: helper.imageId, imageTag: helper.imageTag, configSha256: helper.configSha256, sha256: helper.sha256, sourceCommit: guide.sourceCommit, alreadyCached: cached.code === 0, publishable: false };
  if (cached.code === 0 && !state.notRun.includes("Fresh helper download (matching helper was already cached)"))
    state.notRun.push("Fresh helper download (matching helper was already cached)");
  passed("configured_origin_and_matching_helper_advertised");
  state.stage = "running";
  save();

  step = "fixture_toolchain_and_guidance";
  await cli("developer", ["component", "sync"]);
  await cli("developer", ["app", "fmt"]);
  const doctor = await cli("developer", ["doctor", "--for", "develop"], { timeout: 120000 });
  state.doctor = doctor.checks.map(check => ({ id: check.id, status: check.status, code: check.code }));
  requireThat(doctor.ready && ["rustfmt", "rustLinker", "projectMcpConfiguration", "projectMcpProcess"].every(id => doctor.checks.some(check => check.id === id && check.status === "pass")), "development_doctor_not_ready");
  await cli("developer", ["app", "check"]);
  await checked("git", ["init", "-b", "main"], { cwd: app });
  await checked("git", ["add", "."], { cwd: app });
  await checked("git", ["-c", "user.name=MIY acceptance", "-c", "user.email=acceptance@localhost", "commit", "-m", "test: isolated real server development flow"], { cwd: app });
  state.appCommit = await checked("git", ["rev-parse", "HEAD"], { cwd: app });
  passed("generated_guidance_mcp_linker_and_application_checks");

  step = "local_preview_server_database";
  preview = start(join(app, ".miy-tools/miy" + suffix), cliArgs(["app", "dev", "--no-open", "--port", "0"]), {
    cwd: app, timeout: 2400000,
    onStderrLine(line) {
      try {
        const value = JSON.parse(line);
        if (value.appId === state.appId && value.database === "server-dev") {
          state.localDevelopment = { databaseId: value.databaseId, schemaFingerprint: value.schemaFingerprint, database: value.database };
          save();
        }
      } catch { /* Build output and the private entry URL are never persisted. */ }
    },
  });
  let previewFinished;
  preview.done.then(result => { previewFinished = result; });
  const previewInfo = await poll(async () => {
    if (previewFinished) { parseCli(previewFinished, 0); fail("preview_exited_before_ready"); }
    const file = join(app, ".miy/preview.json");
    if (!existsSync(file)) return false;
    try { return readJson(file); } catch { return false; }
  }, 1800000);
  const entry = new URL(previewInfo.url);
  requireThat(previewInfo.appId === state.appId && entry.protocol === "http:" && entry.hostname === "127.0.0.1", "local_preview_target_mismatch");
  const local = await browser.newContext();
  contexts.push(local); local.setDefaultTimeout(45000);
  const page = await local.newPage();
  await page.goto(previewInfo.url);
  await page.getByRole("heading", { name: "Core flow acceptance", exact: true }).waitFor();
  const localContextResponse = await local.request.get(entry.origin + "/__miy/context");
  requireThat(localContextResponse.status() === 200, "local_context_unavailable");
  const localContext = await localContextResponse.json();
  requireThat(localContext.user.id === users.developer.me.id, "local_identity_mismatch");
  const localApi = async (path, method = "GET", data) => {
    const response = await local.request.fetch(entry.origin + "/__miy/api" + path, { method, data, headers: { origin: entry.origin, "x-miy-csrf": localContext.csrf } });
    requireThat(response.status() === 200, "local_fixture_api_failed");
    return response.json();
  };
  const marker = "created-locally-" + state.appId;
  await page.getByLabel("Record key", { exact: true }).fill("keep");
  await page.getByLabel("Record value", { exact: true }).fill(marker);
  await page.getByRole("button", { name: "Save record", exact: true }).click();
  await poll(async () => (await page.getByTestId("records").textContent())?.includes(marker));
  const created = await localApi("/records");
  requireThat(created.userId === users.developer.me.id && created.records.length === 1 && created.records[0].value === marker, "local_record_not_created");
  await localApi("/records", "POST", { key: "temporary", value: "before-update" });
  await localApi("/records", "POST", { key: "temporary", value: "after-update" });
  const updated = await localApi("/records");
  requireThat(updated.records.some(row => row.key === "temporary" && row.value === "after-update"), "local_update_failed");
  await localApi("/records/temporary", "DELETE");
  requireThat((await localApi("/records")).records.length === 1, "local_delete_failed");
  requireThat(state.localDevelopment?.databaseId, "server_development_database_id_missing");
  state.localDevelopment.databaseName = created.database;
  state.record = { key: "keep", value: marker, count: 1, owner: created.userId };
  passed("local_browser_create_and_api_read_update_delete_on_server_dev_database");
  await stopPreview();

  step = "build_and_dev_deploy";
  const image = await cli("developer", ["app", "build"]);
  state.build = { imageId: image.imageId, sourceCommit: image.sourceCommit, platform: image.platform };
  state.devJob = await runtimeJob("developer", ["app", "deploy", "--environment", "dev"]);
  const dev = selectedInstallation(await cli("developer", ["app", "status"]), "dev");
  requireThat(dev.runtimeStatus === "running" && dev.dbStatus === "ready", "dev_runtime_not_ready");
  state.dev = installationEvidence(dev);
  const devRows = await fixtureApi(users.developer, dev, "/records");
  requireThat(devRows.database === created.database && devRows.userId === created.userId && devRows.records.length === 1 && devRows.records[0].key === "keep" && devRows.records[0].value === marker, "development_database_record_not_preserved");
  const devUi = await browserRecords(users.developer, dev);
  requireThat(devUi.data.records.length === 1 && devUi.data.records[0].value === marker, "deployed_browser_record_not_visible");
  const adminRows = await fixtureApi(users.admin, dev, "/records");
  requireThat(adminRows.userId === users.admin.me.id && adminRows.records.length === 0, "sdk_identity_ownership_failed");
  passed("dev_deployment_reuses_exact_server_database_and_local_record", { databaseId: state.localDevelopment.databaseId, installationId: dev.id, releaseId: dev.releaseId });
  passed("separate_admin_sdk_identity_cannot_read_developer_owned_rows");

  step = "acceptance_and_production_permission";
  const acceptance = join(evidence, "acceptance.json");
  writeJson(acceptance, { releaseId: dev.releaseId, note: "Working-tree acceptance: local preview writes remained in the same server development database after deployment.", checks: [
    { scope: "local CRUD and dev persistence", actor: "ordinary developer via CLI/OIDC", result: "passed", evidence: "App " + state.appId + "; database " + state.localDevelopment.databaseId + "; dev installation " + dev.id + "; exactly one locally created row is visible through deployed API and browser." },
    { scope: "SDK identity row ownership", actor: "separate admin OIDC browser", result: "passed", evidence: "Admin and developer received different signed SDK identities; admin saw zero developer-owned records." },
    { scope: "actual Windows and live AI agent", actor: "acceptance harness", result: "not_run", evidence: "Linux working-tree harness. Actual Windows installation and Codex/Claude trust/live tool usage require separate validation." },
  ] });
  await cli("developer", ["app", "accept", "--input", "@" + acceptance]);
  await cli("developer", ["--app", state.appId, "--yes", "app", "promote", "--environment", "prod", "--release", dev.releaseId, "--no-wait"], { expected: 4 });
  passed("accepted_release_cannot_promote_without_production_grant");
  for (const permission of ["deploy", "use"])
    await cli("admin", ["--app", state.appId, "grant", "set", "--user", users.developer.me.id, "--permission", permission, "--environment", "prod"]);
  state.prodJob = await runtimeJob("developer", ["app", "promote", "--environment", "prod", "--release", dev.releaseId]);
  const prod = selectedInstallation(await cli("developer", ["app", "status"]), "prod");
  requireThat(prod.releaseId === dev.releaseId && prod.runtimeStatus === "running" && prod.dbStatus === "ready", "production_release_not_ready");
  state.prod = installationEvidence(prod);
  const prodRows = await fixtureApi(users.developer, prod, "/records");
  requireThat(prodRows.database !== created.database && prodRows.records.length === 0, "production_database_not_isolated");
  const prodUi = await browserRecords(users.developer, prod);
  requireThat(prodUi.data.records.length === 0 && prodUi.data.database === prodRows.database, "production_browser_not_isolated");
  state.productionDatabase = { databaseName: prodRows.database, recordCount: prodRows.records.length };
  passed("admin_granted_exact_release_promotion_with_separate_empty_production_database");

  step = "logs_and_status";
  for (const environment of ["dev", "prod"]) {
    const logs = await cli("developer", ["--app", state.appId, "app", "logs", "--environment", environment, "--tail", "20", "--timeout", "60"], { timeout: 90000 });
    requireThat(logs.appId === state.appId && logs.environment === environment && logs.status === "ready", "runtime_logs_unavailable");
    event("logs_observed", { environment, requestId: logs.id, status: logs.status });
  }
  const status = await cli("developer", ["--app", state.appId, "app", "status"]);
  requireThat(status.installations.filter(item => item.appId === state.appId && item.runtimeStatus === "running").length === 2, "running_installation_status_mismatch");
  passed("dev_and_prod_logs_and_running_status");
  state.stage = "verified"; state.status = "passed"; delete state.pending; save();
}

async function cleanup() {
  if (options.phase === "auth" && state?.stage === "registered") return;
  if (preview || (app && existsSync(join(app, ".miy/remote-dev.json")))) {
    try { await stopPreview(); state.cleanup.push({ action: "local-preview-stop", status: "passed" }); }
    catch (error) {
      state.cleanup.push({ action: "local-preview-stop", status: "failed", code: safeCode(error.safeCode) });
      // Signal only this harness's owned process; preserve its recovery journal.
      if (preview) {
        signalOwned(preview.child, "SIGINT");
        const stopped = await Promise.race([preview.done, new Promise(resolveWait => setTimeout(() => resolveWait(null), 15000))]);
        if (!stopped) {
          signalOwned(preview.child, "SIGKILL");
          await preview.done;
        }
      }
    }
  }
  if (state?.registrationId && loggedIn.has("admin")) {
    try {
      step = "cleanup_owned_test_runtimes";
      const status = await cli("admin", ["--app", state.appId, "app", "status"]);
      for (const installation of status.installations || []) {
        requireThat(installation.appId === state.appId && ["dev", "prod"].includes(installation.environment), "cleanup_target_mismatch");
        if (installation.runtimeStatus === "running") {
          const stopped = await runtimeJob("admin", ["app", "stop", "--environment", installation.environment]);
          state.cleanup.push({ action: "runtime-stop", environment: installation.environment, jobId: stopped.id, status: "passed", databasePreserved: true });
        }
      }
      const after = await cli("admin", ["--app", state.appId, "app", "status"]);
      state.finalInstallations = after.installations.map(installationEvidence);
      requireThat(after.installations.every(item => item.runtimeStatus === "stopped"), "test_runtime_not_stopped");
    } catch (error) { state.cleanup.push({ action: "runtime-stop", status: "failed", code: safeCode(error.safeCode) }); }
  }
  // A registered partial run retains its private credentials for continuation.
  if (state?.stage !== "registered") {
    for (const account of loggedIn) {
      try {
        step = account + "_logout";
        await cli(account, ["auth", "logout"], { cwd: root, project: false, timeout: 60000 });
        state.cleanup.push({ action: account + "-logout", status: "passed" });
      } catch (error) { state.cleanup.push({ action: account + "-logout", status: "failed", code: safeCode(error.safeCode) }); }
    }
  }
}

let failure;
try {
  const settings = readJson(join(root, ".local/server.json"));
  const credentials = readJson(join(root, ".local/credentials.json"));
  const origin = new URL(settings.origin);
  requireThat(origin.protocol === "https:" && origin.origin === settings.origin && !origin.username && !origin.password, "configured_public_https_origin_required");
  requireThat(settings.developmentLogin === false, "ordinary_oidc_server_required");
  requireThat(typeof credentials.admin === "string" && typeof credentials.developer === "string", "private_oidc_credentials_required");
  const cache = process.env.PLAYWRIGHT_BROWSERS_PATH || join(homedir(), ".cache/ms-playwright");
  const headlessInstalled = existsSync(cache) && readdirSync(cache).some(name => name.startsWith("chromium_headless_shell-"));
  if (options.check) {
    console.log(JSON.stringify({ configuredOrigin: origin.origin, ordinaryOidc: true, cliExists: existsSync(resolve(root, options.cli)), playwrightChromiumAvailable: existsSync(chromium.executablePath()) || headlessInstalled, mutations: 0, evidenceKind: "working-tree-snapshot", publishable: false }));
  } else await run(settings, credentials);
} catch (error) {
  failure = { step, code: safeCode(error.safeCode, error.name === "TimeoutError" ? "browser_timeout" : "unexpected_failure") };
  if (state) { state.status = "failed"; state.failure = failure; save(); }
  console.error("FAIL " + failure.step + ": " + failure.code);
} finally {
  if (options.run && state) {
    await cleanup().catch(() => state.cleanup.push({ action: "cleanup", status: "failed", code: "unexpected_failure" }));
    if (state.cleanup.some(item => item.status === "failed")) state.status = "failed";
    state.updatedAt = new Date().toISOString(); save();
  }
  for (const context of contexts) await context.close().catch(() => {});
  if (browser) await browser.close().catch(() => {});
}
if (failure || state?.status === "failed") process.exitCode = 1;
else if (state) console.log(JSON.stringify({ status: state.status, appId: state.appId, passed: state.passed.length, evidence, publishable: false }));
