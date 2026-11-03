import { test, expect, type Page } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";

// UI-1: the shell distinguishes loading, empty and failed reads with mocked APIs.
// These checks cover presentation only; they are not evidence for server auth.
const user = { id: "fixture", name: "셸 검증 사용자", admin: false, csrf: "fixture" };
const app: Installation = {
  id: "40000000-0000-4000-8000-000000000001",
  appId: "shell-app",
  name: "셸 검증 앱",
  releaseId: "50000000-0000-4000-8000-000000000001",
  environment: "dev",
  version: "1.0.0",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  kind: "docker_app",
  canManage: false,
  ui: { entry: "/artifacts/health-fixture/mf-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
};

type Reply = { status: number; json: unknown } | "abort";
async function mock(page: Page, handlers: { me: () => Reply; installations: () => Reply }) {
  await page.route("**/api/**", async (route) => {
    expect(route.request().method()).toBe("GET");
    const path = new URL(route.request().url()).pathname;
    const reply =
      path === "/api/v1/me"
        ? handlers.me()
        : path === "/api/v1/installations"
          ? handlers.installations()
          : { status: 200, json: { title: "저장된 제목" } };
    if (reply === "abort") return route.abort("connectionrefused");
    await route.fulfill(reply);
  });
}

test("a failed session read is shown as a connection error, not as signed out", async ({ page }) => {
  let me: Reply = { status: 503, json: { error: { code: "unavailable" } } };
  await mock(page, { me: () => me, installations: () => ({ status: 200, json: [app] }) });
  await page.goto("/environments/dev");
  await expect(page.getByRole("alert")).toContainText("서버에 연결하지 못했습니다");
  await expect(page.getByRole("link", { name: "로그인", exact: true })).toHaveCount(0);
  me = { status: 200, json: user };
  await page.getByRole("button", { name: "다시 시도" }).click();
  await expect(page.locator("header")).toContainText("셸 검증 사용자");
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("401 alone shows the login screen", async ({ page }) => {
  await mock(page, { me: () => ({ status: 401, json: {} }), installations: () => ({ status: 401, json: {} }) });
  await page.goto("/environments/dev");
  await expect(page.getByRole("link", { name: "로그인", exact: true })).toBeVisible();
});

test("installation read failure is distinct from an empty list and can be retried", async ({ page }) => {
  let installations: Reply = "abort";
  await mock(page, { me: () => ({ status: 200, json: user }), installations: () => installations });
  await page.goto("/environments/dev");
  await expect(page.getByRole("alert")).toContainText("앱 목록을 불러오지 못했습니다");
  await expect(page.getByText("사용할 수 있는 앱이 없습니다")).toHaveCount(0);
  installations = { status: 200, json: [] };
  await page.getByRole("button", { name: "다시 시도" }).click();
  await expect(page.getByText("이 실행 환경에서 사용할 수 있는 앱이 없습니다")).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "다시 시도" })).toHaveCount(0);
  installations = { status: 200, json: [app] };
  await page.getByRole("link", { name: "홈", exact: true }).click();
  await expect(page.getByRole("link", { name: "셸 검증 앱", exact: true })).toBeVisible();
  installations = { status: 500, json: { error: { code: "internal_error" } } };
  await expect(page.getByRole("alert")).toContainText("최신 앱 상태를 확인하지 못했습니다");
  await expect(page.getByRole("link", { name: "셸 검증 앱", exact: true })).toBeVisible();
});

test("app screen read failure is not reported as missing permission", async ({ page }) => {
  let failApp = true;
  await mock(page, {
    me: () => ({ status: 200, json: user }),
    installations: () => (failApp ? { status: 500, json: { error: { code: "internal_error" } } } : { status: 200, json: [app] }),
  });
  await page.goto("/environments/dev/apps/shell-app");
  await expect(page.getByRole("heading", { name: "앱 정보를 불러오지 못했습니다" })).toBeVisible();
  await expect(page.getByText("접근 권한이 없습니다")).toHaveCount(0);
});

test("remote apps mount inside a dedicated container, not as shell main content", async ({ page }) => {
  const { resolve, sep } = await import("node:path");
  const remote = resolve(".local/evidence/runtime-health-ui/remote");
  await page.route("**/artifacts/health-fixture/**", async (route) => {
    const path = resolve(remote, new URL(route.request().url()).pathname.slice("/artifacts/health-fixture/".length));
    expect(path.startsWith(remote + sep)).toBe(true);
    await route.fulfill({ path });
  });
  await mock(page, { me: () => ({ status: 200, json: user }), installations: () => ({ status: 200, json: [app] }) });
  await page.goto("/environments/dev/apps/shell-app");
  await expect(page.getByLabel("문서 제목")).toHaveValue("저장된 제목");
  const container = page.locator('.workspace > main > .miy-app-root[data-miy-app="shell-app"]');
  await expect(container).toHaveCount(1);
  expect(await container.evaluate((node) => node.childElementCount)).toBeGreaterThan(0);
  // Directly inside main: the app container and the shell's session bar only.
  expect(
    await page.locator(".workspace > main > *").evaluateAll((nodes) =>
      nodes.filter((node) => !node.classList.contains("miy-app-root") && !node.classList.contains("app-session-bar")).length,
    ),
  ).toBe(0);
});

test("runtime confirmation is a keyboard modal: Escape cancels and focus returns", async ({ page }) => {
  const prod = { ...app, environment: "prod" as const, canManage: true };
  const writes: string[] = [];
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() !== "GET") writes.push(path);
    if (path === "/api/v1/me") return route.fulfill({ json: { ...user, admin: true } });
    if (path === "/api/v1/installations") return route.fulfill({ json: [prod] });
    if (path.endsWith("/runtime/preflight")) return route.fulfill({ json: { canSubmit: true, expectedState: {}, changes: [] } });
    return route.fulfill({ status: 500, json: { error: { code: "unexpected" } } });
  });
  await page.goto("/environments/prod");
  const stop = page.getByRole("button", { name: "중지", exact: true });
  await stop.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "취소" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(stop).toBeFocused();
  expect(writes).toEqual(["/api/v1/apps/shell-app/runtime/preflight"]);
});

test("rail shows unique monograms with accessible names and marks the current app", async ({ page }) => {
  const apps: Installation[] = [
    { ...app, id: "a1", appId: "docs", name: "문서 관리" },
    { ...app, id: "a2", appId: "inquiry", name: "문의 게시판" },
    { ...app, id: "a3", appId: "long", name: "아주 긴 이름을 가진 업무 앱 이름 표시 검증" },
  ];
  await mock(page, { me: () => ({ status: 200, json: user }), installations: () => ({ status: 200, json: apps }) });
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/environments/dev");
  const rail = page.locator("#platform-navigation");
  await expect(rail.getByRole("link", { name: "문서 관리", exact: true })).toContainText("문서");
  await expect(rail.getByRole("link", { name: "문의 게시판", exact: true })).toContainText("문의");
  await rail.getByRole("link", { name: "문의 게시판", exact: true }).hover();
  await expect(page.locator(".rail-tooltip")).toHaveText("문의 게시판");
  await rail.getByRole("link", { name: "문서 관리", exact: true }).click();
  await expect(rail.getByRole("link", { name: "문서 관리", exact: true })).toHaveAttribute("aria-current", "page");
  await expect(page.locator("header")).toContainText("문서 관리");
  for (const width of [1440, 768, 390]) {
    await page.setViewportSize({ width, height: 900 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
});

test("mobile drawer closes on Escape and returns focus to the toggle", async ({ page }) => {
  await mock(page, { me: () => ({ status: 200, json: user }), installations: () => ({ status: 200, json: [app] }) });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/environments/dev");
  await expect(page.locator("header")).toContainText("테스트 · DEV");
  const toggle = page.getByRole("button", { name: "메뉴 열기" });
  await toggle.click();
  await expect(page.locator("#platform-navigation")).toBeVisible();
  await expect(page.locator("#platform-navigation").getByRole("link", { name: "셸 검증 앱", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#platform-navigation")).toBeHidden();
  await expect(page.getByRole("button", { name: "메뉴 열기" })).toBeFocused();
});
