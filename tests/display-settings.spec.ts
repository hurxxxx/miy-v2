import { browserUser, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";
import { mockFrames, type FrameMockOptions } from "./frame-mock";

// Display settings (theme, language, time zone, date format) reach the portal
// document and the app frame. Presentation only: no evidence for server checks.
test.beforeEach(async ({ page }) => { await mockSurface(page); });

const user = { id: "fixture", name: "설정 검증 사용자", admin: false, csrf: "fixture" };
const app: Installation = {
  id: "40000000-0000-4000-8000-000000000021",
  appId: "settings-app",
  name: "설정 검증 앱",
  releaseId: "50000000-0000-4000-8000-000000000021",
  environment: "dev",
  version: "1.0.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  canManage: false,
  ui: { entry: "/artifacts/health-fixture/mf-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};

async function open(page: Page, stored: object | undefined, options: FrameMockOptions = {}) {
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/me") return route.fulfill({ json: browserUser(route, user) });
    if (path === "/api/v1/installations") return route.fulfill({ json: [app] });
    await route.fulfill({ json: { title: "저장된 제목" } });
  });
  await mockFrames(page, () => [app], () => ({ json: { title: "저장된 제목" } }), options);
  if (stored) await page.addInitScript((value) => window.localStorage.setItem("miy:display-settings", JSON.stringify(value)), stored);
  await page.goto("/environments/dev/apps/settings-app");
  await expect(page.frameLocator("iframe.app-frame").getByLabel("문서 제목")).toHaveValue("저장된 제목");
  return page.frames().find((value) => value.url().endsWith("/_miy/launch"))!;
}
const marks = () => ({
  dark: document.documentElement.classList.contains("dark"),
  theme: document.documentElement.dataset.miyTheme,
  lang: document.documentElement.lang,
});

test("a stored dark English choice reaches the portal document and the app frame", async ({ page }) => {
  const frame = await open(page, { theme: "dark", locale: "en-US", timeZone: "America/New_York", dateFormat: "iso" });
  expect(await page.evaluate(marks)).toEqual({ dark: true, theme: "dark", lang: "en-US" });
  await expect.poll(() => frame.evaluate(marks)).toEqual({ dark: true, theme: "dark", lang: "en-US" });
  // The frame takes its colors from the tokens the platform serves on its origin.
  const background = await frame.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(background).toBe("rgb(30, 31, 33)");
});

test("without a stored choice the portal and the frame follow the browser theme", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  const frame = await open(page, undefined);
  expect(await page.evaluate(marks)).toEqual({ dark: true, theme: "dark", lang: "ko-KR" });
  await expect.poll(() => frame.evaluate(marks)).toEqual({ dark: true, theme: "dark", lang: "ko-KR" });
  await page.emulateMedia({ colorScheme: "light" });
  await expect.poll(() => page.evaluate(marks)).toEqual({ dark: false, theme: "light", lang: "ko-KR" });
  await expect.poll(() => frame.evaluate(marks)).toEqual({ dark: false, theme: "light", lang: "ko-KR" });
});

test("an app built before display settings stays light and Korean", async ({ page }) => {
  const frame = await open(page, { theme: "dark", locale: "en-US" }, { settingsAware: false });
  expect(await page.evaluate(marks)).toEqual({ dark: true, theme: "dark", lang: "en-US" });
  expect(await frame.evaluate(marks)).toEqual({ dark: false, theme: undefined, lang: "ko" });
  const background = await frame.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(background).toBe("rgb(255, 255, 255)");
});

test("the frame document uses the platform font and tokens, not the portal's styles", async ({ page }) => {
  const frame = await open(page, { theme: "light" });
  const style = await frame.evaluate(() => {
    const body = getComputedStyle(document.body);
    return { font: body.fontFamily, size: body.fontSize, color: body.color };
  });
  expect(style.font).toContain("Pretendard Variable");
  expect(style).toMatchObject({ size: "14px", color: "rgb(17, 24, 39)" });
});
