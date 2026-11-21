#!/usr/bin/env node
// Real-server pilot acceptance with the published CLI: one new app from
// `app init` through registration, approval, build and dev deployment to its
// frame on the dev portal, as a developer and an administrator who sign in with
// ordinary OIDC and approve their CLI on the console.
//
// This CHANGES the selected server: it creates one pilot-* app (registration,
// develop grant, dev database, release, container). The app is stopped at the
// end unless --keep-running true is given; its records and database remain.
// A lost deployment response is observed without resubmission before cleanup;
// an unconfirmed outcome is a cleanup failure, never a successful stop.
// Passwords, cookies, device codes and tokens are never printed or stored.
//
// Usage: node scripts/check-pilot-flow.mjs --origin https://PLATFORM --cli /abs/miy
//   --credentials-file /abs/credentials.json --output NEW_DIRECTORY
//   [--publish-component DIRECTORY] [--keep-running true]
//   [--unpublished true]   isolated stacks only: the CLI is not in an install guide
//   [--developer-username NAME]   sign in the developer with this name (default
//   "developer"), for example an account created from an invitation link. The
//   credentials file keeps its "developer" key for that account's password.
import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { chmod, copyFile, lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import { createPilotLifecycle, finalizePilotResult, pilotCliFailure, pilotReceipt } from "./lib/pilot-lifecycle.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const hash = value => createHash("sha256").update(value).digest("hex");
const error = code => Object.assign(new Error(code), { safeCode: code });
const safe = value => typeof value === "string" && /^[a-zA-Z0-9_.:-]{1,160}$/.test(value) ? value : "unexpected_failure";
function requireThat(condition, code) { if (!condition) throw error(code); }

function command(program, args, { env, cwd, timeout = 120000 } = {}) {
  return new Promise((done, failed) => {
    const child = spawn(program, args, { env, cwd, stdio: ["ignore", "pipe", "pipe"] });
    const out = [], err = [];
    let size = 0;
    const keep = list => chunk => { size += chunk.length; if (size <= 16 << 20) list.push(chunk); };
    child.stdout.on("data", keep(out));
    child.stderr.on("data", keep(err));
    let timedOut = false;
    // Wait for close after killing the CLI: cleanup must not race a surviving
    // local process that can still submit the original request.
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, timeout);
    child.on("error", () => { clearTimeout(timer); failed(error("command_unavailable")); });
    child.on("close", code => {
      clearTimeout(timer);
      if (timedOut) { failed(error("command_timeout")); return; }
      done({ code, stdout: Buffer.concat(out).toString("utf8"), stderr: Buffer.concat(err).toString("utf8") });
    });
  });
}

const result = { schemaVersion: 2, harness: "pilot-flow-v2", startedAt: new Date().toISOString(), finishedAt: null,
  status: "running", passes: [], cleanupFailures: [] };
const passed = (name, details = {}) => { result.passes.push(pilotReceipt(name, details)); console.log("PASS " + name); };
let stage = "arguments", output, browser, executable, appId, lifecycle;
const sessions = {};
const usernames = { admin: "admin", developer: "developer" };

async function cli(account, args, { cwd, timeout, expectFailure } = {}) {
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("MIY_")));
  Object.assign(env, { MIY_CONFIG_DIR: join(output, "private", account), MIY_CREDENTIAL_BACKEND: "file", RUST_LOG: "off", CI: "true" });
  const run = await command(executable, ["--output", "json", "--no-input", "--context", "default", ...args],
    { env, cwd: cwd || output, timeout });
  let envelope;
  try { envelope = JSON.parse(run.stdout || run.stderr); } catch { throw error("cli_invalid_json"); }
  if (expectFailure) return envelope;
  if (run.code !== 0 || envelope?.ok !== true) throw pilotCliFailure(envelope?.error?.code, envelope?.error?.details);
  return envelope.data;
}

async function signIn(account, password, origins) {
  const context = await browser.newContext();
  context.setDefaultTimeout(45000);
  const page = await context.newPage();
  await page.goto(origins.console + "/auth/login");
  await page.locator("input[name=username]").fill(usernames[account]);
  await page.locator("input[name=password]").fill(password);
  await page.locator("#kc-login").click();
  try { await page.waitForURL(url => url.origin === origins.console && url.pathname === "/"); }
  catch { throw error("oidc_login_did_not_complete"); }
  const response = await context.request.get(origins.console + "/api/v1/me");
  requireThat(response.status() === 200, "console_identity_unavailable");
  const me = await response.json();
  requireThat(typeof me.id === "string" && me.admin === (account === "admin") && me.development === false
    && me.browserPurpose === "console", "ordinary_oidc_identity_required");
  return { context, page, me };
}

async function deviceLogin(account, origins) {
  const session = sessions[account];
  await cli(account, ["context", "init", "--url", origins.portal]);
  const pending = await cli(account, ["auth", "login", "--device-code", "--no-open", "--no-wait"]);
  requireThat(pending.status === "pending" && typeof pending.loginId === "string" && typeof pending.userCode === "string",
    "invalid_device_login_response");
  const verification = new URL(pending.verificationUrl);
  requireThat(verification.origin === origins.portal && verification.pathname === "/cli/device/" + pending.loginId,
    "invalid_device_approval_target");
  await session.page.goto(verification.href);
  requireThat(new URL(session.page.url()).origin === origins.console, "device_approval_did_not_reach_console");
  await session.page.locator("input[name=userCode]").fill(pending.userCode);
  await session.page.getByRole("button", { name: "이 계정으로 CLI 연결 승인", exact: true }).click();
  await session.page.getByRole("status").filter({ hasText: "CLI 연결을 승인했습니다" }).waitFor();
  const authenticated = await cli(account, ["auth", "wait", pending.loginId], { timeout: 120000 });
  requireThat(authenticated.status === "authenticated" && authenticated.userId === session.me.id, "cli_device_identity_mismatch");
  session.cli = true;
}

try {
  const values = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    const flag = process.argv[i], value = process.argv[i + 1];
    requireThat(["--origin", "--cli", "--credentials-file", "--output", "--publish-component", "--keep-running", "--unpublished",
      "--developer-username"].includes(flag)
      && value && !value.startsWith("--") && values[flag] === undefined, "invalid_arguments");
    values[flag] = value;
  }
  requireThat(["--origin", "--cli", "--credentials-file", "--output"].every(key => values[key]), "required_arguments_missing");
  requireThat(["--keep-running", "--unpublished"].every(key => values[key] === undefined || ["true", "false"].includes(values[key])), "invalid_boolean_argument");
  requireThat(isAbsolute(values["--cli"]) && isAbsolute(values["--credentials-file"]), "absolute_private_input_paths_required");
  if (values["--developer-username"] !== undefined) {
    requireThat(/^[a-z0-9][a-z0-9._-]{2,31}$/.test(values["--developer-username"]) && values["--developer-username"] !== "admin",
      "invalid_developer_username");
    usernames.developer = values["--developer-username"];
  }
  const origin = new URL(values["--origin"]).origin;
  output = resolve(values["--output"]);
  let existing;
  try { existing = await lstat(output); } catch (e) { if (e.code !== "ENOENT") throw e; }
  if (existing) requireThat(existing.isDirectory() && (await readdir(output)).length === 0, "output_must_be_new_or_empty_directory");
  await mkdir(join(output, "private"), { recursive: true, mode: 0o700 });
  await chmod(output, 0o700);
  await writeFile(join(output, ".gitignore"), "*\n", { mode: 0o600 });
  executable = join(output, "private", "miy");
  await copyFile(values["--cli"], executable);
  await chmod(executable, 0o700);
  result.cliSha256 = hash(await readFile(executable));
  result.build = "server";

  stage = "published_cli_binding";
  const publicJson = async url => {
    const response = await fetch(url, { redirect: "error", signal: AbortSignal.timeout(15000) });
    requireThat(response.status === 200, "public_http_status");
    return response.json();
  };
  const capabilities = await publicJson(origin + "/api/v1/capabilities");
  const version = (await command(executable, ["--version"])).stdout.trim().replace(/^miy /, "");
  requireThat(version === capabilities.serverVersion, "cli_server_version_mismatch");
  Object.assign(result, { cliVersion: version, serverVersion: capabilities.serverVersion, published: values["--unpublished"] !== "true" });
  if (result.published) {
    const guide = await publicJson(origin + "/api/v1/cli/install");
    requireThat(version === guide.cliVersion, "published_cli_server_version_mismatch");
    requireThat(guide.downloads.some(row => row.binarySha256 === result.cliSha256), "cli_not_in_published_install_guide");
    result.publishedSourceCommit = guide.sourceCommit;
  }
  const options = await publicJson(origin + "/auth/options");
  const origins = { portal: origin, console: options.consoleOrigin, devPortal: options.devPortalOrigin };
  requireThat([origins.console, origins.devPortal].every(value => typeof value === "string" && value.startsWith("http")), "auth_options_mismatch");
  passed(result.published ? "published_cli_matches_the_server_and_its_install_guide" : "working_tree_cli_matches_the_isolated_server", { version });

  stage = "oidc_and_device_logins";
  let credentials = JSON.parse(await readFile(values["--credentials-file"], "utf8"));
  requireThat(["admin", "developer"].every(name => typeof credentials[name] === "string" && credentials[name]), "oidc_credentials_required");
  browser = await chromium.launch({ channel: "chrome", headless: true, timeout: 30000 });
  for (const account of ["admin", "developer"]) {
    sessions[account] = await signIn(account, credentials[account], origins);
    await deviceLogin(account, origins);
  }
  credentials = null;
  requireThat(sessions.admin.me.id !== sessions.developer.me.id, "separate_accounts_required");
  passed("administrator_and_developer_oidc_sign_in_and_console_device_approval");

  if (values["--publish-component"]) {
    stage = "component_publication";
    const source = resolve(values["--publish-component"]);
    const published = await cli("admin", ["component", "publish", source]);
    requireThat(typeof published.version === "string" && /^[0-9a-f]{64}$/.test(published.sha256), "component_publication_failed");
    result.publishedComponent = { name: published.name, version: published.version, sha256: published.sha256 };
    passed("administrator_published_the_shared_component", result.publishedComponent);
  }

  stage = "new_project";
  appId = "pilot-" + randomBytes(4).toString("hex");
  const project = join(output, "project");
  const initialized = await cli("developer", ["app", "init", project, "--id", appId, "--name", "파일럿 인수 " + appId.slice(6)]);
  result.appId = appId;
  result.bundleVersion = initialized.bundleVersion;
  const mcp = JSON.parse(await readFile(join(project, ".mcp.json"), "utf8")).mcpServers?.miy_app;
  requireThat(mcp?.type === "stdio" && JSON.stringify(mcp.args) === '["mcp"]', "generated_mcp_configuration_mismatch");
  await cli("developer", ["component", "sync"], { cwd: project, timeout: 300000 });
  const check = await cli("developer", ["component", "check"], { cwd: project });
  result.components = (check.components || []).map(item => ({ name: item.name, version: item.version }));
  passed("app_init_generated_sdk_mcp_configuration_and_synchronized_components", { bundleVersion: result.bundleVersion });

  stage = "registration_and_approval";
  const registration = await cli("developer", ["app", "register"], { cwd: project });
  requireThat(typeof registration.id === "string", "registration_not_created");
  const denied = await cli("developer", ["registration", "approve", registration.id], { expectFailure: true });
  requireThat(denied?.ok === false && denied.error?.code === "forbidden", "developer_approved_own_registration");
  await cli("admin", ["registration", "approve", registration.id]);
  passed("developer_registration_refused_to_self_approve_and_approved_by_administrator");

  stage = "build";
  const git = async args => {
    const run = await command("git", ["-c", "user.name=Pilot acceptance", "-c", "user.email=pilot@miy.invalid", ...args], { cwd: project });
    requireThat(run.code === 0, "git_command_failed");
  };
  await git(["init", "-q"]);
  await git(["add", "."]);
  await git(["commit", "-qm", "Pilot acceptance app"]);
  const started = Date.now();
  const built = await cli("developer", ["app", "build", "--timeout", "2400"],
    { cwd: project, timeout: 2700000 });
  result.buildSeconds = Math.round((Date.now() - started) / 1000);
  result.imageId = built.imageId ?? built.image?.id ?? null;
  passed("server_build_from_committed_source", { seconds: result.buildSeconds });

  stage = "dev_deployment";
  lifecycle = createPilotLifecycle({ appId, keepRunning: values["--keep-running"] === "true",
    cli: (args, options) => cli("developer", args, { cwd: project, ...options }) });
  result.runtime = lifecycle.state;
  const installation = await lifecycle.deploy();
  result.installationId = installation.id;
  passed("dev_deployment_running", { installationId: installation.id });

  stage = "frame_on_dev_portal";
  const { context, page } = sessions.developer;
  await page.goto(origins.devPortal + "/auth/login");
  try { await page.waitForURL(url => url.origin === origins.devPortal && url.pathname === "/"); }
  catch { throw error("dev_portal_sso_did_not_complete"); }
  await page.goto(origins.devPortal + "/environments/dev/apps/" + appId);
  const frame = page.frameLocator("iframe.app-frame");
  await frame.getByText("개발 준비가 완료됐습니다.").waitFor({ timeout: 60000 });
  // The portal launches the frame with a form POST: read where it actually is.
  const launched = await (await page.locator("iframe.app-frame").elementHandle()).contentFrame();
  const frameOrigin = new URL(launched.url()).origin;
  result.frameOriginSha256 = hash(frameOrigin);
  requireThat(frameOrigin !== origins.devPortal && frameOrigin !== origins.portal && frameOrigin !== origins.console, "app_frame_not_on_its_own_origin");
  const portalCookieOnAppApi = await context.request.get(origins.devPortal + "/api/v1/installations/" + installation.id + "/api/ping", { maxRedirects: 0 });
  requireThat([401, 403, 404].includes(portalCookieOnAppApi.status()), "portal_cookie_reached_app_api");
  await page.screenshot({ path: join(output, "app-frame.png") });
  passed("generated_app_renders_in_its_own_frame_origin_on_the_dev_portal");

  result.status = "passed";
} catch (failure) {
  result.status = "failed";
  result.failure = { stage, code: safe(failure?.safeCode) };
  console.error("FAIL " + stage + ": " + result.failure.code);
} finally {
  const cleanup = async (name, action) => { try { await action(); } catch { result.cleanupFailures.push(name); } };
  if (lifecycle) {
    result.pilotCleanup = await lifecycle.cleanup();
    result.stopped = result.pilotCleanup.stopped;
    if (result.pilotCleanup.status === "failed") result.cleanupFailures.push("pilot_app:" + result.pilotCleanup.code);
  }
  for (const account of Object.keys(sessions))
    if (sessions[account].cli) await cleanup(account + "_cli_logout", () => cli(account, ["auth", "logout"]));
  if (browser) await cleanup("browser_close", () => browser.close());
  result.finishedAt = new Date().toISOString();
  const exitCode = finalizePilotResult(result);
  if (output) await writeFile(join(output, "result.json"), JSON.stringify(result, null, 2) + "\n", { mode: 0o600 }).catch(() => {});
  console.log(JSON.stringify({ status: result.status, appId: result.appId ?? null, passed: result.passes.length,
    cleanupFailures: result.cleanupFailures.length, evidence: output ?? null }));
  process.exit(exitCode);
}
