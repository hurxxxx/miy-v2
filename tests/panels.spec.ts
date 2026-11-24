import { readFileSync } from "node:fs";
import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page, type Route } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";
import { mockFrames, type FrameApi } from "./frame-mock";

// M5 in the portal: side panels (a secondary app frame with its own location)
// and the home's summary cards. Mocked API and frames: presentation only. The
// Core regression covers the manifest contract, pins and the card calls.
const planner: Installation = {
  id: "40000000-0000-4000-8000-000000000071",
  appId: "planner",
  name: "플래너",
  releaseId: "50000000-0000-4000-8000-000000000071",
  environment: "prod",
  version: "1.0.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  canManage: false,
  panels: [
    { id: "todos", title: "할 일", path: "/widgets/todos" },
    { id: "memos", title: "메모", path: "/widgets/memos" },
  ],
  ui: { entry: "/artifacts/health-fixture/mf-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
} as Installation;
const ledger: Installation = {
  ...planner,
  id: "40000000-0000-4000-8000-000000000072",
  appId: "ledger",
  name: "장부",
  releaseId: "50000000-0000-4000-8000-000000000072",
  panels: [],
} as Installation;

async function open(page: Page, options: {
  cards?: unknown;
  frameApi?: FrameApi;
  installations?: () => Installation[];
  pins?: string[];
} = {}) {
  await mockSurface(page, "portal");
  let preferences = { ...defaultPreferences, pinnedPanels: options.pins ?? [] };
  const patches: unknown[] = [];
  const launches: URLSearchParams[] = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/_miy/launch") launches.push(new URLSearchParams(request.postData() ?? ""));
  });
  await page.route("**/api/**", async (route: Route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === "/api/v1/me")
      return route.fulfill({ json: browserUser(route, { id: "u", name: "패널 사용자", admin: false, csrf: "p-csrf", preferences }) });
    if (path === "/api/v1/me/preferences" && request.method() === "PATCH") {
      const body = request.postDataJSON() as { panelPin: { id: string; pinned: boolean } };
      patches.push({ body, csrf: request.headers()["x-miy-csrf"] ?? null });
      const pins = preferences.pinnedPanels.filter((id) => id !== body.panelPin.id);
      if (body.panelPin.pinned) pins.push(body.panelPin.id);
      preferences = { ...preferences, pinnedPanels: pins };
      return route.fulfill({ json: { name: "패널 사용자", preferences } });
    }
    if (path === "/api/v1/installations") return route.fulfill({ json: options.installations?.() ?? [planner, ledger] });
    if (path === "/api/v1/home/cards") return route.fulfill({ json: options.cards ?? { cards: [] } });
    if (path === "/api/v1/notifications/unread-count") return route.fulfill({ json: { unread: 0 } });
    if (path === "/api/v1/launcher/categories") return route.fulfill({ json: { categories: [] } });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await mockFrames(page, options.installations ?? (() => [planner, ledger]), options.frameApi ?? (() => ({ json: { title: "저장된 제목" } })));
  return { patches, launches };
}

test("a side panel opens beside the main frame with its own location", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  const { launches } = await open(page);
  await page.goto("/environments/prod/apps/ledger/reports");
  await expect(page.locator("iframe.app-frame")).toHaveCount(1);
  await page.getByRole("button", { name: "보조 패널" }).click();
  const menu = page.getByRole("list", { name: "보조 패널" });
  await expect(menu.getByRole("button", { name: "할 일 플래너", exact: true })).toBeVisible();
  await menu.getByRole("button", { name: "할 일 플래너", exact: true }).click();
  const panel = page.getByRole("complementary", { name: "할 일 패널" });
  await expect(panel).toBeVisible();
  await expect(panel.locator("iframe")).toHaveAttribute("title", "할 일 패널");
  await expect(page.frameLocator("iframe.app-frame-panel").getByLabel("문서 제목")).toHaveValue("저장된 제목");
  // The panel launch names the panel's path; the address keeps the main frame's.
  await expect.poll(() => launches.map((form) => form.get("pathname"))).toContain("/widgets/todos");
  await expect(page).toHaveURL(/\/environments\/prod\/apps\/ledger\/reports$/);
  // Moving the main frame to another page keeps the panel open.
  await page.locator("#platform-navigation").getByRole("link", { name: "홈", exact: true }).click();
  await expect(page).toHaveURL(/\/environments\/prod$/);
  await expect(panel).toBeVisible();
  // One panel at a time: choosing another replaces it.
  await page.getByRole("button", { name: "보조 패널" }).click();
  await page.getByRole("list", { name: "보조 패널" }).getByRole("button", { name: "메모 플래너", exact: true }).click();
  await expect(page.getByRole("complementary", { name: "메모 패널" })).toBeVisible();
  await expect(page.locator("iframe.app-frame-panel")).toHaveCount(1);
  // Until the panel reports its input state, closing asks first (as for the main frame).
  await expect(page.frameLocator("iframe.app-frame-panel").getByLabel("문서 제목")).toHaveValue("저장된 제목");
  await page.getByRole("button", { name: "패널 닫기" }).click();
  await expect(page.locator("iframe.app-frame-panel")).toHaveCount(0);
});

test("pinning a panel saves the preference and shows it in the header", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  const { patches } = await open(page);
  await page.goto("/environments/prod");
  await page.getByRole("button", { name: "보조 패널" }).click();
  await page.getByRole("button", { name: "메모 고정", exact: true }).click();
  await expect.poll(() => patches).toEqual([{ body: { panelPin: { id: "planner/memos", pinned: true } }, csrf: "p-csrf" }]);
  const chip = page.locator(".panel-chip", { hasText: "메모" });
  await expect(chip).toBeVisible();
  await expect(page.getByRole("button", { name: "메모 고정 해제" })).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");
  await chip.click();
  await expect(page.getByRole("complementary", { name: "메모 패널" })).toBeVisible();
  await expect(chip).toHaveAttribute("aria-pressed", "true");
  await expect(page.frameLocator("iframe.app-frame-panel").getByLabel("문서 제목")).toHaveValue("저장된 제목");
});

test("a pin for a panel outside this portal can be removed", async ({ page }) => {
  const { patches } = await open(page, { pins: ["other/panel"] });
  await page.goto("/environments/prod");
  await page.getByRole("button", { name: "보조 패널" }).click();
  const unpin = page.getByRole("button", { name: "other/panel 고정 해제" });
  await expect(unpin).toBeVisible();
  await unpin.click();
  await expect.poll(() => patches).toEqual([{ body: { panelPin: { id: "other/panel", pinned: false } }, csrf: "p-csrf" }]);
  await expect(unpin).toHaveCount(0);
});

test("the full pin allowance keeps the panels menu and account actions inside the header", async ({ page }) => {
  const panels = Array.from({ length: 16 }, (_, index) => ({
    id: `panel-${index}`, title: `고정한 보조 패널 번호 ${index}`, path: "/widgets/todos",
  }));
  await open(page, {
    pins: panels.map(({ id }) => `planner/${id}`),
    installations: () => [{ ...planner, panels }, ledger],
  });
  await page.goto("/environments/prod");
  const toggle = page.getByRole("button", { name: "보조 패널", exact: true });
  const signOut = page.locator(".header-account").getByRole("button").last();
  await expect(toggle).toBeVisible();
  for (const width of [1024, 1400, 1920]) {
    await page.setViewportSize({ width, height: 850 });
    for (const element of [toggle, signOut]) {
      const bounds = await element.boundingBox();
      expect(bounds).toBeTruthy();
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
    }
  }
  await toggle.click();
  await expect(page.getByRole("list", { name: "보조 패널" }).locator("li")).toHaveCount(16);
});

test("a full reload releases the previous document's frame slot", async ({ page }) => {
  await open(page);
  await page.goto("/environments/prod/apps/ledger");
  await expect(page.frameLocator("iframe.app-frame").getByLabel("문서 제목")).toHaveValue("저장된 제목");
  const deletion = page.waitForRequest((request) =>
    request.method() === "DELETE" && new URL(request.url()).pathname.startsWith("/api/v1/app-frames/"),
  );
  await page.reload();
  await deletion;
  await expect(page.frameLocator("iframe.app-frame").getByLabel("문서 제목")).toHaveValue("저장된 제목");
});

test("unsaved input in a panel is confirmed before it closes", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await open(page);
  await page.goto("/environments/prod");
  await page.getByRole("button", { name: "보조 패널" }).click();
  await page.getByRole("list", { name: "보조 패널" }).getByRole("button", { name: "할 일 플래너", exact: true }).click();
  const input = page.frameLocator("iframe.app-frame-panel").getByLabel("문서 제목");
  await expect(input).toHaveValue("저장된 제목");
  await input.fill("미저장 제목");
  page.once("dialog", async (dialog) => {
    expect(dialog.message()).toContain("저장하지 않은 제목");
    await dialog.dismiss();
  });
  await page.getByRole("button", { name: "패널 닫기" }).click();
  await expect(input).toHaveValue("미저장 제목");
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "패널 닫기" }).click();
  await expect(page.locator("iframe.app-frame-panel")).toHaveCount(0);
});

test("a release change preserves the panel draft until an explicit reopen", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  const { launches } = await open(page, {
    frameApi: (_app, _path, method) => method === "GET"
      ? { json: { title: "저장된 제목" } }
      : { status: 409, json: { error: { code: "release_changed_reload_required" } } },
  });
  await page.goto("/environments/prod");
  await page.getByRole("button", { name: "보조 패널" }).click();
  await page.getByRole("button", { name: "할 일 플래너", exact: true }).click();
  const frame = page.frameLocator("iframe.app-frame-panel");
  const input = frame.getByLabel("문서 제목");
  await expect(input).toHaveValue("저장된 제목");
  await input.fill("미저장 패널 제목");
  await frame.getByRole("button", { name: "저장", exact: true }).click();
  const reopen = page.getByRole("button", { name: "다시 열기", exact: true });
  await expect(reopen).toBeVisible();
  await expect(input).toHaveValue("미저장 패널 제목");
  expect(launches).toHaveLength(1);
  page.once("dialog", (dialog) => dialog.dismiss());
  await reopen.click();
  await expect(input).toHaveValue("미저장 패널 제목");
  page.once("dialog", (dialog) => dialog.accept());
  await reopen.click();
  await expect(input).toHaveValue("저장된 제목");
  expect(launches).toHaveLength(2);
});

test("promoting a panel closes it only when the main frame accepts navigation", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await open(page);
  await page.goto("/environments/prod/apps/ledger/reports");
  const main = page.frameLocator("iframe.app-frame:not(.app-frame-panel)").getByLabel("문서 제목");
  await expect(main).toHaveValue("저장된 제목");
  await page.getByRole("button", { name: "보조 패널" }).click();
  await page.getByRole("button", { name: "할 일 플래너", exact: true }).click();
  const panel = page.frameLocator("iframe.app-frame-panel").getByLabel("문서 제목");
  await expect(panel).toHaveValue("저장된 제목");
  await main.fill("미저장 주 화면 제목");
  await panel.fill("미저장 패널 제목");
  page.once("dialog", dialog => dialog.dismiss());
  await page.getByRole("button", { name: "앱에서 열기", exact: true }).click();
  await expect(page).toHaveURL(/\/apps\/ledger\/reports$/);
  await expect(panel).toHaveValue("미저장 패널 제목");
  page.on("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "앱에서 열기", exact: true }).click();
  await expect(page).toHaveURL(/\/apps\/planner\/widgets\/todos$/);
  await expect(page.locator("iframe.app-frame-panel")).toHaveCount(0);
  await page.getByRole("navigation", { name: "열린 앱" }).getByRole("button", { name: /장부/ }).first().click();
  await expect(page.frameLocator("iframe.app-frame:visible").getByLabel("문서 제목")).toHaveValue("미저장 주 화면 제목");
});

test("narrow screens open the app at the panel's path instead", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 900 });
  await open(page);
  await page.goto("/environments/prod");
  await page.getByRole("button", { name: "보조 패널" }).click();
  await page.getByRole("list", { name: "보조 패널" }).getByRole("button", { name: "할 일 플래너", exact: true }).click();
  await expect(page).toHaveURL(/\/environments\/prod\/apps\/planner\/widgets\/todos$/);
  await expect(page.locator("iframe.app-frame-panel")).toHaveCount(0);
});

test("the home shows summary cards as text and marks an app that did not answer", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await open(page, {
    cards: {
      cards: [
        {
          installationId: planner.id,
          appId: "planner",
          appName: "플래너",
          status: "ok",
          title: "오늘 일정",
          count: 2,
          items: [
            { title: "<b>주간 회의</b>", subtitle: "회의실 A", at: "2026-11-22T01:00:00Z", path: "/calendar?date=2026-11-22" },
            { title: "고객 통화", subtitle: null, at: null, path: null },
          ],
        },
        { installationId: ledger.id, appId: "ledger", appName: "장부", status: "unavailable", title: null, count: null, items: [] },
      ],
    },
  });
  await page.goto("/environments/prod");
  const cards = page.getByRole("region", { name: "요약" });
  await expect(cards.getByText("오늘 일정")).toBeVisible();
  // App text is shown as text, never as markup.
  await expect(cards.getByText("<b>주간 회의</b>")).toBeVisible();
  await expect(cards.locator("b")).toHaveCount(0);
  await expect(cards.getByText("고객 통화")).toBeVisible();
  await expect(cards.getByText("가져오지 못함")).toBeVisible();
  await cards.getByRole("link", { name: /주간 회의/ }).click();
  await expect(page).toHaveURL(/\/environments\/prod\/apps\/planner\/calendar\?date=2026-11-22$/);
});

test("an installation removed from the portal no longer displays its cached home card", async ({ page }) => {
  let apps = [planner, ledger];
  await open(page, {
    installations: () => apps,
    cards: { cards: [{
      installationId: planner.id,
      appId: planner.appId,
      appName: planner.name,
      status: "ok",
      title: "오늘 일정",
      count: 1,
      items: [{ title: "주간 회의" }],
    }] },
  });
  await page.goto("/environments/prod");
  await expect(page.getByText("주간 회의")).toBeVisible();
  apps = [ledger];
  await expect(page.getByText("주간 회의")).toHaveCount(0);
  await expect(page.getByRole("main").getByText("장부", { exact: true })).toBeVisible();
});

test("opened apps reuse their frames over thirty round trips", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width: 1400, height: 900 });
  const { launches } = await open(page);
  await page.goto("/environments/prod/apps/ledger/reports");
  const input = (title: string) => page.frameLocator(`iframe[title="${title}"]`).getByLabel("문서 제목");
  await expect(input("장부")).toBeVisible();
  const times: number[] = [];
  for (let round = 0; round < 30; round++) {
    for (const title of ["플래너", "장부"]) {
      const start = performance.now();
      await page.locator(".rail-apps").getByRole("link", { name: title, exact: true }).click();
      await input(title).waitFor({ state: "visible" });
      times.push(performance.now() - start);
    }
  }
  console.log(JSON.stringify({ switches: times.length, launches: launches.length,
    p95Ms: Math.round(times.sort((a,b) => a-b)[Math.ceil(times.length*.95)-1]!) }));
  expect(launches).toHaveLength(2);
  await expect(page).toHaveURL(/ledger\/reports$/);
});

for (const count of [1, 5, 10]) {
  test(`idle cost with ${count} resident app frames`, async ({ page, browser }) => {
    test.skip(process.env.MIY_MEASURE_TAB_COST !== "1", "Five-minute resource measurement");
    test.setTimeout(420_000);
    await page.setViewportSize({ width: 1400, height: 1000 });
    const apps = Array.from({ length: count }, (_, i) => ({
      ...ledger, id: `40000000-0000-4000-8000-${String(i+1).padStart(12, "0")}`,
      appId: `bench-${i}`, name: `Bench ${i}`,
    }));
    let polls = 0, requests = 0, sockets = 0;
    page.on("request", r => {
      ++requests;
      if (new URL(r.url()).pathname === "/api/v1/installations") ++polls;
    });
    page.on("websocket", ws => { ++sockets; ws.on("close", () => --sockets); });
    await open(page, { installations: () => apps });
    await page.goto("/environments/prod/apps/bench-0");
    for (const app of apps) {
      await page.locator(".rail-apps").getByRole("link", { name: app.name, exact: true }).click();
      await page.frameLocator(`iframe[title="${app.name}"]`).getByLabel("문서 제목").waitFor();
    }
    const cdp = await browser.newBrowserCDPSession();
    async function rendererCost() {
      const { processInfo } = await cdp.send("SystemInfo.getProcessInfo");
      const renderers = processInfo.filter(p => p.type.toLowerCase() === "renderer");
      const rssKiB = renderers.reduce((sum, p) => sum + Number(/^VmRSS:\s+(\d+)/m.exec(readFileSync(`/proc/${p.id}/status`, "utf8"))?.[1] ?? 0), 0);
      return { rssMiB: Math.round(rssKiB / 1024), cpuSeconds: renderers.reduce((sum,p) => sum+p.cpuTime, 0) };
    }
    requests=0; polls=0;
    const start = await rendererCost();
    await page.waitForTimeout(300_000);
    const end = await rendererCost();
    console.log(JSON.stringify({ resident: count, seconds: 300, rssMiB: end.rssMiB, cpuSeconds: end.cpuSeconds - start.cpuSeconds, requests, polls, sockets }));
    expect(polls).toBeGreaterThan(80);
    expect(polls).toBeLessThanOrEqual(102);
    await expect(page.locator("iframe.app-frame")).toHaveCount(count);
    await cdp.detach();
  });
}

test("a full account keeps the previous tab, draft, and URL", async ({ page }) => {
  await open(page);
  await page.goto("/environments/prod/apps/ledger/reports");
  const input = page.frameLocator("iframe.app-frame:visible").getByLabel("문서 제목");
  await input.fill("보존할 입력");
  await page.route("**/api/v1/app-frames", route => route.fulfill({ status: 409, json: { error: { code: "frame_limit_reached" } } }));
  await page.locator("#platform-navigation").getByRole("link", { name: "플래너", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("10개");
  await expect(page).toHaveURL(/\/ledger\/reports$/);
  await expect(input).toHaveValue("보존할 입력");
  await expect(page.locator("iframe.app-frame")).toHaveCount(1);
});
