import { test, expect, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { closeSync, copyFileSync, mkdirSync, openSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { openNewTab } from "./new-tab";
// @ts-expect-error plain ESM helpers shared with the CLI regression scripts
import { fixtureEnv, workflowArgs, workflowData } from "../scripts/lib/cli-workflow.mjs";
// @ts-expect-error plain ESM helpers shared with the CLI regression scripts
import { cliData } from "../scripts/lib/cli-result.mjs";

// UI-5 on the disposable full stack (scripts/runtime-regression/stack-ui.sh):
// Keycloak login, a CLI-deployed Docker app with real health, controls and release
// change, and permission revocation while the app is open.
const origin = "http://localhost:18300";
const root = resolve(__dirname, "..");
const state = join(root, ".local/stack-ui");
const credentials = JSON.parse(readFileSync(join(state, "credentials.json"), "utf8")) as Record<string, string>;
const appId = "ui5-" + Date.now().toString(36);
const appName = `UI5 검증 ${appId.slice(4)}`;
const work = join(state, "work", appId);
const appDir = join(work, "app");
const adminDir = join(work, "admin");
const alice = "00000000-0000-4000-8000-000000000003";
const env = fixtureEnv(work, {
  PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
  CI: "true",
  CARGO_TARGET_DIR: join(work, "cargo"),
});
const binary = join(work, "miy");
const checks: string[] = [];
let sequence = 0;

function run(program: string, args: string[], cwd: string) {
  const log = openSync(join(work, `command-${++sequence}.log`), "w");
  try {
    return execFileSync(program, args, { cwd, env, encoding: "utf8", stdio: ["ignore", "pipe", log], maxBuffer: 16 << 20 });
  } finally {
    closeSync(log);
  }
}
const cli = (args: string[], cwd = appDir) => workflowData(cliData(run(binary, workflowArgs(args, cwd, env, binary), cwd)), args);
const container = () => `miy-app-${installation().id.replaceAll("-", "")}`;
const running = () => {
  try {
    return execFileSync("docker", ["inspect", "-f", "{{.State.Running}}", container()], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return "absent";
  }
};
let installationRow: { id: string; releaseId: string } | undefined;
const installation = () => installationRow!;

function writeApp(version: string) {
  writeFileSync(
    join(appDir, "web/src/AppRoot.tsx"),
    `import {useState,useEffect} from 'react';import type {AppProps} from '@miy/sdk';
export default function AppRoot({runtime}:AppProps){const [draft,setDraft]=useState('');
const [boundary,setBoundary]=useState('');
async function checkBoundary(){const me=await (await fetch('/api/v1/me')).json();
const changed=await fetch('/api/v1/apps',{method:'POST',headers:{'Content-Type':'application/json','x-miy-csrf':me.csrf},body:JSON.stringify({id:'boundary-${appId}',name:'경계 확인 앱'})});
let consoleReadable=false;try{await fetch(me.consoleOrigin+'/api/v1/me',{credentials:'include'});consoleReadable=true;}catch{}
setBoundary(JSON.stringify({managementStatus:changed.status,consoleReadable}));}
useEffect(()=>runtime.registerNavigationGuard(()=>draft?'저장하지 않은 입력이 있습니다':undefined),[runtime.registerNavigationGuard,draft]);
return <section><h1 data-testid="probe-version">UI5 probe ${version}</h1><input aria-label="메모" data-testid="draft" value={draft} onChange={e=>setDraft(e.target.value)}/><button onClick={checkBoundary}>경계 확인</button><output data-testid="boundary-result">{boundary}</output></section>;}`,
  );
  writeFileSync(join(appDir, "src/main.rs"), `use axum::{Json, Router, response::Html, routing::get};
use serde_json::json;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    let app = Router::new()
        .route("/ping", get(|| async { Json(json!({"message":"Ready to build"})) }))
        .route("/boundary-html", get(|| async { Html("<!doctype html><h1>Boundary HTML probe</h1><script>window.__boundaryHtmlExecuted=true</script>") }))
        .with_state(pool);
    miy_app_sdk::serve(app).await
}
`);
  const manifest = JSON.parse(readFileSync(join(appDir, "manifest.json"), "utf8"));
  writeFileSync(join(appDir, "manifest.json"), JSON.stringify({ ...manifest, version }, null, 2) + "\n");
  run("git", ["add", "."], appDir);
  run("git", ["-c", "user.name=UI5 probe", "-c", "user.email=probe@localhost", "commit", "-qm", `UI5 probe ${version}`], appDir);
  cli(["build"]);
  cli(["deploy"]);
  installationRow = (cli(["status"]) as { environment: string; id: string; releaseId: string }[]).find((v) => v.environment === "dev");
}

async function surfaceOrigin(context: BrowserContext, surface: "portal" | "console") {
  const options = await (await context.request.get(origin + "/auth/options")).json();
  return (surface === "console" ? options.consoleOrigin : options.portalOrigin) as string;
}
async function signInPage(context: BrowserContext, user: string, surface: "portal" | "console") {
  const selectedOrigin = await surfaceOrigin(context, surface);
  const page = await context.newPage();
  page.on("pageerror", (e) => console.log(`[${user}] pageerror: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && console.log(`[${user}] console: ${m.text()}`));
  await page.goto(selectedOrigin + "/");
  await page.getByRole("link", { name: /^(OAuth로 로그인|로그인)$/ }).click();
  await Promise.race([
    page.locator("#username").waitFor({ state: "visible" }),
    page.getByRole("button", { name: "로그아웃" }).waitFor({ state: "visible" }),
  ]);
  if (await page.locator("#username").isVisible()) {
    await page.locator("#username").fill(user);
    await page.locator("#password").fill(credentials[user]!);
    await page.locator("#kc-login").click();
  }
  await page.waitForURL(selectedOrigin + "/");
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
  return page;
}
async function signIn(browser: Browser, user: string, surface: "portal" | "console" = "portal"): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext();
  return { context, page: await signInPage(context, user, surface) };
}
async function api(context: BrowserContext, path: string, method = "GET", data?: unknown, surface: "portal" | "console" = "console") {
  const selectedOrigin = await surfaceOrigin(context, surface);
  const csrf = (await (await context.request.get(selectedOrigin + "/api/v1/me")).json()).csrf;
  return context.request.fetch(selectedOrigin + "/api/v1" + path, { method, data, headers: { origin: selectedOrigin, "x-miy-csrf": csrf } });
}
const appCard = (page: Page) => page.locator(".app-card").filter({ hasText: appName });

test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  test.setTimeout(1200000);
  mkdirSync(adminDir, { recursive: true });
  copyFileSync(join(root, "target/debug/miy"), binary);
  cli(["auth", "login", "--account", "admin", "--url", origin], adminDir);
  cli(["component", "publish", join(root, "registry/components/ui/1.0.1")], adminDir);
  cli(["init", appDir, "--id", appId, "--name", appName], work);
  cli(["auth", "login", "--account", "developer", "--url", origin]);
  cli(["component", "sync"]);
  const registration = cli(["register"]) as { id: string };
  cli(["registration", "approve", registration.id], adminDir);
  run("git", ["init", "-q"], appDir);
  writeApp("0.1.0");
  expect(installationRow?.id).toBeTruthy();
  expect(running()).toBe("true");
  checks.push("cli-build-deploy");
});

test("Keycloak login and logout through the new shell", async ({ browser }) => {
  const { context, page } = await signIn(browser, "admin");
  await expect(page.locator("header")).toContainText("admin");
  await expect(page.locator("#platform-navigation").getByRole("link", { name: appName })).toBeVisible();
  await page.getByRole("button", { name: "로그아웃" }).click();
  await page.waitForURL((url) => url.pathname.includes("/protocol/openid-connect/logout"));
  await expect(page.getByText("Invalid redirect uri", { exact: false })).toHaveCount(0);
  if (await page.locator("#kc-logout").count()) await page.locator("#kc-logout").click();
  await expect(page.getByRole("heading", { name: "로그인" })).toBeVisible();
  expect((await context.request.get(origin + "/api/v1/me")).status()).toBe(401);
  await context.close();
  checks.push("keycloak-login-logout");
});

test("deployed Docker app opens in the portal and runtime controls stay on the console", async ({ browser }) => {
  const { context, page } = await signIn(browser, "admin");
  const manifests: number[] = [];
  context.on("response", (r) => r.url().endsWith("mf-manifest.json") && manifests.push(r.status()));
  const card = appCard(page);
  await expect(card.getByRole("button", { name: "중지" })).toHaveCount(0);
  const appPage = await openNewTab(page, card.getByRole("link", { name: "앱 열기", exact: true }));
  await expect(appPage).toHaveURL(new RegExp(`/environments/dev/apps/${appId}$`));
  await expect(appPage.locator(`.miy-app-root[data-miy-app="${appId}"]`).getByTestId("probe-version")).toHaveText("UI5 probe 0.1.0");
  await expect(appPage.locator("#platform-navigation").getByRole("link", { name: appName })).toHaveAttribute("aria-current", "page");
  await expect(appPage.getByText(/컨테이너:|마지막 서비스 검사:/)).toHaveCount(0);
  await expect(card).toBeVisible();
  expect(manifests).toContain(200);
  // V1-O2: the deployment capped this installation's roles and the app tagged its connections.
  const sql = (query: string) =>
    execFileSync("docker", ["exec", "miy-stack-ui-postgres-1", "psql", "-X", "-At", "-U", "postgres", "-d", "miy_core", "-c", query], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  expect(
    sql(
      `SELECT string_agg(r.rolconnlimit::text, ',' ORDER BY r.rolname = d.runtime_role DESC) FROM db_resources d ` +
        `JOIN pg_roles r ON r.rolname IN (d.runtime_role, d.migration_role) WHERE d.app_id = '${appId}' AND d.environment = 'dev'`,
    ),
  ).toBe("6,2");
  await expect
    .poll(() => Number(sql(`SELECT count(*) FROM pg_stat_activity WHERE application_name = 'miy-app-${appId}-dev'`)), { timeout: 20000 })
    .toBeGreaterThan(0);
  expect(Number(sql(`SELECT count(*) FROM pg_stat_activity WHERE application_name IN ('miy-core-serve', 'miy-core-worker')`))).toBeGreaterThan(0);

  const management = await signIn(browser, "admin", "console");
  const managedCard = appCard(management.page);
  await expect(managedCard.getByRole("status", { name: "앱 실행 및 서비스 상태" })).toContainText("서비스: 정상", { timeout: 60000 });
  await managedCard.getByRole("button", { name: "중지" }).click();
  await expect(managedCard.locator(".app-card-meta")).toContainText("stopped", { timeout: 120000 });
  expect(running()).toBe("false");
  await appPage.reload();
  await expect(appPage.getByRole("heading", { name: "앱이 실행 중이 아닙니다" })).toBeVisible();
  await managedCard.getByRole("button", { name: "시작" }).click();
  await expect(managedCard.getByRole("status", { name: "앱 실행 및 서비스 상태" })).toContainText("컨테이너: running · 서비스: 정상", {
    timeout: 120000,
  });
  expect(running()).toBe("true");
  await page.screenshot({ path: join(root, ".local/evidence/stack-ui/home.png"), fullPage: true });
  await management.context.close();
  await context.close();
  checks.push("remote-ui-from-core", "health", "db-connection-budget", "stop-start");
});

test("unsaved input blocks shell navigation, and a new release asks for a reload", async ({ browser }) => {
  const { context, page } = await signIn(browser, "admin");
  const dialogs: string[] = [];
  let answer: "accept" | "dismiss" = "dismiss";
  page.on("dialog", async (d) => {
    dialogs.push(d.type());
    await d[answer]();
  });
  await page.goto(`/environments/dev/apps/${appId}`);
  await page.getByTestId("draft").fill("작성 중");
  await page.locator("#platform-navigation").getByRole("link", { name: "홈", exact: true }).click();
  await expect.poll(() => dialogs.length).toBe(1);
  await expect(page).toHaveURL(new RegExp(`/apps/${appId}$`));
  await expect(page.getByTestId("draft")).toHaveValue("작성 중");

  writeApp("0.2.0");
  await expect(page.locator(".app-session-bar")).toContainText("앱 상태가 변경되었습니다", { timeout: 30000 });
  answer = "accept";
  await page.getByRole("button", { name: "새로고침" }).click();
  await expect(page.getByTestId("probe-version")).toHaveText("UI5 probe 0.2.0", { timeout: 60000 });
  await expect(page.getByText(/컨테이너:|마지막 서비스 검사:/)).toHaveCount(0);
  await page.screenshot({ path: join(root, ".local/evidence/stack-ui/app.png"), fullPage: true });
  await context.close();
  checks.push("navigation-guard", "release-change");
});

test("a real portal module cannot use console privileges or read the IdP, and API HTML cannot execute", async ({ browser }) => {
  const { context, page } = await signIn(browser, "admin");
  try {
    // Keep both purpose cookies in the same browser context. A CORS failure with
    // no authenticated console session would not establish this boundary.
    const consolePage = await signInPage(context, "admin", "console");
    const consoleOrigin = await surfaceOrigin(context, "console");
    const consoleMe = await context.request.get(consoleOrigin + "/api/v1/me");
    expect(consoleMe.status()).toBe(200);
    expect((await consoleMe.json()).browserPurpose).toBe("console");
    await page.goto(`/environments/dev/apps/${appId}`);
    await expect(page.getByTestId("probe-version")).toHaveText("UI5 probe 0.2.0");
    await expect(page.locator("iframe")).toHaveCount(0);
    // This button and the fetch calls are part of the built, deployed MF module.
    await page.getByRole("button", { name: "경계 확인", exact: true }).click();
    await expect(page.getByTestId("boundary-result")).toHaveText(JSON.stringify({ managementStatus: 403, consoleReadable: false }));

    const identityUrl = consoleOrigin + "/identity/realms/miy/protocol/openid-connect/auth";
    const identity = await context.request.get(identityUrl);
    expect(identity.headers()["content-security-policy"]).toContain("frame-ancestors 'none'");
    expect(identity.headers()["x-frame-options"]?.toUpperCase()).toBe("DENY");
    const frame = await page.evaluate(async (url) => {
      // An attack probe, never the application execution mechanism.
      return new Promise<{ loaded: boolean; readable: boolean }>((resolve) => {
        const element = document.createElement("iframe");
        const timer = setTimeout(() => { element.remove(); resolve({ loaded: false, readable: false }); }, 10000);
        element.onload = () => {
          let readable = false;
          try { readable = !!element.contentWindow?.document.documentElement; } catch { /* cross-origin access denied */ }
          clearTimeout(timer);
          element.remove();
          resolve({ loaded: true, readable });
        };
        element.src = url;
        document.body.append(element);
      });
    }, identityUrl);
    expect(frame).toEqual({ loaded: true, readable: false });
    await expect(page.locator("iframe")).toHaveCount(0);

    const apiPath = `/api/v1/installations/${installation().id}/api/boundary-html`;
    const html = await context.request.get(origin + apiPath);
    expect(html.status()).toBe(200);
    expect(html.headers()["content-type"]).toContain("text/html");
    expect(html.headers()["content-security-policy"]).toContain("sandbox");
    expect(html.headers()["x-content-type-options"]).toBe("nosniff");
    const denied = await context.request.get(consoleOrigin + apiPath);
    expect([403, 404]).toContain(denied.status());
    const htmlPage = await context.newPage();
    await htmlPage.goto(origin + apiPath);
    await expect(htmlPage.getByRole("heading", { name: "Boundary HTML probe", exact: true })).toBeVisible();
    expect(await htmlPage.evaluate(() => (window as typeof window & { __boundaryHtmlExecuted?: boolean }).__boundaryHtmlExecuted)).toBeUndefined();
    await htmlPage.close();
    await consolePage.close();
    checks.push("real-mf-portal-management-denied", "authenticated-console-cross-origin-read-denied", "idp-dom-and-framing-denied", "api-html-sandbox-nosniff");
  } finally { await context.close(); }
});

test("revoking a user's permission while the app is open", async ({ browser }) => {
  const admin = await signIn(browser, "admin", "console");
  const grant = (enabled: boolean) =>
    api(admin.context, "/grants", "PUT", { userId: alice, appId, environment: "dev", permission: "use", enabled });
  expect((await grant(true)).ok()).toBeTruthy();
  const user = await signIn(browser, "alice");
  const rail = user.page.locator("#platform-navigation");
  const appPage = await openNewTab(user.page, rail.getByRole("link", { name: appName }));
  await expect(appPage.getByTestId("probe-version")).toHaveText("UI5 probe 0.2.0");
  await expect(appCard(appPage)).toHaveCount(0);
  await expect(appCard(user.page)).toBeVisible();

  expect((await grant(false)).ok()).toBeTruthy();
  await expect(appPage.locator(".app-session-bar")).toContainText("앱 상태가 변경되었습니다", { timeout: 30000 });
  await appPage.locator("#platform-navigation").getByRole("link", { name: "홈", exact: true }).click();
  await expect(appPage.locator("#platform-navigation").getByRole("link", { name: appName })).toHaveCount(0);
  await expect(rail.getByRole("link", { name: appName })).toHaveCount(0);
  await appPage.goto(`/environments/dev/apps/${appId}`);
  await expect(appPage.getByRole("heading", { name: "앱에 접근할 수 없습니다" })).toBeVisible();
  const visible = (await (await api(user.context, "/installations", "GET", undefined, "portal")).json()) as { appId: string }[];
  expect(visible.map((a) => a.appId)).not.toContain(appId);
  await user.context.close();
  await admin.context.close();
  checks.push("grant-revocation-in-use");
});

test("removing the app goes through the confirm dialog and removes the container", async ({ browser }) => {
  const { context, page } = await signIn(browser, "admin", "console");
  const card = appCard(page);
  const trigger = card.getByRole("button", { name: "앱 제거(DB 보존)" });
  await trigger.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText(appId);
  await expect(dialog.getByRole("button", { name: "취소" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(trigger).toBeFocused();
  expect(running()).toBe("true");
  await trigger.click();
  await dialog.getByRole("button", { name: "앱 제거(DB 보존)" }).click();
  await expect(card).toContainText("앱 실행 환경이 제거됐습니다", { timeout: 120000 });
  expect(running()).toBe("absent");
  await context.close();
  checks.push("remove-confirm-dialog");
});

test.afterAll(() => {
  mkdirSync(join(root, ".local/evidence/stack-ui"), { recursive: true });
  writeFileSync(
    join(root, ".local/evidence/stack-ui/result.json"),
    JSON.stringify({ appId, installation: installationRow, checks }, null, 2),
  );
});
