import { test, expect, type Browser, type BrowserContext, type Frame, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { closeSync, copyFileSync, cpSync, mkdirSync, openSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
// @ts-expect-error plain ESM helpers shared with the CLI regression scripts
import { fixtureEnv, workflowArgs, workflowData } from "../scripts/lib/cli-workflow.mjs";
// @ts-expect-error plain ESM helpers shared with the CLI regression scripts
import { cliData } from "../scripts/lib/cli-result.mjs";

// Business app frames (docs/APP-FRAMES.md FV-01/02/04/05/06/07/08/10) on the
// disposable full stack (scripts/runtime-regression/stack-ui.sh): Keycloak login,
// CLI-deployed Docker apps, the dev portal and per-installation frame origins (P3:
// ports of one host). A real business app (examples/native-todo) and a probe app.
const origin = "http://localhost:18300";
const devPortal = "http://localhost:18307";
const root = resolve(__dirname, "..");
const state = join(root, ".local/stack-ui");
const evidence = join(root, ".local/evidence/app-frames");
const credentials = JSON.parse(readFileSync(join(state, "credentials.json"), "utf8")) as Record<string, string>;
const suffix = Date.now().toString(36);
// MIY_FRAME_APPS=todoId,probeId reuses apps an earlier run deployed: every
// installation takes a Docker network from the host's limited address pools.
const reuse = process.env.MIY_FRAME_APPS?.split(",");
const todoId = reuse?.[0] ?? "ui5-todo" + suffix;
const probeId = reuse?.[1] ?? "ui5-probe" + suffix;
const work = join(state, "work", "frames-" + suffix);
const adminDir = join(work, "admin");
const env = fixtureEnv(work, {
  PATH: `${process.env.HOME}/.cargo/bin:${process.env.PATH}`,
  CI: "true",
  CARGO_TARGET_DIR: join(work, "cargo"),
});
const binary = join(work, "miy");
let sequence = 0;
const observations: Record<string, unknown> = {};

function run(program: string, args: string[], cwd: string) {
  const log = openSync(join(work, `command-${++sequence}.log`), "w");
  try {
    return execFileSync(program, args, { cwd, env, encoding: "utf8", stdio: ["ignore", "pipe", log], maxBuffer: 16 << 20 });
  } finally {
    closeSync(log);
  }
}
const cli = (args: string[], cwd: string) => workflowData(cliData(run(binary, workflowArgs(args, cwd, env, binary), cwd)), args);

const PROBE = `import {useState} from 'react';import type {AppProps} from '@miy/sdk';
export default function AppRoot({runtime}:AppProps){const [out,setOut]=useState('');const [draft,setDraft]=useState('');
async function probes(){const r:Record<string,unknown>={};
try{void window.parent.document.title;r.parentDom='readable';}catch(e){r.parentDom=(e as Error).name;}
try{void (window.top as Window).location.href;r.topHref='readable';}catch(e){r.topHref=(e as Error).name;}
try{r.portalStorage=window.parent.localStorage.length;}catch(e){r.portalStorage=(e as Error).name;}
document.cookie='frame_probe_toss=1; path=/';
try{const res=await fetch('${devPortal}/api/v1/me',{credentials:'include'});r.portalMe=res.status;}catch(e){r.portalMe=(e as Error).name;}
try{const res=await fetch('/_miy/api/ping',{credentials:'omit'});r.noToken=res.status;}catch(e){r.noToken=(e as Error).name;}
for(const path of ['//evil.example/x','/../../../users']){window.parent.postMessage({protocol:'miy.frame/v1',type:'route.navigate',id:'p',payload:{path,replace:false}},'*');}
window.parent.postMessage({protocol:'other/v1',type:'route.navigate',id:'q',payload:{path:'/ignored',replace:false}},'*');
try{(window.top as Window).location.href='${origin}/';r.topNavigate='attempted';}catch(e){r.topNavigate=(e as Error).name;}
r.ping=(await runtime.api<{message:string}>('/ping')).message;
r.blobSize=(await runtime.apiBlob('/download')).size;
setOut(JSON.stringify(r));}
async function download(){const blob=await runtime.apiBlob('/download');const url=URL.createObjectURL(blob);const a=document.createElement('a');a.href=url;a.download='frame-probe.txt';document.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);}
return <section><h1 data-testid="probe">Frame probe</h1>
<input aria-label="probe draft" data-testid="draft" value={draft} onChange={e=>{setDraft(e.target.value);runtime.setNavigationState({dirty:!!e.target.value,message:'probe draft unsaved'});}}/>
<button onClick={probes}>run probes</button><button onClick={download}>download</button>
<button onClick={()=>alert('frame-modal')}>modal</button>
<a href="https://example.invalid/outside">outside link</a>
<a href={runtime.basePath+'/detail?x=1'}>inside link</a>
<output data-testid="location">{runtime.location.pathname+runtime.location.search}</output>
<output data-testid="result">{out}</output></section>;}`;

const PROBE_RUST = `use axum::{Json, Router, routing::get};
use serde_json::json;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    let app = Router::new()
        .route(
            "/ping",
            get(|| async { Json(json!({"message":"frame ready"})) }),
        )
        .route(
            "/download",
            get(|| async { ([("content-type", "text/plain")], "frame probe download") }),
        )
        .with_state(pool);
    miy_app_sdk::serve(app).await
}
`;

function scaffold(id: string, name: string) {
  const dir = join(work, id);
  cli(["init", dir, "--id", id, "--name", name], work);
  cli(["auth", "login", "--account", "developer", "--url", origin], dir);
  cli(["component", "sync"], dir);
  const registration = cli(["register"], dir) as { id: string };
  cli(["registration", "approve", registration.id], adminDir);
  run("git", ["init", "-q"], dir);
  return dir;
}
function deploy(dir: string) {
  run("git", ["add", "."], dir);
  run("git", ["-c", "user.name=frame probe", "-c", "user.email=probe@localhost", "commit", "-qm", "frame PoC"], dir);
  cli(["build"], dir);
  cli(["deploy"], dir);
}

test.describe.configure({ mode: "serial" });

test.beforeAll(async () => {
  test.setTimeout(1800000);
  mkdirSync(adminDir, { recursive: true });
  mkdirSync(evidence, { recursive: true });
  if (reuse) return;
  copyFileSync(join(root, "target/debug/miy"), binary);
  cli(["auth", "login", "--account", "admin", "--url", origin], adminDir);
  cli(["component", "publish", join(root, "registry/components/ui/1.0.1")], adminDir);
  // The real business app: examples/native-todo business files on a fresh scaffold.
  const todo = scaffold(todoId, "할 일 실증 " + suffix);
  for (const part of ["src", "web/src", "migrations/0002_todos.sql"])
    cpSync(join(root, "examples/native-todo", part), join(todo, part), { recursive: true });
  deploy(todo);
  const probe = scaffold(probeId, "프레임 프로브 " + suffix);
  writeFileSync(join(probe, "web/src/AppRoot.tsx"), PROBE);
  writeFileSync(join(probe, "src/main.rs"), PROBE_RUST);
  deploy(probe);
});

test.afterAll(() => {
  writeFileSync(join(evidence, `observations-${test.info().project.name}.json`), JSON.stringify(observations, null, 2));
});

async function signIn(browser: Browser, user: string): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => console.log(`[${user}] pageerror: ${e.message}`));
  await page.goto(devPortal + "/");
  await page.getByRole("link", { name: /^(OAuth로 로그인|로그인)$/ }).click();
  await page.locator("#username").waitFor({ state: "visible" });
  await page.locator("#username").fill(user);
  await page.locator("#password").fill(credentials[user]!);
  await page.locator("#kc-login").click();
  await page.waitForURL(devPortal + "/");
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
  return { context, page };
}
const appPath = (id: string) => `${devPortal}/environments/dev/apps/${id}`;
async function appFrame(page: Page): Promise<Frame> {
  const element = page.locator("iframe.app-frame");
  await expect(element).toBeVisible();
  const frame = await (await element.elementHandle())!.contentFrame();
  await expect.poll(() => frame?.url() ?? "").toMatch(/^http:\/\/localhost:183(2\d)\/_miy\/launch$/);
  return frame!;
}

test("FV-01/05/08: a real app runs in its own frame origin with routing, reload and persisted data", async ({ browser }) => {
  const { context, page } = await signIn(browser, "developer");
  const historyBefore = await page.evaluate(() => history.length);
  const started = Date.now();
  await page.goto(appPath(todoId) + "/?view=active");
  const frame = await appFrame(page);
  await expect(frame.getByRole("heading", { name: "오늘의 할 일" })).toBeVisible();
  observations.firstOpenMs = Date.now() - started;
  const frameOrigin = new URL(frame.url()).origin;
  expect(frameOrigin).not.toBe(devPortal);
  observations.todoFrameOrigin = frameOrigin;
  // Launch adds no history entry; the direct link reached the app location.
  expect(await page.evaluate(() => history.length)).toBe(historyBefore + 1);
  await expect(frame.getByRole("button", { name: /^할 일/ })).toHaveAttribute("aria-current", "page");
  // Create through the frame API (frame token, frame Origin), then filter by app navigation.
  const title = "프레임 실증 " + suffix;
  await frame.locator("#todo-title").fill(title);
  await frame.getByRole("button", { name: "+ 추가하기" }).click();
  await expect(frame.getByText(title)).toBeVisible();
  // The app reports loading as unsaved work; leave only once it settled, or the
  // portal rightly asks first.
  const settled = () => expect(frame.getByRole("button", { name: "새로고침" })).toBeEnabled();
  await settled();
  await frame.getByRole("button", { name: /^전체/ }).click();
  // The portal router drops a trailing slash, as it always did for app routes.
  await expect(page).toHaveURL(new RegExp(`${appPath(todoId)}/?$`));
  await expect(frame.getByRole("button", { name: /^전체/ })).toHaveAttribute("aria-current", "page");
  await settled();
  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`${appPath(todoId)}/?\\?view=active$`));
  await expect(frame.getByRole("button", { name: /^할 일/ })).toHaveAttribute("aria-current", "page");
  // The frame reloading itself resends its consumed code: it relaunches cleanly.
  // The relaunch replaces the iframe element, so content is located afresh.
  await settled();
  const content = page.frameLocator("iframe.app-frame");
  await frame.evaluate(() => location.reload());
  await expect(content.getByText(title)).toBeVisible();
  // A full portal reload opens a new frame on the same origin with the same data.
  const again = Date.now();
  await page.reload();
  await expect(content.getByText(title)).toBeVisible();
  const reloaded = await appFrame(page);
  observations.reopenMs = Date.now() - again;
  expect(new URL(reloaded.url()).origin).toBe(frameOrigin);
  // Assets come from this frame's capability path with authorized revalidation.
  const assets = await reloaded.evaluate(() =>
    performance.getEntriesByType("resource").map((entry) => new URL(entry.name).pathname).filter((path) => path.startsWith("/_miy/a/")),
  );
  expect(assets.length).toBeGreaterThan(0);
  await context.close();
});

test("FV-02: unsaved input in the frame asks before the portal leaves the app", async ({ browser }) => {
  const { context, page } = await signIn(browser, "developer");
  await page.goto(appPath(probeId));
  const frame = await appFrame(page);
  await expect(frame.getByTestId("probe")).toBeVisible();
  await frame.getByTestId("draft").fill("unsaved");
  const dialogs: string[] = [];
  page.on("dialog", (dialog) => {
    dialogs.push(dialog.message());
    void dialog.dismiss();
  });
  await page.locator("#platform-navigation a").first().click();
  await expect.poll(() => dialogs.length).toBeGreaterThan(0);
  expect(page.url()).toContain(probeId);
  await frame.getByTestId("draft").fill("");
  dialogs.length = 0;
  page.removeAllListeners("dialog");
  await page.locator("#platform-navigation a").first().click();
  await expect(page).not.toHaveURL(new RegExp(probeId));
  observations.inputProtection = "dirty asked, clean left";
  await context.close();
});

test("FV-04/06/07/10: the frame cannot reach the portal, abuse the bridge or use portal cookies; UX basics work", async ({ browser }) => {
  const { context, page } = await signIn(browser, "developer");
  await page.goto(appPath(probeId));
  const frame = await appFrame(page);
  const before = page.url();
  await frame.getByRole("button", { name: "run probes" }).click();
  await expect(frame.getByTestId("result")).not.toHaveText("");
  const result = JSON.parse((await frame.getByTestId("result").textContent()) || "{}");
  observations.probes = result;
  expect(result.parentDom).toBe("SecurityError");
  expect(result.portalStorage).toBe("SecurityError");
  expect(result.noToken).toBe(401);
  expect(result.ping).toBe("frame ready");
  expect(result.blobSize).toBe("frame probe download".length);
  // Cross-origin cookie requests to the portal are refused or unreadable.
  expect(result.portalMe === 403 || result.portalMe === "TypeError").toBe(true);
  // Forged and out-of-app navigation messages and top navigation leave the portal alone.
  await page.waitForTimeout(500);
  expect(page.url()).toBe(before);
  // In-app link goes through the portal router; outside links open a new tab.
  await frame.getByRole("link", { name: "inside link" }).click();
  await expect(page).toHaveURL(appPath(probeId) + "/detail?x=1");
  await expect(frame.getByTestId("location")).toHaveText("/detail?x=1");
  // The external host does not resolve here; the new tab's request proves the target.
  const popup = page.waitForEvent("popup");
  const outside = context.waitForEvent("request", (request) => request.url().includes("example.invalid"));
  await frame.getByRole("link", { name: "outside link" }).click();
  await popup;
  expect((await outside).url()).toBe("https://example.invalid/outside");
  expect(page.url()).toBe(appPath(probeId) + "/detail?x=1");
  // Downloads through apiBlob and app dialogs stay usable inside the sandbox.
  const download = page.waitForEvent("download");
  await frame.getByRole("button", { name: "download" }).click();
  expect((await download).suggestedFilename()).toBe("frame-probe.txt");
  // An alert blocks the click until dismissed, so handle it before clicking.
  let modal = "";
  page.once("dialog", (dialog) => {
    modal = dialog.message();
    void dialog.dismiss();
  });
  await frame.getByRole("button", { name: "modal" }).click();
  expect(modal).toBe("frame-modal");
  // FV-06 (record, not a fix): a cookie written by the frame on another port.
  observations.portalSeesFrameCookie = (await context.cookies(devPortal)).some((c) => c.name === "frame_probe_toss");
  // FV-10: the portal cookie session no longer reaches app APIs.
  const installation = (await (await context.request.get(devPortal + "/api/v1/installations")).json()).find(
    (row: { appId: string }) => row.appId === probeId,
  );
  expect((await context.request.get(`${devPortal}/api/v1/installations/${installation.id}/api/ping`)).status()).toBe(403);
  await context.close();
});
