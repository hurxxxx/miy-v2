#!/usr/bin/env node
// Read-only browser acceptance for the approved two-origin deployment.
// Preparation: node --check <this-file>. No network is used without --run-after-deploy.
// No screenshots, traces, response bodies, passwords, cookies or app names are saved.
import { createHash } from "node:crypto";
import { lstat, readFile, realpath, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

process.umask(0o077);
delete process.env.DEBUG;
delete process.env.DEBUG_FILE;
delete process.env.PWDEBUG;
delete process.env.PW_LOG_FILE;

const harness = fileURLToPath(import.meta.url);
const evidenceDirectory = dirname(harness);
const portalOrigin = "https://v2.dwdcc.kr";
const consoleOrigin = "https://v2.dwdcc.kr:9443";
const bodyLimit = 4 * 1024 * 1024;
const settleWindowMs = 1200;
const hash = (value) => createHash("sha256").update(value).digest("hex");
const failure = (safeCode) => Object.assign(new Error(safeCode), { safeCode });
function requireThat(condition, code) { if (!condition) throw failure(code); }
const safeCode = (error, fallback) => /^[a-z0-9_]+$/.test(error?.safeCode || "") ? error.safeCode : fallback;
const result = {
  schemaVersion: 1,
  scope: "production_read_only_existing_dev_mf_and_console_browser_boundaries",
  expectedDeploymentCommit: "a4ad370",
  startedAt: new Date().toISOString(),
  finishedAt: null,
  status: "NOT_RUN",
  harnessSha256: null,
  tlsVerification: "required_no_bypass",
  screenshotsTracesAndRawResponses: "disabled",
  settleWindowMs,
  portalChecks: [],
  consoleChecks: [],
  appCounts: { visibleDev: 0, runningDev: 0, selected: 0, passed: 0, failed: 0 },
  apps: [],
  blockedBrowserWrites: 0,
  blockedWebSockets: 0,
  requestGuardFailures: 0,
  cleanup: [],
};
let browser, context, resultFile, stage = "arguments", phase = "preparation";
let failureCode;
const statsByPage = new WeakMap();

function parseArguments() {
  const values = {};
  for (let i = 2; i < process.argv.length; i++) {
    const flag = process.argv[i];
    if (flag === "--run-after-deploy") {
      requireThat(!values[flag], "duplicate_argument");
      values[flag] = true;
      continue;
    }
    requireThat(["--credentials-file", "--result-file", "--browser-channel", "--max-apps"].includes(flag)
      && !values[flag] && process.argv[i + 1] && !process.argv[i + 1].startsWith("--"), "invalid_arguments");
    values[flag] = process.argv[++i];
  }
  requireThat(values["--run-after-deploy"] === true, "explicit_postdeploy_run_flag_required");
  requireThat(isAbsolute(values["--credentials-file"] || ""), "absolute_credentials_path_required");
  const maxApps = Number(values["--max-apps"] || "3");
  requireThat(Number.isInteger(maxApps) && maxApps >= 1 && maxApps <= 3, "invalid_max_apps");
  requireThat(process.env.NODE_TLS_REJECT_UNAUTHORIZED !== "0", "tls_bypass_environment_forbidden");
  return { ...values, maxApps };
}

async function checkedJson(response, expectedStatus, code) {
  requireThat(response.status() === expectedStatus, code + "_status");
  const body = await response.body();
  requireThat(body.length <= bodyLimit, code + "_body_limit");
  try { return JSON.parse(body.toString("utf8")); }
  catch { throw failure(code + "_invalid_json"); }
}

async function apiJson(origin, path, code) {
  const response = await context.request.get(origin + path, { timeout: 20000, maxRedirects: 0 });
  return checkedJson(response, 200, code);
}

function isAppContent(url) {
  return /^\/(artifacts|preview)(\/|$)/.test(url.pathname)
    || /^\/api\/v1\/preview(\/|$)/.test(url.pathname)
    || /^\/api\/v1\/installations\/[^/]+\/(api(\/|$)|connections\/[^/]+\/api(\/|$))/.test(url.pathname);
}

function trackPage(page, kind) {
  const stats = { kind, active: true, pageErrors: 0, consoleErrors: 0, httpFailures: 0,
    failedRequests: 0, abortedNavigations: 0, unexpectedDialogs: 0, blockedWrites: 0, blockedWebSockets: 0,
    remoteScriptRequests: 0, appContentRequests: 0, artifact200: 0, artifactOther: 0 };
  statsByPage.set(page, stats);
  page.on("pageerror", () => { if (stats.active) stats.pageErrors++; });
  page.on("console", (message) => { if (stats.active && message.type() === "error") stats.consoleErrors++; });
  page.on("dialog", async (dialog) => {
    if (stats.active) stats.unexpectedDialogs++;
    try { await dialog.dismiss(); } catch { /* No dialog data is retained. */ }
  });
  page.on("request", (request) => {
    if (!stats.active) return;
    let url;
    try { url = new URL(request.url()); } catch { return; }
    if (kind === "console") {
      if (isAppContent(url)) stats.appContentRequests++;
      if (request.resourceType() === "script" &&
          (url.origin !== consoleOrigin || isAppContent(url) || /\/AppView[-.][^/]*\.js$/.test(url.pathname)))
        stats.remoteScriptRequests++;
    }
  });
  page.on("response", (response) => {
    if (!stats.active) return;
    let url;
    try { url = new URL(response.url()); } catch { return; }
    if (response.status() >= 400) stats.httpFailures++;
    if (url.origin === portalOrigin && url.pathname.startsWith("/artifacts/")) {
      if (response.status() === 200) stats.artifact200++;
      else stats.artifactOther++;
    }
  });
  page.on("requestfailed", (request) => {
    if (!stats.active) return;
    // Page transitions can cancel already-started GET polling. Count these
    // separately; retain no URL, error text or credential-bearing headers.
    if (request.failure()?.errorText === "net::ERR_ABORTED") stats.abortedNavigations++;
    else stats.failedRequests++;
  });
  return stats;
}

function metrics(stats) {
  const { active: _active, kind: _kind, ...counts } = stats;
  return { ...counts };
}

function requireNoUnexpectedErrors(stats, code) {
  requireThat(stats.pageErrors === 0 && stats.consoleErrors === 0 && stats.httpFailures === 0
    && stats.failedRequests === 0 && stats.unexpectedDialogs === 0, code);
  requireThat(stats.blockedWrites === 0 && stats.blockedWebSockets === 0, "automatic_app_write_or_socket_prevented");
}

async function waitShell(page, origin) {
  await page.waitForURL((url) => url.origin === origin && url.pathname === "/", { timeout: 45000 });
  await page.getByRole("button", { name: "로그아웃", exact: true }).waitFor({ state: "visible", timeout: 30000 });
}

async function checkMounted(page, appId, expectedPath, stats) {
  requireThat(new URL(page.url()).origin === portalOrigin && new URL(page.url()).pathname === expectedPath,
    "app_navigation_target_mismatch");
  const appRoot = page.locator(`.miy-app-root[data-miy-app="${appId}"]`);
  await appRoot.waitFor({ state: "visible", timeout: 45000 });
  requireThat(await appRoot.evaluate((element) => element.childElementCount > 0), "empty_remote_app_root");
  requireThat(await page.locator("iframe").count() === 0, "unexpected_app_iframe");
  await page.waitForTimeout(settleWindowMs);
  requireNoUnexpectedErrors(stats, "app_browser_error_observed");
}

async function inspectApp(app, index) {
  const report = { appIdSha256: hash(app.appId), kind: app.kind === "docker_app" ? "docker_app" : "other",
    status: "running", checks: [], metrics: null };
  result.apps.push(report);
  const page = await context.newPage();
  const stats = trackPage(page, "app");
  const appPath = `/environments/dev/apps/${app.appId}`;
  let appStage = "app_initial_navigation";
  try {
    await page.goto(portalOrigin + "/environments/dev", { waitUntil: "load", timeout: 30000 });
    await page.getByRole("heading", { name: "테스트 앱", exact: true }).waitFor();
    await page.goto(portalOrigin + appPath, { waitUntil: "load", timeout: 30000 });
    await checkMounted(page, app.appId, appPath, stats);
    requireThat(stats.artifact200 > 0 && stats.artifactOther === 0, "app_artifact_http_not_200");
    report.checks.push("mf_mounted_with_200_assets_no_iframe");

    appStage = "app_back_forward";
    await page.goBack({ waitUntil: "load", timeout: 30000 });
    requireThat(new URL(page.url()).origin === portalOrigin && new URL(page.url()).pathname === "/environments/dev",
      "back_navigation_target_mismatch");
    await page.getByRole("heading", { name: "테스트 앱", exact: true }).waitFor();
    await page.goForward({ waitUntil: "load", timeout: 30000 });
    await checkMounted(page, app.appId, appPath, stats);
    report.checks.push("read_only_back_and_forward");

    appStage = "app_reload";
    const assetsBeforeReload = stats.artifact200;
    await page.reload({ waitUntil: "load", timeout: 30000 });
    await checkMounted(page, app.appId, appPath, stats);
    requireThat(stats.artifact200 > assetsBeforeReload && stats.artifactOther === 0, "reload_artifact_http_not_200");
    report.checks.push("read_only_reload_with_200_assets");
    report.status = "PASS";
    result.appCounts.passed++;
  } catch (error) {
    report.status = "FAIL";
    report.failureCode = safeCode(error, appStage + "_failed");
    result.appCounts.failed++;
  } finally {
    report.metrics = metrics(stats);
    stats.active = false;
    try { await page.close({ runBeforeUnload: false }); } catch { /* Context cleanup follows. */ }
  }
  console.log(`APP_CHECK ${index + 1} ${report.status}`);
}

async function logoutPurpose(purpose, origin) {
  const current = await context.request.get(origin + "/api/v1/me", { timeout: 15000, maxRedirects: 0 });
  if (current.status() === 401) return;
  const identity = await checkedJson(current, 200, "cleanup_identity");
  requireThat(identity.browserPurpose === purpose && typeof identity.csrf === "string" && identity.csrf,
    "cleanup_identity_purpose_mismatch");
  const loggedOut = await context.request.post(origin + "/auth/logout", {
    timeout: 15000, maxRedirects: 0, headers: { origin, "x-miy-csrf": identity.csrf },
  });
  requireThat(loggedOut.status() === 200, "purpose_logout_failed");
  // Do not follow the IdP redirect or revoke account-wide sessions.
  const after = await context.request.get(origin + "/api/v1/me", { timeout: 15000, maxRedirects: 0 });
  requireThat(after.status() === 401, "purpose_logout_unconfirmed");
}

try {
  const options = parseArguments();
  const credentialsPath = options["--credentials-file"];
  requireThat((await lstat(credentialsPath)).isFile(), "regular_credentials_file_required");
  const candidateResult = resolve(options["--result-file"] || join(evidenceDirectory,
    `ui-readonly-${new Date().toISOString().replaceAll(/[:.]/g, "-")}-${process.pid}.json`));
  requireThat(await realpath(dirname(candidateResult)) === await realpath(evidenceDirectory)
    && basename(candidateResult).endsWith(".json"), "result_must_be_inside_approved_evidence_directory");
  try { await lstat(candidateResult); throw failure("result_file_already_exists"); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  resultFile = candidateResult;
  result.harnessSha256 = hash(await readFile(harness));

  const { chromium } = await import("@playwright/test");
  browser = await chromium.launch({ channel: options["--browser-channel"] || "chrome", headless: true, timeout: 30000 });
  context = await browser.newContext({ ignoreHTTPSErrors: false, acceptDownloads: false, serviceWorkers: "block" });
  context.setDefaultTimeout(30000);
  context.setDefaultNavigationTimeout(30000);
  await context.route("**/*", async (route) => {
    try {
      const request = route.request();
      if (["GET", "HEAD", "OPTIONS"].includes(request.method())) { await route.continue(); return; }
      const url = new URL(request.url());
      const loginPost = phase === "authentication" && request.method() === "POST" && url.origin === consoleOrigin
        && url.pathname === "/identity/realms/miy/login-actions/authenticate";
      if (loginPost) { await route.continue(); return; }
      result.blockedBrowserWrites++;
      try { const stats = statsByPage.get(request.frame().page()); if (stats?.active) stats.blockedWrites++; } catch {}
      await route.abort("blockedbyclient");
    } catch {
      if (phase !== "cleanup") result.requestGuardFailures++;
      // Never propagate a driver error that could contain a private request URL.
    }
  });
  await context.routeWebSocket("**/*", async (socket) => {
    result.blockedWebSockets++;
    // Never connect a business WebSocket whose messages might mutate state.
    try { await socket.close({ code: 1008, reason: "read_only_acceptance" }); }
    catch { if (phase !== "cleanup") result.requestGuardFailures++; }
  });

  stage = "surface_discovery";
  const portalOptions = await apiJson(portalOrigin, "/auth/options", "portal_options");
  const consoleOptions = await apiJson(consoleOrigin, "/auth/options", "console_options");
  requireThat(portalOptions.surface === "portal" && portalOptions.portalOrigin === portalOrigin
    && portalOptions.consoleOrigin === consoleOrigin && portalOptions.oauthUrl === portalOrigin + "/auth/login"
    && consoleOptions.surface === "console" && consoleOptions.portalOrigin === portalOrigin
    && consoleOptions.consoleOrigin === consoleOrigin && consoleOptions.oauthUrl === consoleOrigin + "/auth/login",
  "deployed_two_surface_contract_not_ready");
  result.portalChecks.push("deployed_two_surface_configuration");

  stage = "portal_oidc_login";
  phase = "authentication";
  let credentials = JSON.parse(await readFile(credentialsPath, "utf8"));
  requireThat(typeof credentials.developer === "string" && credentials.developer.length > 0, "developer_password_missing");
  const portalPage = await context.newPage();
  await portalPage.goto(portalOrigin + "/auth/login", { waitUntil: "load" });
  const loginUrl = new URL(portalPage.url());
  requireThat(loginUrl.origin === consoleOrigin && loginUrl.pathname.startsWith("/identity/realms/miy/"),
    "login_page_outside_trusted_console");
  await portalPage.locator("input[name=username]").fill("developer");
  await portalPage.locator("input[name=password]").fill(credentials.developer);
  credentials = null;
  await portalPage.locator("#kc-login").click();
  await waitShell(portalPage, portalOrigin);
  const portalMe = await apiJson(portalOrigin, "/api/v1/me", "portal_identity");
  requireThat(portalMe.browserPurpose === "portal" && portalMe.development === false && portalMe.admin === false,
    "ordinary_portal_identity_required");
  requireThat(await portalPage.locator(".runtime-controls").count() === 0, "portal_runtime_controls_exposed");
  result.portalChecks.push("ordinary_developer_portal_session_and_no_runtime_controls");

  stage = "console_oidc_sso";
  const consolePage = await context.newPage();
  const consoleStats = trackPage(consolePage, "console");
  await consolePage.goto(consoleOrigin + "/auth/login", { waitUntil: "load" });
  await waitShell(consolePage, consoleOrigin);
  const consoleMe = await apiJson(consoleOrigin, "/api/v1/me", "console_identity");
  requireThat(consoleMe.id === portalMe.id && consoleMe.browserPurpose === "console" && consoleMe.development === false,
    "console_session_identity_mismatch");
  phase = "read_only";
  await consolePage.getByRole("heading", { name: "내가 관리하는 앱", exact: true }).waitFor();
  await consolePage.waitForFunction(() => document.querySelectorAll(".ui-loading").length === 0, undefined, { timeout: 20000 });
  await consolePage.waitForTimeout(settleWindowMs);
  requireThat(await consolePage.locator(".miy-app-root, iframe").count() === 0, "console_app_root_or_iframe_present");
  requireThat(consoleStats.remoteScriptRequests === 0 && consoleStats.appContentRequests === 0,
    "console_requested_remote_app_content");
  requireNoUnexpectedErrors(consoleStats, "console_browser_error_observed");
  result.consoleChecks.push("console_has_no_remote_script_business_content_or_browser_errors");
  result.consoleMetrics = metrics(consoleStats);
  consoleStats.active = false;
  await consolePage.close({ runBeforeUnload: false });
  await portalPage.close({ runBeforeUnload: false });

  stage = "portal_management_read_denials";
  for (const [name, path] of [["managed_installations", "/api/v1/runtime/installations"], ["app_catalog", "/api/v1/apps?limit=1"]]) {
    const response = await context.request.get(portalOrigin + path, { timeout: 20000, maxRedirects: 0 });
    requireThat(response.status() === 403, "portal_management_read_not_403");
    result.portalChecks.push(name + "_403");
  }

  stage = "read_existing_dev_installations";
  const installations = await apiJson(portalOrigin, "/api/v1/installations", "visible_installations");
  requireThat(Array.isArray(installations), "invalid_installation_list");
  const dev = installations.filter((app) => app.environment === "dev");
  const running = dev.filter((app) => app.runtimeStatus === "running" && app.desiredState === "running");
  requireThat(running.every((app) => typeof app.appId === "string" && /^[a-z][a-z0-9-]{0,47}$/.test(app.appId)),
    "invalid_visible_app_identifier");
  const selected = running.sort((left, right) => left.appId.localeCompare(right.appId)).slice(0, options.maxApps);
  result.appCounts.visibleDev = dev.length;
  result.appCounts.runningDev = running.length;
  result.appCounts.selected = selected.length;
  if (selected.length === 0) {
    result.status = "NOT_RUN";
    result.reason = "no_existing_running_dev_apps_available_to_account";
  } else {
    stage = "existing_dev_app_read_only_navigation";
    for (let index = 0; index < selected.length; index++) await inspectApp(selected[index], index);
    result.status = result.appCounts.failed === 0 ? "PASS" : "FAIL";
  }
  requireThat(result.blockedBrowserWrites === 0 && result.blockedWebSockets === 0, "automatic_mutation_or_socket_prevented");
  requireThat(result.requestGuardFailures === 0, "browser_request_guard_failed");
} catch (error) {
  result.status = "FAIL";
  failureCode = safeCode(error, stage + "_failed");
  result.failure = { stage, code: failureCode };
} finally {
  phase = "cleanup";
  if (context) {
    for (const page of context.pages()) {
      const stats = statsByPage.get(page);
      if (stats) stats.active = false;
      try { await page.close({ runBeforeUnload: false }); } catch {}
    }
    for (const [purpose, origin] of [["console", consoleOrigin], ["portal", portalOrigin]]) {
      try { await logoutPurpose(purpose, origin); result.cleanup.push({ purpose, status: "PASS" }); }
      catch (error) { result.cleanup.push({ purpose, status: "FAIL", code: safeCode(error, "logout_check_failed") }); result.status = "FAIL"; }
    }
    try { await context.close(); } catch { result.cleanup.push({ purpose: "browser_context", status: "FAIL" }); result.status = "FAIL"; }
  }
  if (browser) {
    try { await browser.close(); } catch { result.cleanup.push({ purpose: "browser_process", status: "FAIL" }); result.status = "FAIL"; }
  }
  result.finishedAt = new Date().toISOString();
  if (resultFile) {
    try { await writeFile(resultFile, JSON.stringify(result, null, 2) + "\n", { mode: 0o600, flag: "wx" }); }
    catch { result.status = "FAIL"; failureCode = "sanitized_evidence_write_failed"; }
  }
  console.log(JSON.stringify({ status: result.status, appCounts: result.appCounts,
    cleanupFailures: result.cleanup.filter((entry) => entry.status === "FAIL").length,
    ...(failureCode ? { failureCode } : {}) }));
  process.exitCode = result.status === "PASS" ? 0 : result.status === "NOT_RUN" ? 3 : 1;
}
