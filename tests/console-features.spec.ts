import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { mockAdminUserPage } from "./admin-users-fixture";

// Console additions of the common features work (P1): user search, filter and
// CSV export, release permission approval, and the console in English. The
// requests are mocked; Core authorization is covered by its own regression.
const accounts = [
  { id: "admin", name: "관리자", admin: true, canDevelop: false, enabled: true, primaryGroupId: "division" },
  { id: "alice", name: "앨리스", admin: false, canDevelop: true, enabled: true, primaryGroupId: "team" },
  { id: "bob", name: "=SUM(1)", admin: false, canDevelop: false, enabled: false, primaryGroupId: null },
];
const organizations = [
  { id: "division", name: "본부", source: "hr" as const, parentId: null, active: true, members: 1 },
  { id: "team", name: "개발팀", source: "hr" as const, parentId: "division", active: true, members: 1 },
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

async function open(page: Page, options: { locale?: string; conflict?: boolean; path?: string; extraUsers?: number } = {}) {
  await mockSurface(page, "console");
  const puts: unknown[] = [];
  const switches: { user: string; body: unknown; csrf: string | null }[] = [];
  const quotas: { method: string; body: unknown; csrf: string | null }[] = [];
  const directoryQueries: URLSearchParams[] = [];
  const rows = accounts.concat(Array.from({ length: options.extraUsers ?? 0 }, (_, index) => ({
    id: `employee-${index}`, name: `직원 ${index}`, admin: false, canDevelop: false, enabled: true, primaryGroupId: null,
  })));
  let quota = { installationId: installation.id, custom: false, maxBytes: 21474836480, maxFiles: 100000, usedBytes: 1073741824, usedFiles: 12 };
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
      "/api/v1/groups": organizations,
      "/api/v1/apps": { apps: [], nextCursor: null },
      "/api/v1/apps/ledger": { app: { id: "ledger", name: "장부", distribution: "organization", disabled: false }, installations: [installation] },
      "/api/v1/admin/overview": {
        pendingRegistrations: 0, pendingApprovals: approval.approvedPermissions.length ? 0 : 1, runtimeIssues: 0,
        approvalExamples: approval.approvedPermissions.length ? [] : [{ appId: "ledger", name: "장부", environment: "dev" }],
        runtimeExamples: [],
      },
      "/api/v1/audit": { entries: [], nextCursor: null },
      "/api/v1/support/reports": { entries: [], nextCursor: null },
    };
    if (path === "/api/v1/users/directory") {
      directoryQueries.push(url.searchParams);
      return route.fulfill({ json: mockAdminUserPage(rows, organizations, url.searchParams) });
    }
    if (path === `/api/v1/installations/${installation.id}/file-quota`) {
      if (request.method() === "PUT") {
        const body = request.postDataJSON() as { maxBytes: number; maxFiles: number };
        quotas.push({ method: "PUT", body, csrf: request.headers()["x-miy-csrf"] ?? null });
        quota = { ...quota, custom: true, ...body };
      } else if (request.method() === "DELETE") {
        quotas.push({ method: "DELETE", body: null, csrf: request.headers()["x-miy-csrf"] ?? null });
        quota = { ...quota, custom: false, maxBytes: 21474836480, maxFiles: 100000 };
      }
      return route.fulfill({ json: quota });
    }
    const switchStart = path.match(/^\/api\/v1\/users\/([^/]+)\/sign-in-as$/);
    if (switchStart && request.method() === "POST") {
      switches.push({ user: decodeURIComponent(switchStart[1]!), body: request.postDataJSON(), csrf: request.headers()["x-miy-csrf"] ?? null });
      const portal = (request.postDataJSON() as { environment: string }).environment === "dev" ? "https://dev.example.test" : "https://portal.example.test";
      return route.fulfill({ json: { url: `${portal}/switch-user#one-time-code`, expiresAt: new Date(Date.now() + 60000).toISOString() } });
    }
    if (path in reply) return route.fulfill({ json: reply[path] });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await page.goto(options.path ?? "/admin");
  return { puts, switches, quotas, directoryQueries };
}
const usersPanel = (page: Page) =>
  page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 계정", exact: true }) });

test("user search and filter narrow the account list", async ({ page }) => {
  const { directoryQueries } = await open(page, { path: "/admin/users" });
  const panel = usersPanel(page);
  await expect(panel.locator(".user-account")).toHaveCount(3);
  await panel.getByLabel("사용자 검색").fill("앨리");
  await panel.getByRole("button", { name: "검색", exact: true }).click();
  await expect(panel.locator(".user-account")).toHaveCount(1);
  await expect(panel.getByText("1페이지 · 1명")).toBeVisible();
  expect(directoryQueries.at(-1)?.get("q")).toBe("앨리");
  expect(new URL(page.url()).searchParams.get("q")).toBe("앨리");
  await panel.getByLabel("사용자 검색").fill("");
  await panel.getByRole("button", { name: "검색", exact: true }).click();
  await panel.getByLabel("계정 상태").selectOption("disabled");
  await expect(panel.locator(".user-account")).toHaveCount(1);
  await expect(panel.locator(".user-account strong")).toHaveText("=SUM(1)");
  await panel.getByLabel("계정 상태").selectOption("");
  await panel.getByLabel("사용자 역할").selectOption("admin");
  await expect(panel.locator(".user-account strong")).toHaveText("관리자");
  expect(directoryQueries.at(-1)?.get("role")).toBe("admin");
});

test("CSV export writes the shown accounts, quoted and safe for spreadsheets", async ({ page }) => {
  await open(page, { path: "/admin/users" });
  const panel = usersPanel(page);
  const [download] = await Promise.all([page.waitForEvent("download"), panel.getByRole("button", { name: "검색 결과 CSV" }).click()]);
  expect(download.suggestedFilename()).toBe("miy-사용자.csv");
  const text = await readFile((await download.path())!, "utf8");
  expect(text).toBe(
    "﻿" +
      ['"id","name","admin","canDevelop","enabled","primaryGroupId","primaryGroupName"', '"admin","관리자","true","false","true","division","본부"', '"alice","앨리스","false","true","true","team","개발팀"', `"bob","'=SUM(1)","false","false","false","",""`].join("\r\n") +
      "\r\n",
  );
});

test("user directory pages on the server and shares organization filters in its URL", async ({ page }) => {
  const { directoryQueries } = await open(page, { path: "/admin/users", extraUsers: 24 });
  const panel = usersPanel(page);
  await expect(panel.locator(".user-account")).toHaveCount(25);
  await panel.getByRole("button", { name: "다음 페이지" }).click();
  await expect(panel.locator(".user-account")).toHaveCount(2);
  expect(directoryQueries.at(-1)?.get("cursor")).toBe("25");
  await panel.getByRole("button", { name: "이전 페이지" }).click();
  await expect(panel.locator(".user-account")).toHaveCount(25);

  await page.getByRole("button", { name: "본부", exact: true }).click();
  await expect(panel.locator(".user-account strong")).toHaveText("관리자");
  await page.getByLabel("하위 조직의 사용자도 표시").check();
  await expect(panel.locator(".user-account strong")).toHaveText(["관리자", "앨리스"]);
  expect(directoryQueries.at(-1)?.get("descendants")).toBe("true");
  await page.reload();
  await expect(panel.locator(".user-account strong")).toHaveText(["관리자", "앨리스"]);
  await expect(page.getByLabel("하위 조직의 사용자도 표시")).toBeChecked();
  await page.getByRole("button", { name: "미소속", exact: true }).click();
  await expect(panel.locator(".user-account")).toHaveCount(25);
  expect(directoryQueries.at(-1)?.get("organization")).toBe("unassigned");
  expect(directoryQueries.at(-1)?.get("descendants")).toBeNull();
  await page.goBack();
  await expect(panel.locator(".user-account strong")).toHaveText(["관리자", "앨리스"]);
});

test("CSV export walks all server pages for the active filters", async ({ page }) => {
  const { directoryQueries } = await open(page, { path: "/admin/users?q=직원", extraUsers: 203 });
  const panel = usersPanel(page);
  await expect(panel.locator(".user-account")).toHaveCount(25);
  const [download] = await Promise.all([page.waitForEvent("download"), panel.getByRole("button", { name: "검색 결과 CSV" }).click()]);
  const csv = await readFile((await download.path())!, "utf8");
  expect(csv.trimEnd().split("\r\n")).toHaveLength(204);
  expect(csv).toContain('"employee-202","직원 202"');
  expect(csv).not.toContain('"alice"');
  const exportQueries = directoryQueries.filter((query) => query.get("limit") === "200");
  expect(exportQueries.map((query) => [query.get("q"), query.get("cursor")])).toEqual([["직원", null], ["직원", "200"]]);
});

test("an administrator approves a release's requested platform permissions in place", async ({ page }) => {
  const { puts } = await open(page, { path: "/admin/apps/ledger/installations" });
  const card = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "테스트", exact: true }) });
  await expect(card.getByText("승인 대기 2개")).toBeVisible();
  // The app's own database is not a platform permission and is not offered.
  await expect(card.getByText("database", { exact: true })).toHaveCount(0);
  await card.getByRole("button", { name: "요청한 권한 모두 승인" }).click();
  await expect(card.getByText("승인됨")).toBeVisible();
  await page.getByRole("link", { name: "개요", exact: true }).click();
  await expect(page.getByRole("heading", { name: "릴리스 권한 승인 대기" })).toBeVisible();
  await expect(page.getByRole("link", { name: "장부", exact: true })).toHaveCount(0);
  expect(puts).toEqual([
    {
      body: { releaseId: installation.releaseId, expectedRevision: 0, permissions: ["platform:directory:read", "platform:files:read"] },
      csrf: "console-csrf",
    },
  ]);
});

test("a stale approval is refused and the latest state is shown", async ({ page }) => {
  await open(page, { conflict: true, path: "/admin/apps/ledger/installations" });
  const card = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "테스트", exact: true }) });
  await card.getByRole("button", { name: "요청한 권한 모두 승인" }).click();
  await expect(card.getByRole("alert")).toHaveText("다른 관리자가 먼저 바꿨습니다. 최신 상태를 다시 불러왔습니다.");
});

test("the console speaks English when the saved language is English", async ({ page }) => {
  await open(page, { locale: "en-US", path: "/admin" });
  await expect(page.getByRole("heading", { name: "Platform administration" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Users", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Installations awaiting release permissions" })).toBeVisible();
  await page.getByRole("link", { name: "장부", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/apps\/ledger\/installations$/);
  await expect(page.getByText("2 awaiting approval")).toBeVisible();
});

test("sign in as a user opens the chosen portal in a new tab with a one-time link", async ({ page, context }) => {
  const { switches } = await open(page, { path: "/admin/users" });
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

test("an administrator sets and resets an installation's file storage limits", async ({ page }) => {
  const { quotas } = await open(page, { path: "/admin/apps/ledger/runtime" });
  const card = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "테스트", exact: true }) });
  await card.getByText("파일 저장소 한도").click();
  await expect(card.getByText("사용 1GiB / 20GiB, 파일 12 / 100000개")).toBeVisible();
  await card.getByLabel("최대 용량(GiB)").fill("5");
  await card.getByLabel("최대 파일 수").fill("500");
  await card.getByRole("button", { name: "한도 저장" }).click();
  await expect(card.getByText("사용 1GiB / 5GiB, 파일 12 / 500개")).toBeVisible();
  await card.getByRole("button", { name: "기본값으로" }).click();
  await expect(card.getByText("(기본값)")).toBeVisible();
  expect(quotas).toEqual([
    { method: "PUT", body: { maxBytes: 5 * 1024 * 1024 * 1024, maxFiles: 500 }, csrf: "console-csrf" },
    { method: "DELETE", body: null, csrf: "console-csrf" },
  ]);
});
