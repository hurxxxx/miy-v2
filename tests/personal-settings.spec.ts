import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page, type Route } from "@playwright/test";

// The personal settings page ported from AI-DO: what the portal sends to
// PATCH /api/v1/me/preferences and /api/v1/me/sessions, and how the page and
// the shell follow the saved choice. The Core regression covers the server.
test.beforeEach(async ({ page }) => { await mockSurface(page); });

type Saved = Record<string, unknown>;
const now = Date.now();
const iso = (offsetMinutes: number) => new Date(now + offsetMinutes * 60000).toISOString();

async function open(page: Page, path: string, options: { fail?: (body: Saved) => number | undefined } = {}) {
  const patches: Saved[] = [];
  const deletes: { path: string; csrf: string | null }[] = [];
  let preferences: Saved = { ...defaultPreferences };
  let name = "설정 사용자";
  let sessions = [
    { id: "s-current", surface: "portal", createdAt: iso(-60), expiresAt: iso(420), lastSeenAt: iso(0), userAgent: "Firefox on Linux", current: true },
    { id: "s-console", surface: "console", createdAt: iso(-30), expiresAt: iso(450), lastSeenAt: iso(-5), userAgent: "Chrome on Windows", current: false },
  ];
  await page.route("**/api/**", async (route: Route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === "/api/v1/me")
      return route.fulfill({ json: browserUser(route, { id: "settings-user", name, admin: false, csrf: "settings-csrf", preferences }) });
    if (path === "/api/v1/installations") return route.fulfill({ json: [] });
    if (path === "/api/v1/me/preferences" && request.method() === "PATCH") {
      const body = request.postDataJSON() as Saved;
      patches.push(body);
      const status = options.fail?.(body);
      if (status) return route.fulfill({ status, json: { error: { code: status === 400 ? "invalid_user_name" : "internal" } } });
      if (typeof body.name === "string") name = body.name;
      const { name: _ignored, ...rest } = body;
      preferences = { ...preferences, ...rest, jobTitle: body.jobTitle === "" ? null : (body.jobTitle ?? preferences.jobTitle) };
      return route.fulfill({ json: { name, preferences } });
    }
    if (path === "/api/v1/me/sessions") return route.fulfill({ json: { items: sessions } });
    if (path.startsWith("/api/v1/me/sessions/") && request.method() === "DELETE") {
      deletes.push({ path, csrf: request.headers()["x-miy-csrf"] ?? null });
      sessions = sessions.filter((session) => !path.endsWith(session.id));
      return route.fulfill({ status: 204, body: "" });
    }
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  await page.goto(path);
  return { patches, deletes };
}
const dark = (page: Page) => page.evaluate(() => document.documentElement.classList.contains("dark"));

test("appearance changes apply at once, are saved, and switch the portal to English", async ({ page }) => {
  const { patches } = await open(page, "/settings/appearance");
  await expect(page.getByRole("heading", { name: "내 설정" })).toBeVisible();
  await page.getByRole("button", { name: "다크" }).click();
  await expect.poll(() => dark(page)).toBe(true);
  await page.getByLabel("언어").selectOption("en-US");
  await expect(page.getByRole("heading", { name: "My settings" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign out" }).first()).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.lang)).toBe("en-US");
  await page.getByLabel("Time zone").selectOption("Europe/Berlin");
  await page.getByLabel("Date format").selectOption("iso");
  expect(patches).toEqual([{ theme: "dark" }, { locale: "en-US" }, { timeZone: "Europe/Berlin" }, { dateFormat: "iso" }]);
  // The saved choice outlives this page: the next load paints with it.
  await page.reload();
  await expect(page.getByRole("heading", { name: "My settings" })).toBeVisible();
  expect(await dark(page)).toBe(true);
});

test("a failed save rolls the display change back", async ({ page }) => {
  const { patches } = await open(page, "/settings/appearance", { fail: (body) => (body.theme ? 500 : undefined) });
  await page.getByRole("button", { name: "다크" }).click();
  // Toasts are announced through a second, visually hidden copy.
  await expect(page.getByText("저장하지 못했습니다.").first()).toBeVisible();
  await expect.poll(() => dark(page)).toBe(false);
  await expect(page.getByRole("button", { name: "시스템" })).toHaveAttribute("aria-pressed", "true");
  expect(patches).toEqual([{ theme: "dark" }]);
});

test("profile saves the display name and job title, and shows a server error code translated", async ({ page }) => {
  const { patches } = await open(page, "/settings/profile", { fail: (body) => (body.name === "x".repeat(3) ? 400 : undefined) });
  await page.getByLabel("표시 이름").fill("새 이름");
  await page.getByLabel("직무").fill("책임");
  await page.getByRole("button", { name: "변경사항 저장" }).click();
  await expect(page.getByText("변경사항을 저장했습니다.").first()).toBeVisible();
  await expect(page.locator(".account-name")).toHaveText("새 이름");
  await page.getByLabel("표시 이름").fill("xxx");
  await page.getByRole("button", { name: "변경사항 저장" }).click();
  await expect(page.getByText("이름은 1~120자로 입력해 주세요.")).toBeVisible();
  expect(patches).toEqual([{ name: "새 이름", jobTitle: "책임" }, { name: "xxx", jobTitle: "책임" }]);
});

test("security lists own sessions with this one first and ends another with the CSRF header", async ({ page }) => {
  const { deletes } = await open(page, "/settings/security");
  const cards = page.locator("text=현재 세션");
  await expect(cards).toBeVisible();
  await expect(page.getByText("Chrome on Windows")).toBeVisible();
  // Only the other session offers an end button.
  await expect(page.getByRole("button", { name: "종료" })).toHaveCount(1);
  await page.getByRole("button", { name: "종료" }).click();
  await expect(page.getByText("세션을 종료했습니다.").first()).toBeVisible();
  await expect(page.getByText("Chrome on Windows")).toHaveCount(0);
  expect(deletes).toEqual([{ path: "/api/v1/me/sessions/s-console", csrf: "settings-csrf" }]);
  await expect(page.getByRole("button", { name: "비밀번호 변경" })).toBeVisible();
});

test("the shell links the account name to the settings page", async ({ page }) => {
  await open(page, "/");
  await page.locator(".account-name").click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole("heading", { name: "프로필" })).toBeVisible();
});
