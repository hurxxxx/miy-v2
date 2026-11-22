import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page, type Route } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";
import { mockFrames } from "./frame-mock";

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

async function open(page: Page, options: { cards?: unknown } = {}) {
  await mockSurface(page, "portal");
  let preferences = { ...defaultPreferences };
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
      const body = request.postDataJSON() as { pinnedPanels: string[] };
      patches.push({ body, csrf: request.headers()["x-miy-csrf"] ?? null });
      preferences = { ...preferences, pinnedPanels: body.pinnedPanels };
      return route.fulfill({ json: { name: "패널 사용자", preferences } });
    }
    if (path === "/api/v1/installations") return route.fulfill({ json: [planner, ledger] });
    if (path === "/api/v1/home/cards") return route.fulfill({ json: options.cards ?? { cards: [] } });
    if (path === "/api/v1/notifications/unread-count") return route.fulfill({ json: { unread: 0 } });
    if (path === "/api/v1/launcher/categories") return route.fulfill({ json: { categories: [] } });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await mockFrames(page, () => [planner, ledger], () => ({ json: { title: "저장된 제목" } }));
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
  await expect.poll(() => patches).toEqual([{ body: { pinnedPanels: ["planner/memos"] }, csrf: "p-csrf" }]);
  const chip = page.locator(".panel-chip", { hasText: "메모" });
  await expect(chip).toBeVisible();
  await expect(page.getByRole("button", { name: "메모 고정 해제" })).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");
  await chip.click();
  await expect(page.getByRole("complementary", { name: "메모 패널" })).toBeVisible();
  await expect(chip).toHaveAttribute("aria-pressed", "true");
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
