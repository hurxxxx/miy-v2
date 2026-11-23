import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";

// The app launcher ported from AI-DO's app bar: the rail's pinned apps, the
// all-apps menu by category, the pin editor (PATCH /api/v1/me/preferences) and
// the console's category editor (/api/v1/launcher/categories). The requests are
// mocked; Core validation and authorization have their own regression.
const base: Installation = {
  id: "40000000-0000-4000-8000-000000000101",
  appId: "ledger",
  name: "장부",
  releaseId: "50000000-0000-4000-8000-000000000101",
  environment: "dev",
  version: "1.0.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  canManage: false,
  ui: { entry: "/unused-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};
const installations: Installation[] = [
  base,
  { ...base, id: "40000000-0000-4000-8000-000000000102", appId: "docs", name: "문서 관리" },
  { ...base, id: "40000000-0000-4000-8000-000000000103", appId: "chat", name: "사내 메신저" },
  { ...base, id: "40000000-0000-4000-8000-000000000104", appId: "wiki", name: "위키" },
];
const updatedAt = "2026-11-22T00:00:00Z";
const categories = [
  // "ghost" is not installed here: the launcher shows only apps the person can open.
  { id: "c-work", title: "업무", iconKey: "briefcase", position: 0, appIds: ["ledger", "ghost", "docs"], updatedAt },
  { id: "c-talk", title: "소통", iconKey: "messages-square", position: 1, appIds: ["chat"], updatedAt },
];

type Sent = { method: string; path: string; body: unknown; csrf: string | null };

async function openPortal(page: Page, options: { pinnedApps?: string[]; locale?: string } = {}) {
  await mockSurface(page);
  const patches: Sent[] = [];
  let categoryReads = 0;
  let preferences = { ...defaultPreferences, locale: options.locale ?? "ko-KR", pinnedApps: options.pinnedApps ?? [] };
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === "/api/v1/me")
      return route.fulfill({
        json: browserUser(route, { id: "launcher-user", name: "런처 사용자", admin: false, csrf: "launcher-csrf", preferences }),
      });
    if (path === "/api/v1/installations") return route.fulfill({ json: installations });
    if (path === "/api/v1/launcher/categories" && request.method() === "GET") {
      categoryReads += 1;
      return route.fulfill({ json: { categories } });
    }
    if (path === "/api/v1/me/preferences" && request.method() === "PATCH") {
      const body = request.postDataJSON() as { pinnedApps: string[] };
      patches.push({ method: "PATCH", path, body, csrf: request.headers()["x-miy-csrf"] ?? null });
      preferences = { ...preferences, ...body };
      return route.fulfill({ json: { name: "런처 사용자", preferences } });
    }
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await page.goto("/environments/dev");
  await expect(page.locator(".shell-header")).toBeVisible();
  return { patches, categoryReads: () => categoryReads };
}

/** Accessible names of the rail's app links, Home first. */
async function railApps(page: Page) {
  const rail = page.locator("#platform-navigation .rail-apps a.rail-item");
  await expect(rail.first()).toBeVisible();
  return rail.evaluateAll((links) => links.map((link) => link.getAttribute("aria-label")));
}

test("with nothing pinned the rail lists every app the person can open", async ({ page }) => {
  const { categoryReads } = await openPortal(page);
  await expect.poll(() => railApps(page)).toEqual(["홈", "장부", "문서 관리", "사내 메신저", "위키"]);
  // Categories are read when the launcher opens, not on every page.
  expect(categoryReads()).toBe(0);
  const launcher = page.getByRole("button", { name: "앱 런처" });
  await expect(launcher).toHaveAttribute("aria-expanded", "false");
  await expect(launcher).toContainText("전체 앱");
});

test("pinned apps keep their order and pins of apps not open here are skipped", async ({ page }) => {
  await openPortal(page, { pinnedApps: ["wiki", "ghost", "ledger"] });
  await expect.poll(() => railApps(page)).toEqual(["홈", "위키", "장부"]);
  await page.locator("#platform-navigation").getByRole("link", { name: "장부", exact: true }).click();
  await expect(page).toHaveURL(/\/environments\/dev\/apps\/ledger$/);
  await expect(page.locator("#platform-navigation").getByRole("link", { name: "장부", exact: true })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("the launcher menu groups apps by category, keeps uncategorized apps, and closes on Escape or outside", async ({
  page,
}) => {
  const { categoryReads } = await openPortal(page, { pinnedApps: ["wiki"] });
  const launcher = page.getByRole("button", { name: "앱 런처" });
  await launcher.click();
  const menu = page.getByRole("menu", { name: "앱 런처" });
  await expect(menu).toBeVisible();
  await expect(launcher).toHaveAttribute("aria-expanded", "true");
  await expect(menu).toContainText("테스트 영역");
  const names = (group: string) =>
    menu
      .getByRole("group", { name: group, exact: true })
      .getByRole("menuitem")
      .evaluateAll((items) => items.map((item) => item.getAttribute("aria-label")));
  await expect.poll(() => names("업무")).toEqual(["장부 열기", "문서 관리 열기"]);
  await expect.poll(() => names("소통")).toEqual(["사내 메신저 열기"]);
  await expect.poll(() => names("기타 앱")).toEqual(["위키 열기"]);
  await expect(menu.getByRole("menuitem")).toHaveCount(4);
  expect(categoryReads()).toBe(1);

  // Search narrows across categories.
  await menu.getByRole("searchbox", { name: "앱 검색" }).fill("메신저");
  await expect(menu.getByRole("menuitem")).toHaveCount(1);
  await menu.getByRole("searchbox", { name: "앱 검색" }).fill("없는 앱");
  await expect(menu.getByText("검색 결과가 없습니다.")).toBeVisible();

  // Escape closes it and gives focus back to the launcher button.
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(launcher).toBeFocused();
  await expect(launcher).toHaveAttribute("aria-expanded", "false");

  // Keyboard: Enter opens, arrow keys move between the menu's controls.
  await page.keyboard.press("Enter");
  await expect(menu).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("searchbox", { name: "앱 검색" })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem", { name: "장부 열기" })).toBeFocused();

  // A press outside closes it.
  await page.mouse.click(1200, 700);
  await expect(menu).toHaveCount(0);

  // Choosing an app opens it in this tab and closes the menu.
  await launcher.click();
  await menu.getByRole("menuitem", { name: "사내 메신저 열기" }).click();
  await expect(page).toHaveURL(/\/environments\/dev\/apps\/chat$/);
  await expect(menu).toHaveCount(0);
});

test("the editor pins, reorders and unpins, then saves the exact list with the CSRF header", async ({ page }) => {
  // "prod-only" is pinned on the other portal: saving here must keep it.
  const { patches } = await openPortal(page, { pinnedApps: ["prod-only", "ledger", "docs"] });
  await expect.poll(() => railApps(page)).toEqual(["홈", "장부", "문서 관리"]);
  await page.getByRole("button", { name: "앱 런처" }).click();
  await page.getByRole("menu", { name: "앱 런처" }).getByRole("button", { name: "즐겨찾기 편집" }).click();
  const editor = page.getByRole("dialog", { name: "즐겨찾기 편집" });
  await expect(editor).toBeVisible();
  await expect(editor).toContainText("즐겨찾기 2개");
  await editor.getByRole("checkbox", { name: "사내 메신저 즐겨찾기에 추가" }).check();
  await expect(editor).toContainText("즐겨찾기 3개");
  await editor.getByRole("button", { name: "사내 메신저 위로 이동" }).click();
  await editor.getByRole("checkbox", { name: "문서 관리 즐겨찾기에 추가" }).first().uncheck();
  await expect(editor.getByRole("button", { name: "장부 위로 이동" })).toBeDisabled();
  await expect(editor.getByRole("button", { name: "사내 메신저 아래로 이동" })).toBeDisabled();
  await editor.getByRole("button", { name: "저장", exact: true }).click();
  await expect(editor).toHaveCount(0);
  expect(patches).toEqual([
    {
      method: "PATCH",
      path: "/api/v1/me/preferences",
      body: { pinnedApps: ["ledger", "chat", "prod-only"] },
      csrf: "launcher-csrf",
    },
  ]);
  await expect.poll(() => railApps(page)).toEqual(["홈", "장부", "사내 메신저"]);
  await expect(page.getByRole("button", { name: "앱 런처" })).toBeFocused();
});

test("reset returns to every app and saves an empty list; unpinning everything is refused", async ({ page }) => {
  const { patches } = await openPortal(page, { pinnedApps: ["wiki"] });
  await expect.poll(() => railApps(page)).toEqual(["홈", "위키"]);
  await page.getByRole("button", { name: "앱 런처" }).click();
  await page.getByRole("button", { name: "즐겨찾기 편집" }).click();
  const editor = page.getByRole("dialog", { name: "즐겨찾기 편집" });
  await editor.getByRole("checkbox", { name: "위키 즐겨찾기에 추가" }).first().uncheck();
  await editor.getByRole("button", { name: "저장", exact: true }).click();
  await expect(editor.getByRole("alert")).toHaveText("앱을 하나 이상 고정해야 합니다.");
  expect(patches).toEqual([]);
  await editor.getByRole("button", { name: "기본값" }).click();
  await expect(editor).toContainText("즐겨찾기 4개");
  await expect(editor.getByRole("alert")).toHaveCount(0);
  await editor.getByRole("button", { name: "저장", exact: true }).click();
  await expect(editor).toHaveCount(0);
  expect(patches).toEqual([
    { method: "PATCH", path: "/api/v1/me/preferences", body: { pinnedApps: [] }, csrf: "launcher-csrf" },
  ]);
  await expect.poll(() => railApps(page)).toEqual(["홈", "장부", "문서 관리", "사내 메신저", "위키"]);
});

test("the phone drawer shows the same rail and Escape closes the launcher before the drawer", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPortal(page, { pinnedApps: ["docs", "chat"] });
  await page.getByRole("button", { name: "메뉴 열기" }).click();
  const drawer = page.locator("#platform-navigation");
  await expect(drawer).toBeVisible();
  await expect.poll(() => railApps(page)).toEqual(["홈", "문서 관리", "사내 메신저"]);
  const launcher = drawer.getByRole("button", { name: "앱 런처" });
  await expect(launcher).toContainText("앱 런처");
  await launcher.click();
  const menu = page.getByRole("menu", { name: "앱 런처" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem")).toHaveCount(4);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(drawer).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(drawer).toBeHidden();
});

async function openConsole(page: Page, options: { locale?: string } = {}) {
  await mockSurface(page, "console");
  const sent: Sent[] = [];
  let state = [
    { id: "c-work", title: "업무", iconKey: "briefcase", position: 0, appIds: ["ledger"], updatedAt },
  ];
  let created = 0;
  const page_ = () => ({
    revision: `layout-${created}`,
    categories: state.map((category, position) => ({ ...category, position })),
  });
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();
    if (path === "/api/v1/me")
      return route.fulfill({
        json: browserUser(route, {
          id: "admin",
          name: "관리자",
          admin: true,
          csrf: "console-csrf",
          preferences: { ...defaultPreferences, locale: options.locale ?? "ko-KR" },
        }),
      });
    if (path.startsWith("/api/v1/launcher/categories")) {
      if (method === "GET") return route.fulfill({ json: page_() });
      const body = method === "DELETE" ? null : request.postDataJSON();
      sent.push({ method, path, body, csrf: request.headers()["x-miy-csrf"] ?? null });
      if (method === "POST") {
        if (body.title === "거절") return route.fulfill({ status: 400, json: { error: { code: "invalid_category_title" } } });
        created += 1;
        state = [...state, { id: `c-new-${created}`, title: body.title, iconKey: body.iconKey, position: 0, appIds: [], updatedAt }];
      } else if (method === "PUT") {
        state = body.categories.map((category: { id: string; title: string; iconKey: string; appIds: string[] }) => ({
          ...category,
          position: 0,
          updatedAt,
        }));
      } else if (method === "DELETE") {
        state = state.filter((category) => !path.endsWith(`/${category.id}`));
      }
      return route.fulfill({ json: page_() });
    }
    const reply: Record<string, unknown> = {
      "/api/v1/installations": [],
      "/api/v1/runtime/installations": [],
      "/api/v1/app-registrations": [],
      "/api/v1/account-links": { available: true, links: [] },
      "/api/v1/users": [{ id: "admin", name: "관리자", admin: true, canDevelop: false, enabled: true }],
      "/api/v1/groups": [],
      "/api/v1/apps": {
        apps: [
          { id: "ledger", name: "장부", disabled: false, distribution: "organization" },
          { id: "wiki", name: "위키", disabled: false, distribution: "organization" },
          { id: "chat", name: "사내 메신저", disabled: false, distribution: "organization" },
        ],
        nextCursor: null,
      },
      "/api/v1/audit": { entries: [], nextCursor: null },
      "/api/v1/support/reports": { entries: [], nextCursor: null },
    };
    if (path in reply) return route.fulfill({ json: reply[path] });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await page.goto("/admin/launcher");
  return { sent };
}

const categorySection = (page: Page, title = "플랫폼 앱바 카테고리") =>
  page.locator("section").filter({ has: page.getByRole("heading", { name: title, exact: true }) }).first();

test("the console category editor creates, renames, assigns, saves and deletes with the CSRF header", async ({ page }) => {
  const { sent } = await openConsole(page);
  const section = categorySection(page);
  await expect(page.getByRole("heading", { name: "앱바 구성", exact: true })).toBeVisible();
  await expect(section.getByRole("button", { name: /^업무/ })).toHaveAttribute("aria-pressed", "true");
  await expect(section.getByRole("button", { name: "장부 · 카테고리에서 제거" })).toBeVisible();

  // A server error code is shown in the launcher's words.
  await section.getByLabel("새 카테고리").fill("거절");
  await section.getByRole("button", { name: "카테고리 추가" }).click();
  await expect(page.getByText("카테고리 이름은 1~120자로 입력해 주세요.", { exact: true })).toBeVisible();

  await section.getByLabel("새 카테고리").fill("현장 업무");
  await section.getByRole("button", { name: "카테고리 추가" }).click();
  await expect(page.getByText("앱바 카테고리를 추가했습니다.", { exact: true })).toBeVisible();
  await expect(section.getByRole("button", { name: /^현장 업무/ })).toHaveAttribute("aria-pressed", "true");

  // Rename, choose an icon, add an unassigned app and move one from another category.
  await section.getByRole("textbox", { name: "카테고리 이름" }).fill("현장 지원");
  await section.getByRole("button", { name: "아이콘", exact: true }).last().click();
  const picker = page.getByRole("dialog", { name: "현장 지원 아이콘 선택" });
  await picker.getByRole("searchbox", { name: "아이콘 검색" }).fill("rocket");
  await picker.getByRole("radio", { name: "rocket", exact: true }).click();
  await expect(picker).toHaveCount(0);
  await section.getByRole("button", { name: "위키 · 이 카테고리에 추가" }).click();
  await section.getByRole("button", { name: "장부 · 이 카테고리로 이동" }).click();
  // The new category goes first.
  await section.getByRole("button", { name: "위로 이동", exact: true }).nth(1).click();
  await page.getByRole("button", { name: "구성 저장" }).click();
  await expect(page.getByText("앱바 카테고리 구성을 저장했습니다.", { exact: true })).toBeVisible();

  // Delete the emptied category after confirming.
  await section.getByRole("button", { name: /^업무/ }).click();
  page.once("dialog", (dialog) => {
    expect(dialog.message()).toContain("업무 카테고리를 삭제할까요?");
    void dialog.accept();
  });
  await section.getByRole("button", { name: "삭제", exact: true }).click();
  await expect(page.getByText("앱바 카테고리를 삭제했습니다.", { exact: true })).toBeVisible();
  await expect(section.getByRole("button", { name: /^업무/ })).toHaveCount(0);

  expect(sent).toEqual([
    {
      method: "POST",
      path: "/api/v1/launcher/categories",
      body: { iconKey: "layout", title: "거절" },
      csrf: "console-csrf",
    },
    {
      method: "POST",
      path: "/api/v1/launcher/categories",
      body: { iconKey: "layout", title: "현장 업무" },
      csrf: "console-csrf",
    },
    {
      method: "PUT",
      path: "/api/v1/launcher/categories/layout",
      body: {
        expectedRevision: "layout-1",
        categories: [
          { appIds: ["wiki", "ledger"], iconKey: "rocket", id: "c-new-1", title: "현장 지원" },
          { appIds: [], iconKey: "briefcase", id: "c-work", title: "업무" },
        ],
      },
      csrf: "console-csrf",
    },
    { method: "DELETE", path: "/api/v1/launcher/categories/c-work", body: null, csrf: "console-csrf" },
  ]);
});

test("the launcher and the category editor speak English when the saved language is English", async ({ page }) => {
  await openPortal(page, { locale: "en-US", pinnedApps: ["wiki"] });
  const launcher = page.getByRole("button", { name: "App launcher" });
  await expect(launcher).toContainText("All apps");
  await launcher.click();
  const menu = page.getByRole("menu", { name: "App launcher" });
  await expect(menu.getByRole("group", { name: "Other apps", exact: true })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "Open 위키" })).toBeVisible();
  await menu.getByRole("button", { name: "Edit favorites" }).click();
  const editor = page.getByRole("dialog", { name: "Edit favorites" });
  await expect(editor).toContainText("1 favorites");
  await expect(editor.getByRole("button", { name: "Default" })).toBeVisible();
  await expect(editor.getByRole("checkbox", { name: "Add 장부 to favorites" })).toBeVisible();

  await page.unrouteAll({ behavior: "ignoreErrors" });
  await openConsole(page, { locale: "en-US" });
  await expect(page.getByRole("heading", { name: "App bar layout", exact: true })).toBeVisible();
  const section = categorySection(page, "Platform app bar categories");
  await expect(section.getByRole("button", { name: "Save layout" })).toBeVisible();
  await expect(section.getByRole("button", { name: "Add category" })).toBeVisible();
  await expect(section.getByText("1 apps")).toBeVisible();
});
