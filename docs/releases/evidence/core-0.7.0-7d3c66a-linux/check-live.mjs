// Read-only live checks for the 7d3c66a rollout, through the shared development login.
//   node check-live.mjs visibility OUT.json   what each development account can see and do (API)
//   node check-live.mjs ui OUT_DIR            shell, admin sections, mobile drawer, public CLI page
// Only GET requests besides login/logout. No setting, group, permission or admission is changed:
// the admin app-access panel is only opened (its admission selects and buttons are not touched).
import { writeFileSync, mkdirSync } from "node:fs";
import { chromium } from "/projects/miy-v2/node_modules/@playwright/test/index.mjs";

const origin = "https://v2.dwdcc.kr";
const accounts = ["admin", "developer", "alice", "bob"];
const [mode, out] = process.argv.slice(2);

async function login(context, account) {
  const response = await context.request.post(origin + "/auth/development", { headers: { origin }, data: { account } });
  if (response.status() !== 200) throw new Error(`development login ${account}: ${response.status()}`);
}
async function logout(context) {
  const me = await context.request.get(origin + "/api/v1/me");
  if (me.status() !== 200) return me.status();
  const { csrf } = await me.json();
  return (await context.request.post(origin + "/auth/logout", { headers: { origin, "x-miy-csrf": csrf } })).status();
}

const browser = await chromium.launch();
if (mode === "visibility") {
  const result = {};
  for (const account of accounts) {
    const context = await browser.newContext();
    await login(context, account);
    const me = await (await context.request.get(origin + "/api/v1/me")).json();
    const installations = await (await context.request.get(origin + "/api/v1/installations")).json();
    const apps = await context.request.get(origin + "/api/v1/apps?limit=200");
    result[account] = {
      admin: me.admin,
      installations: installations.map((i) => `${i.appId}:${i.environment}:${i.canPromote ? "promote" : "-"}`).sort(),
      catalog: apps.status() === 200 ? (await apps.json().then((b) => (Array.isArray(b) ? b : b.apps))).map((a) => a.id).sort() : apps.status(),
      logout: await logout(context),
    };
    await context.close();
  }
  writeFileSync(out, JSON.stringify(result, null, 1) + "\n");
  console.log(JSON.stringify(Object.fromEntries(Object.entries(result).map(([k, v]) => [k, { installations: v.installations.length, logout: v.logout }]))));
} else {
  mkdirSync(out, { recursive: true });
  const errors = [];
  const result = {};
  for (const [name, viewport] of [["desktop", { width: 1440, height: 900 }], ["mobile", { width: 390, height: 844 }]]) {
    const context = await browser.newContext({ viewport });
    const page = await context.newPage();
    page.on("pageerror", (e) => errors.push(`${name}: ${e.message}`));
    page.on("response", (r) => {
      const path = new URL(r.url()).pathname;
      if (r.status() >= 500 || (r.status() >= 400 && path.startsWith("/api/") && path !== "/api/v1/me")) errors.push(`${name}: HTTP ${r.status()} ${path}`);
    });
    await page.goto(origin + "/");
    await page.getByRole("heading", { name: "로그인" }).waitFor();
    await page.getByRole("button", { name: /관리자로 시작/ }).click();
    await page.getByRole("button", { name: "로그아웃" }).waitFor();
    await page.waitForTimeout(2000);
    result[name + "RailApps"] = await page.locator('#platform-navigation nav[aria-label="앱"] a.rail-item').count();
    result[name + "Overflow"] = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
    await page.screenshot({ path: `${out}/${name}-home.png`, fullPage: true });
    if (name === "desktop") {
      const running = page.locator(".app-card").filter({ has: page.getByRole("link", { name: "앱 열기 →" }) }).first();
      if (await running.count()) {
        await running.getByRole("link", { name: "앱 열기 →" }).click();
        await page.waitForTimeout(5000);
        result.appRendered = (await page.locator(".miy-app-root").count()) > 0 && (await page.locator(".miy-app-root").innerText()).trim().length > 0;
        result.appBoundary = await page.getByRole("heading", { name: "앱을 열 수 없습니다" }).count();
        await page.screenshot({ path: `${out}/app.png`, fullPage: true });
      }
      await page.goto(origin + "/admin");
      await page.getByRole("heading", { name: "플랫폼 관리" }).waitFor();
      const panel = (title) => page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: title, exact: true }) });
      await panel("그룹").waitFor();
      result.groupsPanel = (await panel("그룹").innerText()).includes("그룹이 없습니다.") ? "empty" : "listed";
      const access = panel("앱 접근");
      const options = await access.locator("select").first().locator("option").evaluateAll((o) => o.map((x) => x.value).filter(Boolean));
      result.accessApps = options.length;
      if (options.length) {
        await access.locator("select").first().selectOption(options[0]); // loads GET /apps/{app}/access only
        await access.getByText("회사 사용 상태").waitFor();
        result.accessState = (await access.getByText("사용 중", { exact: true }).count()) ? "enabled" : "disabled";
        result.accessAdmission = await access.locator('select[aria-label$="입장 대상"]').evaluateAll((s) => s.map((x) => x.value));
      }
      const audit = panel("감사 기록");
      await audit.locator("table, .ui-empty-inline").first().waitFor();
      result.auditRows = await audit.locator("tbody tr").count();
      await page.screenshot({ path: `${out}/admin.png`, fullPage: true });
    } else {
      await page.getByRole("button", { name: "메뉴 열기" }).click();
      await page.keyboard.press("Escape");
      result.drawerFocusReturned = await page.evaluate(() => document.activeElement?.classList.contains("navigation-toggle"));
    }
    result[name + "Logout"] = await logout(context);
    await context.close();
  }
  const publicContext = await browser.newContext();
  const publicPage = await publicContext.newPage();
  await publicPage.goto(origin + "/cli/install");
  await publicPage.getByRole("heading", { name: "CLI 설치", exact: true }).waitFor();
  result.publicInstall = true;
  await publicContext.close();
  writeFileSync(`${out}/result.json`, JSON.stringify({ ...result, errors }, null, 1) + "\n");
  console.log(JSON.stringify({ ...result, errors }));
}
await browser.close();
