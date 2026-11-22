import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

// Console additions of the common features work (P1): user search, filter and
// CSV export, release permission approval, and the console in English. The
// requests are mocked; Core authorization is covered by its own regression.
const accounts = [
  { id: "admin", name: "관리자", admin: true, canDevelop: false, enabled: true },
  { id: "alice", name: "앨리스", admin: false, canDevelop: true, enabled: true },
  { id: "bob", name: "=SUM(1)", admin: false, canDevelop: false, enabled: false },
];
const installation = {
  id: "ledger-dev",
  appId: "ledger",
  name: "장부",
  environment: "dev",
  releaseId: "50000000-0000-4000-8000-000000000031",
  version: "1.2.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  canManage: false,
  canOpen: true,
  ui: { entry: "/unused-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};

async function open(page: Page, options: { locale?: string; conflict?: boolean } = {}) {
  await mockSurface(page, "console");
  const puts: unknown[] = [];
  const switches: { user: string; body: unknown; csrf: string | null }[] = [];
  let approval = {
    installationId: installation.id,
    releaseId: installation.releaseId,
    requestedPermissions: ["database", "platform:directory:read", "platform:files:read"],
    approvedPermissions: [] as string[],
    revision: 0,
    approvedAt: null as string | null,
  };
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    if (path === "/api/v1/me")
      return route.fulfill({
        json: browserUser(route, {
          id: "admin", name: "관리자", admin: true, csrf: "console-csrf",
          preferences: { ...defaultPreferences, locale: options.locale ?? "ko-KR" },
        }),
      });
    if (path === `/api/v1/installations/${installation.id}/permissions`) {
      if (request.method() === "PUT") {
        const body = request.postDataJSON() as { permissions: string[]; expectedRevision: number };
        puts.push({ body, csrf: request.headers()["x-miy-csrf"] });
        if (options.conflict) return route.fulfill({ status: 409, json: { error: { code: "permission_revision_changed" } } });
        approval = { ...approval, approvedPermissions: body.permissions, revision: body.expectedRevision + 1, approvedAt: new Date().toISOString() };
      }
      return route.fulfill({ json: approval });
    }
    const reply: Record<string, unknown> = {
      "/api/v1/installations": [],
      "/api/v1/runtime/installations": [installation],
      "/api/v1/app-registrations": [],
      "/api/v1/account-links": { available: true, links: [] },
      "/api/v1/users": accounts,
      "/api/v1/groups": [],
      "/api/v1/apps": { apps: [], nextCursor: null },
      "/api/v1/audit": { entries: [], nextCursor: null },
      "/api/v1/support/reports": { entries: [], nextCursor: null },
    };
    const switchStart = path.match(/^\/api\/v1\/users\/([^/]+)\/sign-in-as$/);
    if (switchStart && request.method() === "POST") {
      switches.push({ user: decodeURIComponent(switchStart[1]!), body: request.postDataJSON(), csrf: request.headers()["x-miy-csrf"] ?? null });
      const portal = (request.postDataJSON() as { environment: string }).environment === "dev" ? "https://dev.example.test" : "https://portal.example.test";
      return route.fulfill({ json: { url: `${portal}/switch-user#one-time-code`, expiresAt: new Date(Date.now() + 60000).toISOString() } });
    }
    if (path in reply) return route.fulfill({ json: reply[path] });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await page.goto("/admin");
  return { puts, switches };
}
const usersPanel = (page: Page) =>
  page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 계정", exact: true }) });

test("user search and filter narrow the account list", async ({ page }) => {
  await open(page);
  const panel = usersPanel(page);
  await expect(panel.locator(".user-account")).toHaveCount(3);
  await panel.getByLabel("사용자 검색").fill("앨리");
  await expect(panel.locator(".user-account")).toHaveCount(1);
  await expect(panel.getByText("1명 표시")).toBeVisible();
  await panel.getByLabel("사용자 검색").fill("");
  await panel.getByLabel("사용자 구분").selectOption("disabled");
  await expect(panel.locator(".user-account")).toHaveCount(1);
  await expect(panel.locator(".user-account strong")).toHaveText("=SUM(1)");
  await panel.getByLabel("사용자 구분").selectOption("admin");
  await expect(panel.locator(".user-account strong")).toHaveText("관리자");
});

test("CSV export writes the shown accounts, quoted and safe for spreadsheets", async ({ page }) => {
  await open(page);
  const panel = usersPanel(page);
  const [download] = await Promise.all([page.waitForEvent("download"), panel.getByRole("button", { name: "CSV 내보내기" }).click()]);
  expect(download.suggestedFilename()).toBe("miy-사용자.csv");
  const text = await readFile((await download.path())!, "utf8");
  expect(text).toBe(
    "﻿" +
      ['"id","name","admin","canDevelop","enabled"', '"admin","관리자","true","false","true"', '"alice","앨리스","false","true","true"', `"bob","'=SUM(1)","false","false","false"`].join("\r\n") +
      "\r\n",
  );
});

test("an administrator approves a release's requested platform permissions in place", async ({ page }) => {
  const { puts } = await open(page);
  const card = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "장부 · dev" }) });
  await expect(card.getByText("승인 대기 2개")).toBeVisible();
  await expect(page.getByText("권한 승인을 기다리는 설치가 1건 있습니다.", { exact: false })).toBeVisible();
  // The app's own database is not a platform permission and is not offered.
  await expect(card.getByText("database", { exact: true })).toHaveCount(0);
  await card.getByRole("button", { name: "요청한 권한 모두 승인" }).click();
  await expect(card.getByText("승인됨")).toBeVisible();
  await expect(page.getByText("권한 승인을 기다리는 설치가", { exact: false })).toHaveCount(0);
  expect(puts).toEqual([
    {
      body: { releaseId: installation.releaseId, expectedRevision: 0, permissions: ["platform:directory:read", "platform:files:read"] },
      csrf: "console-csrf",
    },
  ]);
});

test("a stale approval is refused and the latest state is shown", async ({ page }) => {
  await open(page, { conflict: true });
  const card = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "장부 · dev" }) });
  await card.getByRole("button", { name: "요청한 권한 모두 승인" }).click();
  await expect(card.getByRole("alert")).toHaveText("다른 관리자가 먼저 바꿨습니다. 최신 상태를 다시 불러왔습니다.");
});

test("the console speaks English when the saved language is English", async ({ page }) => {
  await open(page, { locale: "en-US" });
  await expect(page.getByRole("heading", { name: "Platform administration" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "User accounts", exact: true }).first()).toBeVisible();
  await expect(page.getByText("2 awaiting approval")).toBeVisible();
  await expect(page.getByRole("button", { name: "Export CSV" })).toBeVisible();
});

test("sign in as a user opens the chosen portal in a new tab with a one-time link", async ({ page, context }) => {
  const { switches } = await open(page);
  const panel = usersPanel(page);
  const rows = panel.locator(".user-account");
  // Not for yourself, not for a disabled account.
  await expect(rows.filter({ hasText: "관리자" }).getByRole("button", { name: "이 사용자로 로그인" })).toHaveCount(0);
  await expect(rows.filter({ hasText: "=SUM(1)" }).getByRole("button", { name: "이 사용자로 로그인" })).toHaveCount(0);
  await context.route("https://dev.example.test/**", (route) => route.fulfill({ contentType: "text/html", body: "<p>dev portal</p>" }));
  await rows.filter({ hasText: "앨리스" }).getByRole("button", { name: "이 사용자로 로그인" }).click();
  const dialog = page.getByRole("dialog", { name: "앨리스(으)로 로그인" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("dev 포털").check();
  const [tab] = await Promise.all([context.waitForEvent("page"), dialog.getByRole("button", { name: "로그인", exact: true }).click()]);
  await tab.waitForURL("https://dev.example.test/switch-user#one-time-code");
  expect(switches).toEqual([{ user: "alice", body: { environment: "dev" }, csrf: "console-csrf" }]);
  expect(await tab.evaluate(() => window.opener)).toBeNull();
  await expect(page.getByText("새 탭에서 앨리스(으)로 로그인합니다.")).toBeVisible();
});
