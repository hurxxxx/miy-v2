#!/usr/bin/env node
// Authenticates once through ordinary OIDC, then exercises read-only platform APIs.
// Only this run's browser and CLI sessions are revoked. No app is registered.
// Example: node scripts/check-platform-connection.mjs --origin https://platform.example
//   --cli /absolute/miy --credentials-file /private/credentials.json --output /private/new-evidence
// The credentials file is { "developer": "password" }. It is never copied or logged.
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { chmod, copyFile, lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

process.umask(0o077);
const harness = fileURLToPath(import.meta.url), root = resolve(dirname(harness), "..");
const limit = 16 * 1024 * 1024;
const versionPattern = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const hash = value => createHash("sha256").update(value).digest("hex");
const error = code => Object.assign(new Error(code), { safeCode: code });
function requireThat(condition, code) { if (!condition) throw error(code); }
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]));
  return value;
}
const digest = value => hash(JSON.stringify(canonical(value)));
const rowDigest = rows => digest(rows.map(canonical).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))));
function signal(child, kind) {
  try { if (process.platform === "win32") child.kill(kind); else process.kill(-child.pid, kind); } catch { /* Already exited. */ }
}
async function bounded(promise, milliseconds, code) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(error(code)), milliseconds); })]); }
  finally { clearTimeout(timer); }
}
function command(executable, args, options = {}) {
  return new Promise((resolveCommand, reject) => {
    const child = spawn(executable, args, { cwd: options.cwd || root, env: options.env || process.env,
      detached: process.platform !== "win32", stdio: ["ignore", "pipe", "ignore"] });
    const chunks = [];
    let size = 0, failure, settled = false, killTimer, finishTimer;
    const settle = (code, exitSignal) => {
      if (settled) return;
      settled = true; clearTimeout(timer); clearTimeout(killTimer); clearTimeout(finishTimer);
      if (failure) reject(error(failure));
      else if (code !== 0 || exitSignal) reject(error("subprocess_failed"));
      else resolveCommand(Buffer.concat(chunks).toString("utf8"));
    };
    const stop = code => {
      if (failure) return;
      failure = code; signal(child, "SIGTERM");
      killTimer = setTimeout(() => signal(child, "SIGKILL"), 1000);
      finishTimer = setTimeout(() => { signal(child, "SIGKILL"); child.stdout.destroy(); child.unref(); settle(null); }, 2000);
    };
    const timer = setTimeout(() => stop("subprocess_timeout"), options.timeout || 60000);
    child.on("error", () => { failure = "subprocess_start_failed"; settle(null); });
    child.stdout.on("data", chunk => { size += chunk.length; if (size > limit) stop("subprocess_output_limit"); else chunks.push(chunk); });
    child.once("close", settle);
  });
}
async function publicResponse(url, expectedStatus = 200) {
  const controller = new AbortController(), timer = setTimeout(() => controller.abort(), 20000);
  try {
    const response = await fetch(url, { redirect: "manual", signal: controller.signal, headers: { accept: "application/json" } });
    requireThat(response.status === expectedStatus, "public_http_status_mismatch");
    const chunks = []; let size = 0;
    for await (const chunk of response.body || []) {
      size += chunk.length; requireThat(size <= limit, "public_http_body_limit"); chunks.push(chunk);
    }
    return { status: response.status, body: Buffer.concat(chunks).toString("utf8") };
  } finally { clearTimeout(timer); }
}
function json(text, code) { try { return JSON.parse(text); } catch { throw error(code); } }
async function publicJson(url) { return json((await publicResponse(url)).body, "public_http_invalid_json"); }

function startMcp(configured, project, env) {
  const child = spawn(configured.command, configured.args, { cwd: project, env: { ...env, ...configured.env },
    detached: process.platform !== "win32", stdio: ["pipe", "pipe", "ignore"] });
  let buffer = "", size = 0, sequence = 0, failure, exited = false;
  const pending = new Map();
  const rejectPending = code => {
    failure = code;
    for (const request of pending.values()) { clearTimeout(request.timer); request.reject(error(code)); }
    pending.clear();
  };
  const closed = new Promise(resolveClosed => child.once("close", () => {
    exited = true; rejectPending("mcp_process_closed"); resolveClosed();
  }));
  child.once("error", () => rejectPending("mcp_process_start_failed"));
  child.stdin.on("error", () => rejectPending("mcp_input_closed"));
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", chunk => {
    size += Buffer.byteLength(chunk);
    if (size > limit) { rejectPending("mcp_output_limit"); signal(child, "SIGTERM"); return; }
    buffer += chunk;
    let boundary;
    while ((boundary = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, boundary); buffer = buffer.slice(boundary + 1);
      if (!line.trim()) continue;
      let reply;
      try { reply = JSON.parse(line); } catch { rejectPending("mcp_invalid_json"); return; }
      if (reply?.jsonrpc !== "2.0") { rejectPending("mcp_invalid_protocol"); return; }
      const request = pending.get(reply.id);
      if (!request) continue;
      clearTimeout(request.timer); pending.delete(reply.id);
      if (reply.error) request.reject(error("mcp_protocol_error")); else request.resolve(reply.result);
    }
  });
  const call = (method, params) => {
    if (failure || exited) return Promise.reject(error(failure || "mcp_process_closed"));
    const id = ++sequence;
    return new Promise((resolveCall, reject) => {
      const timer = setTimeout(() => { pending.delete(id); reject(error("mcp_request_timeout")); }, 30000);
      pending.set(id, { resolve: resolveCall, reject, timer });
      child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  };
  return {
    call,
    initialized() { child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n"); },
    async tool(name, args) {
      const result = await call("tools/call", { name, arguments: args });
      requireThat(result && !result.isError && Array.isArray(result.content), "mcp_tool_failed");
      return json(result.content.find(block => block.type === "text")?.text, "mcp_tool_invalid_json");
    },
    async close() {
      rejectPending("mcp_shutting_down"); child.stdin.end();
      try { await bounded(closed, 3000, "mcp_shutdown_timeout"); }
      catch { signal(child, "SIGKILL"); await bounded(closed, 2000, "mcp_shutdown_timeout"); }
    },
  };
}

const result = { schemaVersion: 1, startedAt: new Date().toISOString(), finishedAt: null, status: "running",
  scope: "ordinary_oidc_and_read_only_platform_cli_stdio_mcp", executionCliSha256: null, cliVersion: null,
  observedServerVersion: null, observedInstallCliVersion: null, repositoryCommit: null, harnessCommit: null,
  harnessSha256: null, passes: [], cleanupFailures: [] };
let output, outputReady = false, stage = "arguments", browser, context, me, mcp, cli, cliStarted = false;
const passed = (name, details = {}) => { result.passes.push({ name, status: "passed", ...details }); console.log("PASS " + name); };
const cleanup = async (name, action) => {
  try { await action(); passed(name); } catch { result.cleanupFailures.push(name); }
};
try {
  const values = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    const flag = process.argv[i], value = process.argv[i + 1];
    requireThat(["--origin", "--cli", "--credentials-file", "--output", "--browser-channel"].includes(flag)
      && value && !value.startsWith("--") && values[flag] === undefined, "invalid_arguments");
    values[flag] = value;
  }
  requireThat(["--origin", "--cli", "--credentials-file", "--output"].every(key => values[key]), "required_arguments_missing");
  requireThat(isAbsolute(values["--cli"]) && isAbsolute(values["--credentials-file"]), "absolute_private_input_paths_required");
  const originUrl = new URL(values["--origin"]), origin = originUrl.origin;
  requireThat(["http:", "https:"].includes(originUrl.protocol) && !originUrl.username && !originUrl.password
    && originUrl.pathname === "/" && !originUrl.search && !originUrl.hash, "invalid_server_origin");
  requireThat((await lstat(values["--cli"])).isFile() && (await lstat(values["--credentials-file"])).isFile(), "regular_private_input_files_required");
  output = resolve(values["--output"]);
  let outputStat;
  try { outputStat = await lstat(output); } catch (e) { if (e.code !== "ENOENT") throw e; }
  if (outputStat) requireThat(outputStat.isDirectory() && !outputStat.isSymbolicLink() && (await readdir(output)).length === 0, "output_must_be_new_or_empty_directory");
  await mkdir(output, { recursive: true, mode: 0o700 }); await chmod(output, 0o700); outputReady = true;
  await writeFile(join(output, ".gitignore"), "*\n", { mode: 0o600 });
  const config = join(output, "config"), bin = join(output, "bin"), project = join(output, "project");
  await mkdir(config, { mode: 0o700 }); await mkdir(bin, { mode: 0o700 });
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("MIY_")));
  Object.assign(env, { MIY_CONFIG_DIR: config, MIY_CREDENTIAL_BACKEND: "file", RUST_LOG: "off" });
  const suffix = process.platform === "win32" ? ".exe" : "", executable = join(bin, "miy" + suffix);
  result.originSha256 = hash(origin);
  result.executionCliSha256 = hash(await readFile(values["--cli"]));
  await copyFile(values["--cli"], executable); await chmod(executable, 0o700);
  requireThat(hash(await readFile(executable)) === result.executionCliSha256, "cli_copy_hash_mismatch");
  result.repositoryCommit = (await command("git", ["rev-parse", "HEAD"])).trim();
  requireThat(/^[a-f0-9]{40}$/.test(result.repositoryCommit), "invalid_repository_commit");
  const harnessCommit = (await command("git", ["log", "-1", "--format=%H", "--", "scripts/check-platform-connection.mjs"])).trim();
  result.harnessCommit = /^[a-f0-9]{40}$/.test(harnessCommit) ? harnessCommit : null;
  result.harnessWorkingTreeChanges = (await command("git", ["status", "--porcelain", "--", "scripts/check-platform-connection.mjs"])).trim().length > 0;
  result.harnessSha256 = hash(await readFile(harness));
  const cliVersion = (await command(executable, ["--version"], { env, cwd: output })).trim();
  requireThat(cliVersion.startsWith("miy ") && versionPattern.test(cliVersion.slice(4)), "invalid_cli_version");
  result.cliVersion = cliVersion.slice(4);
  cli = async (args, options = {}) => {
    const envelope = json(await command(executable, ["--output", "json", "--no-input", "--context", "default", ...args],
      { env, cwd: options.cwd || output, timeout: options.timeout }), "cli_invalid_json");
    requireThat(envelope?.schemaVersion === 1 && envelope.ok === true && envelope.error === null && Object.hasOwn(envelope, "data"), "cli_invalid_success_envelope");
    return envelope.data;
  };
  passed("isolated_private_config_and_execution_cli_provenance");

  stage = "public_platform_metadata";
  requireThat((await publicJson(origin + "/api/health")).status === "ok", "public_health_not_ready");
  const capabilities = await publicJson(origin + "/api/v1/capabilities"), guide = await publicJson(origin + "/api/v1/cli/install");
  requireThat(capabilities.schemaVersion === 1 && capabilities.cliContractVersion === 1 && versionPattern.test(capabilities.serverVersion), "server_capabilities_mismatch");
  result.observedServerVersion = capabilities.serverVersion;
  requireThat(guide.schemaVersion === 1 && guide.serverUrl === origin && versionPattern.test(guide.cliVersion), "install_guide_origin_mismatch");
  result.observedInstallCliVersion = guide.cliVersion;
  requireThat(guide.cliVersion === result.cliVersion && capabilities.serverVersion === result.cliVersion, "published_cli_server_version_mismatch");
  requireThat(Array.isArray(guide.downloads) && guide.downloads.some(row => row.binarySha256 === result.executionCliSha256), "execution_cli_not_in_published_install_guide");
  if (/^[a-f0-9]{40}$/.test(guide.sourceCommit)) result.publishedSourceCommit = guide.sourceCommit;
  passed("public_health_capabilities_and_published_cli_install_binding", { publishedArtifactCount: guide.downloads.length });
  const metadata = await publicJson(origin + "/.well-known/oauth-protected-resource/mcp");
  requireThat(metadata.resource === origin + "/mcp" && Array.isArray(metadata.authorization_servers) && metadata.authorization_servers.length === 1, "mcp_resource_metadata_mismatch");
  const issuer = new URL(metadata.authorization_servers[0]);
  requireThat(["http:", "https:"].includes(issuer.protocol) && !issuer.username && !issuer.password && !issuer.search && !issuer.hash
    && (originUrl.protocol !== "https:" || issuer.protocol === "https:"), "invalid_oidc_issuer");
  const discovery = await publicJson(issuer.href.replace(/\/$/, "") + "/.well-known/openid-configuration");
  requireThat(discovery.issuer === metadata.authorization_servers[0] && new URL(discovery.authorization_endpoint).origin === issuer.origin, "oidc_discovery_mismatch");
  await publicResponse(origin + "/mcp", 401);
  passed("oidc_discovery_and_http_mcp_unauthenticated_401");

  stage = "ordinary_oidc_login";
  let credentials = json(await readFile(values["--credentials-file"], "utf8"), "invalid_private_credentials_file");
  requireThat(typeof credentials.developer === "string" && credentials.developer.length > 0, "developer_oidc_credential_required");
  browser = await chromium.launch({ channel: values["--browser-channel"] || "chrome", headless: true, timeout: 30000 });
  context = await browser.newContext(); context.setDefaultTimeout(45000);
  const page = await context.newPage();
  await page.goto(origin + "/auth/login");
  const loginUrl = new URL(page.url());
  requireThat(loginUrl.origin === issuer.origin && loginUrl.pathname.startsWith(issuer.pathname.replace(/\/$/, "") + "/"), "oidc_login_target_mismatch");
  await page.locator("input[name=username]").fill("developer");
  await page.locator("input[name=password]").fill(credentials.developer);
  credentials = null;
  await page.locator("#kc-login").click();
  // Keycloak may share this origin under /identity; only the callback's home
  // redirect counts as successful login. Never retry the supplied password.
  try { await page.waitForURL(url => url.origin === origin && url.pathname === "/"); }
  catch { throw error("oidc_login_did_not_complete"); }
  const api = async path => {
    const response = await context.request.get(origin + path, { timeout: 20000, maxRedirects: 0 });
    requireThat(response.status() === 200, "authenticated_api_status_mismatch");
    const body = await response.body(); requireThat(body.length <= limit, "authenticated_api_body_limit");
    return json(body.toString("utf8"), "authenticated_api_invalid_json");
  };
  me = await api("/api/v1/me");
  requireThat(typeof me.id === "string" && me.id.length > 0 && me.admin === false && me.development === false
    && typeof me.csrf === "string" && me.csrf.length > 0, "ordinary_developer_oidc_identity_required");
  passed("ordinary_developer_oidc_login_without_shared_development_session");

  stage = "cli_context_and_device_approval";
  await cli(["context", "init", "--url", origin]);
  const selected = await cli(["context", "show"]);
  requireThat(selected.name === "default" && selected.context?.origin === origin, "isolated_context_origin_mismatch");
  cliStarted = true;
  const pending = await cli(["auth", "login", "--device-code", "--no-open", "--no-wait"]);
  requireThat(pending.status === "pending" && typeof pending.loginId === "string" && typeof pending.userCode === "string", "invalid_device_login_response");
  const verification = new URL(pending.verificationUrl);
  requireThat(verification.origin === origin && verification.pathname === "/cli/device/" + pending.loginId
    && !verification.username && !verification.password && !verification.search && !verification.hash, "invalid_device_approval_target");
  await page.goto(verification.href);
  await page.locator("input[name=userCode]").fill(pending.userCode);
  await page.getByRole("button", { name: "이 계정으로 CLI 연결 승인", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "CLI 연결을 승인했습니다" }).waitFor();
  const authenticated = await cli(["auth", "wait", pending.loginId], { timeout: 120000 });
  requireThat(authenticated.status === "authenticated" && authenticated.userId === me.id && typeof authenticated.sessionId === "string", "cli_device_identity_mismatch");
  const status = await cli(["auth", "status"]);
  requireThat(status.authenticated === true && status.userId === me.id && status.sessionId === authenticated.sessionId, "cli_session_status_mismatch");
  passed("isolated_cli_device_login_same_browser_identity_and_auth_status");
  const doctor = await cli(["doctor", "--for", "operate"]);
  requireThat(doctor.ready === true && Array.isArray(doctor.checks), "operate_doctor_not_ready");
  passed("authenticated_cli_doctor_operate", { checkCount: doctor.checks.length });

  stage = "local_project_initialization";
  const initialized = await cli(["app", "init", project, "--id", "connection-probe-" + randomUUID().slice(0, 8), "--name", "Platform connection probe"]);
  requireThat(versionPattern.test(initialized.bundleVersion), "local_project_bundle_version_missing");
  result.bundleVersion = initialized.bundleVersion;
  const lock = json(await readFile(join(project, "miy.bundle.json"), "utf8"), "local_bundle_lock_invalid");
  requireThat(lock.bundleVersion === result.bundleVersion, "local_bundle_version_mismatch");
  const configured = json(await readFile(join(project, ".mcp.json"), "utf8"), "generated_mcp_json_invalid").mcpServers?.miy_app;
  requireThat(configured?.type === "stdio" && configured.command === join(project, ".miy-tools", "miy" + suffix)
    && JSON.stringify(configured.args) === '["mcp"]' && configured.env?.MIY_APP_DIRECTORY === project, "generated_mcp_configuration_mismatch");
  requireThat(hash(await readFile(configured.command)) === result.executionCliSha256, "project_cli_hash_mismatch");
  passed("local_app_init_and_generated_mcp_configuration_without_remote_registration");

  stage = "generated_stdio_mcp_live_read_tools";
  mcp = startMcp(configured, project, env);
  const initializedMcp = await mcp.call("initialize", { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "platform-connection-check", version: "1" } });
  requireThat(initializedMcp?.serverInfo, "mcp_initialize_failed"); mcp.initialized();
  const listed = await mcp.call("tools/list", {});
  requireThat(["get_contract", "search_api", "list_components", "get_cli_help"].every(name => listed.tools?.some(item => item.name === name)), "mcp_required_tool_missing");
  passed("generated_stdio_mcp_initialize_and_required_tools", { toolCount: listed.tools.length });
  const contract = await mcp.tool("get_contract", {}), liveContract = await api("/api/v1/contracts");
  requireThat(contract.platformBundleVersion === liveContract.bundleVersion && contract.bundleVersion === result.bundleVersion
    && Object.keys(contract.protectedFiles || {}).length > 0 && digest(contract.protectedFiles) === digest(lock.protectedFiles), "mcp_live_contract_mismatch");
  passed("mcp_get_contract_matches_live_version_and_installed_bundle", { protectedFileCount: Object.keys(contract.protectedFiles).length, protectedFilesSha256: digest(contract.protectedFiles) });
  const catalog = await mcp.tool("search_api", { query: "" }), liveCatalog = await api("/api/v1/catalog");
  requireThat(Array.isArray(catalog) && catalog.length > 0 && Array.isArray(liveCatalog) && rowDigest(catalog) === rowDigest(liveCatalog), "mcp_live_api_catalog_mismatch");
  passed("mcp_search_api_matches_authenticated_http_catalog", { apiCount: catalog.length, catalogSha256: rowDigest(catalog) });
  const components = await mcp.tool("list_components", { query: "" }), liveComponents = await api("/api/v1/components");
  requireThat(Array.isArray(components) && Array.isArray(liveComponents) && rowDigest(components) === rowDigest(liveComponents), "mcp_live_components_mismatch");
  passed("mcp_list_components_matches_authenticated_http_components", { componentCount: components.length, componentsSha256: rowDigest(components) });
  const help = await mcp.tool("get_cli_help", { command: "app dev" }), directHelp = await cli(["describe", "--command", "app dev"], { cwd: project });
  requireThat(help.scope === "command" && help.cliVersion === result.cliVersion && digest(help) === digest(directHelp), "mcp_cli_help_mismatch");
  passed("mcp_get_cli_help_matches_executed_cli_describe", { commandCount: 1, descriptionSha256: digest(help) });
} catch (failure) {
  result.failure = { stage, code: failure.safeCode || "verification_failed" };
} finally {
  if (mcp) await cleanup("owned_stdio_mcp_process_closed", () => mcp.close());
  if (cliStarted && cli) await cleanup("owned_cli_session_logged_out", async () => {
    const loggedOut = await cli(["auth", "logout"]);
    requireThat(loggedOut.signedOut === true && (loggedOut.alreadySignedOut === true
      || (loggedOut.remoteRevoked === true && loggedOut.localCredentialsRemoved === true)), "owned_cli_logout_unconfirmed");
  });
  if (context) await cleanup("owned_browser_platform_session_logged_out", async () => {
    // Also handles a session created just before a failed identity assertion.
    const current = await context.request.get(new URL("/api/v1/me", process.argv[process.argv.indexOf("--origin") + 1]).href, { timeout: 15000, maxRedirects: 0 });
    if (current.status() === 401) return;
    requireThat(current.status() === 200, "browser_cleanup_identity_unavailable");
    const identity = await current.json();
    requireThat(typeof identity.csrf === "string" && identity.csrf.length > 0, "browser_cleanup_csrf_missing");
    const origin = new URL(process.argv[process.argv.indexOf("--origin") + 1]).origin;
    const revoked = await context.request.post(origin + "/auth/logout", { timeout: 15000, maxRedirects: 0,
      headers: { origin, "x-miy-csrf": identity.csrf } });
    requireThat(revoked.status() === 200, "browser_session_logout_failed");
    // Intentionally do not follow the returned IdP logout URL or revoke user-wide sessions.
    const after = await context.request.get(origin + "/api/v1/me", { timeout: 15000, maxRedirects: 0 });
    requireThat(after.status() === 401, "browser_session_logout_unconfirmed");
  });
  if (context) await cleanup("owned_browser_context_closed", () => bounded(context.close(), 5000, "browser_context_close_timeout"));
  if (browser) await cleanup("owned_headless_browser_closed", () => bounded(browser.close(), 5000, "browser_close_timeout"));
  result.finishedAt = new Date().toISOString();
  result.status = result.failure || result.cleanupFailures.length ? "failed" : "passed";
  if (outputReady) {
    try { await writeFile(join(output, "result.json"), JSON.stringify(result, null, 2) + "\n", { mode: 0o600 }); }
    catch { result.status = "failed"; result.cleanupFailures.push("private_evidence_write_failed"); }
  }
  console.log(JSON.stringify({ status: result.status, passCount: result.passes.length, failure: result.failure || null, cleanupFailures: result.cleanupFailures }));
  process.exitCode = result.status === "passed" ? 0 : 1;
}
