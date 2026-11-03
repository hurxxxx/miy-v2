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
  // The only element directly inside main from the app session is the container, plus shell status.
  expect(
    await page.locator(".workspace > main > *").evaluateAll((nodes) =>
      nodes.filter((node) => !node.classList.contains("miy-app-root") && node.getAttribute("role") !== "status").length,
    ),
  ).toBe(0);
});
