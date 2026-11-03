import { test, expect, type Page } from "@playwright/test";
import { randomUUID } from "node:crypto";

// UI-5: the new shell against the real core server (current source) and a
// disposable database, using the explicit loopback development login.
// Not covered here: OIDC/Keycloak login, deployed Docker apps and remote app UIs.

// Development login returns to the platform home; then open the target page.
async function signIn(page: Page, label: RegExp, path?: string) {
  await page.goto("/");
  await page.getByRole("button", { name: label }).click();
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
  if (path) await page.goto(path);
}

test("anonymous visitors see the login screen backed by server options", async ({ page }) => {
  await page.goto("/environments/dev");
  await expect(page.getByRole("heading", { name: "로그인" })).toBeVisible();
  for (const label of ["관리자로 시작", "개발자", "사용자 A", "사용자 B"])
    await expect(page.getByRole("button", { name: new RegExp(label) })).toBeVisible();
  expect((await page.request.get("/api/v1/me")).status()).toBe(401);
});

test("admin signs in, sees an empty launcher, real directory data and survives reload", async ({ page }) => {
  await signIn(page, /관리자로 시작/);
  await expect(page.locator("header")).toContainText("admin");
  await expect(page.getByText("이 실행 환경에서 사용할 수 있는 앱이 없습니다.")).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  const rail = page.locator("#platform-navigation");
  await rail.getByRole("link", { name: "플랫폼 관리", exact: true }).click();
  await expect(page.getByRole("heading", { name: "플랫폼 관리" })).toBeVisible();
  await expect(page.getByRole("combobox", { name: "사용자", exact: true })).toContainText("alice");
  await expect(page.getByText("대기 중인 신청이 없습니다.")).toBeVisible();
  await expect(page.getByText("배포된 앱이 없습니다.")).toBeVisible();
  await page.reload();
  await expect(page.getByRole("heading", { name: "플랫폼 관리" })).toBeVisible();
  await page.getByRole("link", { name: "운영 영역", exact: true }).click();
  await expect(page).toHaveURL(/\/environments\/prod$/);
  await expect(page.getByRole("heading", { name: "운영 앱" })).toBeVisible();
});

test("admin grant form sends the session CSRF and shows the server's answer", async ({ page }) => {
  await signIn(page, /관리자로 시작/, "/admin");
  await page.getByRole("combobox", { name: "사용자", exact: true }).selectOption({ label: "alice" });
  await page.getByLabel("앱 ID").fill("missing-app");
  const [response] = await Promise.all([
    page.waitForResponse((r) => r.url().endsWith("/api/v1/grants") && r.request().method() === "PUT"),
    page.getByRole("button", { name: "권한 적용" }).click(),
  ]);
  expect(response.request().headers()["x-miy-csrf"]).toBeTruthy();
  // Unknown app: the server, not the UI, decides; the answer is shown either way.
  await expect(page.locator("p[role=status]")).toHaveText(response.ok() ? "권한을 변경했습니다." : /Error: /);
  test.info().annotations.push({ type: "grant-status", description: String(response.status()) });
});

test("a regular user gets only what the server allows", async ({ page }) => {
  await signIn(page, /사용자 A/);
  await expect(page.locator("#platform-navigation").getByRole("link", { name: "플랫폼 관리" })).toHaveCount(0);
  await page.goto("/admin");
  await expect(page.getByRole("heading", { name: "관리자 권한이 필요합니다" })).toBeVisible();
  expect((await page.request.get("/api/v1/users")).status()).toBe(403);
  await page.goto("/environments/dev/apps/missing-app");
  await expect(page.getByRole("heading", { name: "앱에 접근할 수 없습니다" })).toBeVisible();
  await page.goto("/environments/stage");
  await expect(page.getByText("존재하지 않는 실행 환경입니다")).toBeVisible();
});

test("CLI device approval completes end to end through the new shell", async ({ page, playwright, baseURL }) => {
  const client = await playwright.request.newContext({ baseURL });
  const started = await client.post("/auth/cli/device", {
    data: { clientId: randomUUID(), clientName: "Server UI regression" },
  });
  expect(started.status()).toBe(200);
  const device = await started.json();
  await page.goto(device.verificationPath);
  await page.getByRole("button", { name: /사용자 A/ }).click();
  await expect(page).toHaveURL(new RegExp(`${device.verificationPath}$`));
  await expect(page.getByRole("heading", { name: "CLI 로그인 승인" })).toBeVisible();
  await expect(page.locator("main").getByText("alice", { exact: true })).toBeVisible();
  const approve = page.getByRole("button", { name: "이 계정으로 CLI 연결 승인" });
  await expect(approve).toBeDisabled();
  await page.getByLabel("인증 코드").fill(device.userCode);
  await approve.click();
  await expect(page.getByRole("status")).toContainText("CLI 연결을 승인했습니다");
  const exchange = await client.post("/auth/cli/token", {
    data: { grantType: "device_code", id: device.id, deviceCode: device.deviceCode },
  });
  expect(exchange.status()).toBe(200);
  expect((await exchange.json()).status).toBe("approved");
  await client.dispose();
});

test("logout ends the server session and returns to the login screen", async ({ page }) => {
  await signIn(page, /개발자/);
  await Promise.all([
    page.waitForResponse((r) => r.url().endsWith("/auth/logout") && r.status() === 200),
    page.getByRole("button", { name: "로그아웃" }).click(),
  ]);
  // The shell follows the server's redirect itself.
  await expect(page.getByRole("heading", { name: "로그인" })).toBeVisible();
  expect((await page.request.get("/api/v1/me")).status()).toBe(401);
});

test("signed-in CLI install page reads the server guide", async ({ page }) => {
  await signIn(page, /사용자 B/);
  await page.locator("#platform-navigation").getByRole("link", { name: "CLI 설치", exact: true }).click();
  await expect(page.getByRole("heading", { name: "CLI 설치", exact: true })).toBeVisible();
  await expect(page.getByTestId("cli-install-server")).toHaveText("http://127.0.0.1:18148");
});
