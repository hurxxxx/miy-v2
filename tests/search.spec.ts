import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";

// Integrated search in the portal (M12): the palette hands the typed text to
// the search page, which shows each app's answer as text and marks an app that
// did not answer. Mocked API; the Core regression covers who is asked.
const planner = {
  id: "40000000-0000-4000-8000-000000000081",
  appId: "planner",
  name: "플래너",
  releaseId: "50000000-0000-4000-8000-000000000081",
  environment: "prod",
  version: "1.0.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  panels: [],
  ui: { entry: "/artifacts/health-fixture/mf-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};
const board = { ...planner, id: "40000000-0000-4000-8000-000000000082", appId: "board", name: "게시판" };
const messenger = { ...planner, id: "40000000-0000-4000-8000-000000000083", appId: "messenger", name: "메신저" };

async function open(page: Page) {
  await mockSurface(page, "portal");
  const queries: string[] = [];
  let installations = [planner, board, messenger];
  await page.route("**/api/**", async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (path === "/api/v1/me")
      return route.fulfill({ json: browserUser(route, { id: "u", name: "검색 사용자", admin: false, csrf: "q-csrf", preferences: defaultPreferences }) });
    if (path === "/api/v1/installations") return route.fulfill({ json: installations });
    if (path === "/api/v1/home/cards") return route.fulfill({ json: { cards: [] } });
    if (path === "/api/v1/notifications/unread-count") return route.fulfill({ json: { unread: 0 } });
    if (path === "/api/v1/launcher/categories") return route.fulfill({ json: { categories: [] } });
    if (path === "/api/v1/search") {
      const q = url.searchParams.get("q") ?? "";
      queries.push(q);
      return route.fulfill({
        json: {
          query: q,
          groups: [
            { installationId: planner.id, appId: "planner", appName: "플래너", status: "ok", items: [
              { title: "<b>주간 회의</b>", snippet: "회의실 A", at: "2026-11-22T01:00:00Z", path: "/?event=1", kind: "일정" },
              { title: "회의 준비", snippet: null, at: null, path: "/../other-app", kind: null },
            ] },
            { installationId: board.id, appId: board.appId, appName: board.name, status: "unavailable", items: [] },
            { installationId: messenger.id, appId: messenger.appId, appName: messenger.name, status: "ok", items: [] },
          ],
        },
      });
    }
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  return { queries, removePlanner: () => { installations = [board, messenger]; } };
}

test("the palette sends typed text to the integrated search", async ({ page }) => {
  const { queries } = await open(page);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "알림", exact: true })).toBeVisible();
  await page.keyboard.press("Control+k");
  const field = page.getByRole("combobox", { name: "명령 검색" });
  await field.fill("회의");
  await expect(page.getByRole("option", { name: "통합 검색: 회의" })).toBeVisible();
  await field.dispatchEvent("keydown", { key: "Enter", isComposing: true });
  await expect(field).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/search\?q=%ED%9A%8C%EC%9D%98$/);
  await expect(page.getByRole("searchbox", { name: "검색어" })).toHaveValue("회의");
  await expect.poll(() => queries).toEqual(["회의"]);
});

test("direct links, the search form and the palette preserve numeric search text", async ({ page }) => {
  const { queries } = await open(page);
  await page.goto("/search?q=1e2");
  const field = page.getByRole("searchbox", { name: "검색어" });
  await expect(field).toHaveValue("1e2");
  await expect.poll(() => queries).toEqual(["1e2"]);

  await field.fill("123");
  await page.getByRole("button", { name: "검색", exact: true }).click();
  await expect.poll(() => queries).toEqual(["1e2", "123"]);
  await expect(field).toHaveValue("123");

  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "명령 검색" }).fill("456");
  await page.keyboard.press("Enter");
  await expect.poll(() => queries).toEqual(["1e2", "123", "456"]);
  await expect(field).toHaveValue("456");
});

test("results are grouped by app as text; an app that did not answer is marked", async ({ page }) => {
  await open(page);
  await page.goto("/search?q=회의");
  const plannerGroup = page.locator(".search-group").filter({ has: page.getByRole("heading", { name: "플래너" }) });
  await expect(plannerGroup.getByText("<b>주간 회의</b>")).toBeVisible();
  await expect(plannerGroup.locator("b")).toHaveCount(0);
  await expect(plannerGroup.getByText("일정")).toBeVisible();
  await expect(plannerGroup.getByText("회의 준비")).toBeVisible();
  await expect(plannerGroup.getByRole("link", { name: "회의 준비" })).toHaveCount(0);
  const boardGroup = page.locator(".search-group").filter({ has: page.getByRole("heading", { name: "게시판" }) });
  await expect(boardGroup.getByText("가져오지 못함")).toBeVisible();
  // An app that answered with nothing takes no space.
  await expect(page.getByRole("heading", { name: "메신저" })).toHaveCount(0);
  await plannerGroup.getByRole("link", { name: /주간 회의/ }).click();
  await expect(page).toHaveURL(/\/environments\/prod\/apps\/planner\/\?event=1$/);
});

test("an app removed from the visible installation list leaves the open search results", async ({ page }) => {
  const { removePlanner } = await open(page);
  await page.goto("/search?q=회의");
  const hit = page.getByRole("link", { name: /주간 회의/ });
  await expect(hit).toBeVisible();
  removePlanner();
  await expect(hit).toHaveCount(0);
  const boardGroup = page.locator(".search-group").filter({ has: page.getByRole("heading", { name: "게시판" }) });
  await expect(boardGroup.getByText("가져오지 못함")).toBeVisible();
});
