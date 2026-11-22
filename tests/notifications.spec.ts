import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page, type Route } from "@playwright/test";

// The portal side of platform notifications (M2) and the Ctrl/⌘+K palette:
// what the portal requests and shows. The Core regression covers who may
// publish, recipient filtering, environments and the app recheck.
const minutes = (value: number) => new Date(Date.now() - value * 60000).toISOString();
const ledger = {
  id: "40000000-0000-4000-8000-000000000061",
  appId: "ledger",
  name: "장부",
  releaseId: "50000000-0000-4000-8000-000000000061",
  environment: "prod",
  version: "1.0.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  ui: { entry: "/artifacts/health-fixture/mf-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};

async function open(page: Page, surface: "portal" | "console" = "portal") {
  await mockSurface(page, surface);
  const sent: { method: string; path: string; body: unknown; csrf: string | null }[] = [];
  let items = [
    { id: "n1", installationId: ledger.id, appId: "ledger", appName: "장부", kind: "message", groupKey: "c-1", title: "앨리스 · 새 메시지 2개", path: "/conversations/1", resourceId: "1", createdAt: minutes(3), readAt: null },
    { id: "n2", installationId: ledger.id, appId: "ledger", appName: "장부", kind: "mention", groupKey: "c-2", title: "밥이 회원님을 언급했습니다", path: "/conversations/2", resourceId: "2", createdAt: minutes(90), readAt: minutes(30) },
  ];
  let mutes: { appId: string; kind: string }[] = [];
  await page.route("**/api/**", async (route: Route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    if (request.method() !== "GET")
      sent.push({ method: request.method(), path: path + url.search, body: request.postDataJSON?.() ?? null, csrf: request.headers()["x-miy-csrf"] ?? null });
    if (path === "/api/v1/me")
      return route.fulfill({ json: browserUser(route, { id: "u", name: "알림 사용자", admin: surface === "console", csrf: "n-csrf", preferences: defaultPreferences }) });
    if (path === "/api/v1/installations") return route.fulfill({ json: [ledger] });
    if (path === "/api/v1/notifications/unread-count") return route.fulfill({ json: { unread: items.filter((i) => !i.readAt).length } });
    if (path === "/api/v1/notifications") {
      sent.push({ method: "GET", path: path + url.search, body: null, csrf: null });
      return route.fulfill({ json: { items, unread: items.filter((i) => !i.readAt).length, nextCursor: null } });
    }
    if (path === "/api/v1/notifications/read-all") {
      items = items.map((i) => ({ ...i, readAt: i.readAt ?? minutes(0) }));
      return route.fulfill({ status: 204, body: "" });
    }
    const read = path.match(/^\/api\/v1\/notifications\/([^/]+)\/read$/);
    if (read) {
      items = items.map((i) => (i.id === read[1] ? { ...i, readAt: minutes(0) } : i));
      return route.fulfill({ status: 204, body: "" });
    }
    if (path === "/api/v1/notifications/settings") {
      if (request.method() === "PUT") mutes = (request.postDataJSON() as { mutes: typeof mutes }).mutes;
      return route.fulfill({ json: { mutes } });
    }
    if (path === "/api/v1/launcher/categories") return route.fulfill({ json: { categories: [] } });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  return { sent };
}

test("the bell shows the unread count and the panel lists notifications as text", async ({ page }) => {
  const { sent } = await open(page);
  await page.goto("/");
  const bell = page.getByRole("button", { name: "알림, 읽지 않음 1개" });
  await expect(bell).toBeVisible();
  await expect(bell).toContainText("1");
  await bell.click();
  const panel = page.getByRole("dialog", { name: "알림" });
  await expect(panel.getByText("앨리스 · 새 메시지 2개")).toBeVisible();
  await expect(panel.getByText("밥이 회원님을 언급했습니다")).toBeVisible();
  await expect(panel.getByText("장부").first()).toBeVisible();
  // Opening the panel asks the Core to recheck with the apps.
  expect(sent.some((s) => s.method === "GET" && s.path.includes("check=true"))).toBe(true);
});

test("opening a notification marks it read and opens its app at the app's path", async ({ page }) => {
  const { sent } = await open(page);
  await page.goto("/");
  await page.getByRole("button", { name: "알림, 읽지 않음 1개" }).click();
  await page.getByRole("dialog", { name: "알림" }).getByText("앨리스 · 새 메시지 2개").click();
  await expect(page).toHaveURL(/\/environments\/prod\/apps\/ledger\/conversations\/1$/);
  await expect.poll(() => sent.filter((s) => s.path === "/api/v1/notifications/n1/read")).toHaveLength(1);
  expect(sent.find((s) => s.path === "/api/v1/notifications/n1/read")?.csrf).toBe("n-csrf");
});

test("mark all as read clears the badge", async ({ page }) => {
  const { sent } = await open(page);
  await page.goto("/");
  await page.getByRole("button", { name: "알림, 읽지 않음 1개" }).click();
  await page.getByRole("button", { name: "모두 읽음으로 표시" }).click();
  await expect(page.getByRole("button", { name: "알림", exact: true })).toBeVisible();
  expect(sent.some((s) => s.path === "/api/v1/notifications/read-all" && s.csrf === "n-csrf")).toBe(true);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "알림" })).toHaveCount(0);
});

test("the console has no bell", async ({ page }) => {
  await open(page, "console");
  await page.goto("/settings/profile");
  await expect(page.getByRole("heading", { name: "내 설정" })).toBeVisible();
  await expect(page.locator(".notification-bell")).toHaveCount(0);
});

test("notification settings turn an app or one kind off", async ({ page }) => {
  const { sent } = await open(page);
  await page.goto("/settings/notifications");
  const all = page.getByLabel("장부 알림 받기");
  await expect(all).toBeChecked();
  await page.getByLabel("장부의 mention 알림 받기").uncheck();
  await expect.poll(() => sent.filter((s) => s.path === "/api/v1/notifications/settings").length).toBe(1);
  await all.uncheck();
  await expect.poll(() => sent.filter((s) => s.path === "/api/v1/notifications/settings").length).toBe(2);
  expect(sent.filter((s) => s.path === "/api/v1/notifications/settings").map((s) => s.body)).toEqual([
    { mutes: [{ appId: "ledger", kind: "mention" }] },
    { mutes: [{ appId: "ledger", kind: "mention" }, { appId: "ledger", kind: "*" }] },
  ]);
});

test("Ctrl+K opens the command palette and goes to an app", async ({ page }) => {
  await open(page);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "알림, 읽지 않음 1개" })).toBeVisible();
  await page.keyboard.press("Control+k");
  const search = page.getByRole("combobox", { name: "명령 검색" });
  await expect(search).toBeFocused();
  await search.fill("장부");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/environments\/prod\/apps\/ledger$/);
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "명령 검색" }).fill("설정");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/settings$/);
});
