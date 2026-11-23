import { test, expect, type Page } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { openNewTab } from "./new-tab";

// UI-5: the new shell against the real core server (current source) and a
// disposable database, using the explicit loopback development login.
// Not covered here: OIDC/Keycloak login, deployed Docker apps and remote app UIs.

// Development login returns to the platform home; then open the target page.
type Surface = "portal" | "dev_portal" | "console";
async function serverOrigins(page: Page) {
  return (await (await page.request.get("/auth/options")).json()) as Record<"portalOrigin" | "devPortalOrigin" | "consoleOrigin", string>;
}
async function signIn(page: Page, label: RegExp, path?: string, surface: Surface = "portal") {
  const origins = await serverOrigins(page);
  const origin = { portal: origins.portalOrigin, dev_portal: origins.devPortalOrigin, console: origins.consoleOrigin }[surface];
  await page.goto(origin + "/");
  await page.getByRole("button", { name: label }).click();
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
  if (path) await page.goto(origin + path);
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
  const consolePage = await openNewTab(page, rail.getByRole("link", { name: "관리 콘솔", exact: true }));
  await consolePage.getByRole("button", { name: /관리자로 시작/ }).click();
  await expect(consolePage.getByRole("heading", { name: "내가 관리하는 앱", exact: true })).toBeVisible();
  await consolePage.locator("#platform-navigation").getByRole("link", { name: "플랫폼 관리", exact: true }).click();
  await expect(consolePage.getByRole("heading", { name: "플랫폼 관리" })).toBeVisible();
  const adminNav = consolePage.getByRole("navigation", { name: "플랫폼 관리" });
  await expect(consolePage.getByRole("heading", { name: "앱 등록 승인 대기" })).toBeVisible();
  await expect(consolePage.getByRole("heading", { name: "릴리스 권한 승인 대기" })).toBeVisible();
  await adminNav.getByRole("link", { name: "사용자", exact: true }).click();
  await expect(consolePage).toHaveURL(/\/admin\/users$/);
  await expect(consolePage.getByText("alice", { exact: true })).toBeVisible();
  await consolePage.reload();
  await expect(consolePage.getByText("alice", { exact: true })).toBeVisible();
  await adminNav.getByRole("link", { name: "개요", exact: true }).click();
  await consolePage.reload();
  await expect(consolePage.getByRole("heading", { name: "플랫폼 관리" })).toBeVisible();
  await consolePage.getByRole("link", { name: "운영 영역", exact: true }).click();
  await expect(consolePage).toHaveURL(/\/environments\/prod$/);
  await expect(consolePage.getByRole("heading", { name: "내가 관리하는 앱", exact: true })).toBeVisible();
  const portalPage = await openNewTab(consolePage, consolePage.getByRole("link", { name: "업무 포털", exact: true }));
  await expect(portalPage.getByRole("heading", { name: "운영 앱", exact: true })).toBeVisible();
  await portalPage.close();
  await consolePage.close();
});

test("organization changes appear in the linked user directory and audit", async ({ page }) => {
  const { consoleOrigin } = await serverOrigins(page);
  const address = `${consoleOrigin}/admin/users?q=alice`;
  const listingReads: string[] = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/api/v1/runtime/installations") listingReads.push(request.url());
  });
  await page.goto(address);
  await page.getByRole("button", { name: /관리자로 시작/ }).click();
  await expect(page).toHaveURL(address);
  await expect(page.locator(".user-account")).toHaveCount(1);
  await expect(page.locator(".user-account")).toContainText("alice");
  await page.getByRole("link", { name: "조직도", exact: true }).click();
  const name = "브라우저 검증 조직";
  await page.getByRole("button", { name: "조직 만들기" }).click();
  await page.getByLabel("이름", { exact: true }).fill(name);
  await page.getByRole("button", { name: "만들기", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/organization\?group=[0-9a-f-]+$/);
  const detail = page.getByRole("region", { name: `${name} 구성원` });
  await detail.getByRole("combobox", { name: "주 소속으로 지정할 사용자" }).fill("alice");
  await detail.getByRole("option", { name: /^alice \(/ }).click();
  await detail.getByRole("button", { name: "주 소속 지정" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "주 소속 지정" }).click();
  await expect(detail.getByRole("listitem")).toContainText("alice");
  await page.getByRole("link", { name: "이 조직의 사용자 보기" }).click();
  await expect(page).toHaveURL(/\/admin\/users\?organization=[0-9a-f-]+$/);
  await expect(page.locator(".user-account")).toHaveCount(1);
  await expect(page.locator(".user-account")).toContainText("alice");
  await page.getByRole("link", { name: "감사", exact: true }).click();
  await expect(page.getByText("주 소속 지정", { exact: true })).toBeVisible();
  expect(listingReads).toHaveLength(0);
});

test("the overview opens the registration queue and approval opens app access", async ({ page }) => {
  await signIn(page, /관리자로 시작/, "/admin", "console");
  const { consoleOrigin } = await serverOrigins(page);
  const { csrf } = await (await page.request.get(consoleOrigin + "/api/v1/me")).json();
  const registration = await page.request.post(consoleOrigin + "/api/v1/app-registrations", {
    data: { id: "console-regression-app", name: "등록 승인 검증 앱" },
    headers: { origin: consoleOrigin, "x-miy-csrf": csrf },
  });
  expect(registration.ok()).toBeTruthy();
  await page.reload();
  await expect(page.getByRole("heading", { name: "앱 등록 승인 대기" })).toBeVisible();
  await page.getByRole("link", { name: "등록 신청 보기" }).click();
  await expect(page).toHaveURL(/\/admin\/apps#registration-requests$/);
  const pending = page.locator(".registration-row").filter({ hasText: "등록 승인 검증 앱" });
  await pending.getByRole("button", { name: "승인", exact: true }).click();
  await expect(pending).toHaveCount(0);
  await page.getByRole("link", { name: "등록 승인 검증 앱", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/apps\/console-regression-app$/);
  await expect(page.getByRole("heading", { name: "앱 접근", exact: true })).toBeVisible();
});

test("admin grant form sends the session CSRF and shows the server's answer", async ({ page }) => {
  await signIn(page, /관리자로 시작/, "/admin/apps", "console");
  await page.getByText("앱 등록 전 개인 권한", { exact: true }).click();
  const user = page.getByRole("combobox", { name: "사용자", exact: true });
  const aliceId = await user.getByRole("option", { name: /^alice \(/ }).getAttribute("value");
  await user.selectOption({ value: aliceId! });
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
  await page.goto("/admin/apps/absent/access");
  await expect(page).toHaveURL((await serverOrigins(page)).consoleOrigin + "/admin/apps/absent/access");
  await page.getByRole("button", { name: /사용자 A/ }).click();
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
  await page.goto((await serverOrigins(page)).consoleOrigin + "/admin");
  await expect(page.getByRole("heading", { name: "관리자 권한이 필요합니다" })).toBeVisible();
  const origins = await serverOrigins(page);
  for (const path of ["/api/v1/users", "/api/v1/users/directory", "/api/v1/admin/overview"]) {
    expect((await page.request.get(origins.consoleOrigin + path)).status()).toBe(403);
  }
  // Dev apps live on the dev portal only: the prod portal moves the path there,
  // and the dev portal has its own sign-in.
  await page.goto("/environments/dev/apps/missing-app");
  await expect(page).toHaveURL((await serverOrigins(page)).devPortalOrigin + "/environments/dev/apps/missing-app");
  await signIn(page, /사용자 A/, "/environments/dev/apps/missing-app", "dev_portal");
  await expect(page.getByRole("heading", { name: "앱에 접근할 수 없습니다" })).toBeVisible();
  for (const origin of [origins.portalOrigin, origins.devPortalOrigin]) {
    expect((await page.request.get(origin + "/api/v1/admin/overview")).status()).toBe(403);
    expect((await page.request.get(origin + "/api/v1/users/directory")).status()).toBe(403);
  }
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

test("anonymous reports persist once, stay private and support administrator triage", async ({ page, playwright, baseURL }) => {
  const client = await playwright.request.newContext({ baseURL });
  try {
    const input = {
      schemaVersion: 1, id: randomUUID(), message: "Mac 인증 오류\npassword=PRIVATE-SENTINEL\n<script>alert(1)</script>",
      diagnostics: { cliVersion: "0.12.0", bundleVersion: "0.11.0", os: "macos", arch: "aarch64", configSource: "home", executable: "project", lastError: null },
    };
    const endpoint = "/api/v1/support/reports";
    expect((await client.post(endpoint, { data: input, headers: { authorization: "Bearer broken" } })).status()).toBe(201);
    expect((await client.post(endpoint, { data: input })).status()).toBe(200);
    expect((await client.post(endpoint, { data: { ...input, message: "changed" } })).status()).toBe(409);
    expect((await client.get(endpoint)).status()).toBe(403);
    expect((await client.patch(`${endpoint}/${input.id}`, { data: { status: "resolved", note: "", revision: 1 } })).status()).toBe(403);
    expect((await client.post(endpoint, { data: input, headers: { origin: String(baseURL) } })).status()).toBe(403);
    expect((await client.post(endpoint, { data: { ...input, id: randomUUID(), credentials: "disallowed" } })).status()).toBe(422);
    expect((await client.post(endpoint, { data: { ...input, id: randomUUID(), message: "x".repeat(17000) } })).status()).toBe(413);
    const origins = await serverOrigins(page);
    expect((await client.get(origins.consoleOrigin + endpoint)).status()).toBe(401);
    expect((await client.post(origins.devPortalOrigin + endpoint, { data: input })).status()).toBe(403);
    expect((await client.post(origins.consoleOrigin + endpoint, { data: input })).status()).toBe(403);
    await signIn(page, /사용자 A/, "/admin", "console");
    expect((await page.request.get(origins.consoleOrigin + endpoint)).status()).toBe(403);
    await page.getByRole("button", { name: "로그아웃" }).click();
    await expect(page.getByRole("heading", { name: "로그인", exact: true })).toBeVisible();
    await signIn(page, /관리자로 시작/, "/admin/support", "console");
    await page.getByRole("button", { name: "신고 조회", exact: true }).click();
    const report = page.locator("details.support-report").filter({ hasText: input.id });
    await report.locator(":scope > summary").click();
    await expect(report.getByText(input.id, { exact: true })).toBeVisible();
    await expect(report).not.toContainText("PRIVATE-SENTINEL");
    await expect(report).toContainText("[redacted sensitive line]");
    expect(await report.locator("script").count()).toBe(0);
    await report.getByRole("combobox", { name: "처리 상태" }).selectOption("investigating");
    await report.getByLabel("조사·해결 메모").fill("안전한 OS 오류 코드를 확인 중");
    await report.getByRole("button", { name: "처리 저장" }).click();
    await expect(page.getByText("처리 상태와 메모를 저장했습니다.")).toBeVisible();
    const me = await (await page.request.get(origins.consoleOrigin + "/api/v1/me")).json();
    const update = { status: "resolved", note: "해결 확인", revision: 1 };
    expect((await page.request.patch(origins.consoleOrigin + endpoint + `/${input.id}`, { data: update, headers: { origin: origins.consoleOrigin, "x-miy-csrf": me.csrf } })).status()).toBe(409);
    expect((await page.request.patch(origins.consoleOrigin + endpoint + `/${input.id}`, { data: { ...update, revision: 2 } })).status()).toBe(403);
    await page.reload();
    await page.getByRole("button", { name: "신고 조회", exact: true }).click();
    await report.locator(":scope > summary").click();
    await expect(report.getByLabel("조사·해결 메모")).toHaveValue("안전한 OS 오류 코드를 확인 중");
    const entries = await (await page.request.get(origins.consoleOrigin + endpoint)).json();
    expect(entries.entries.filter((entry: {id: string}) => entry.id === input.id)).toHaveLength(1);
    // Fill the bounded hourly intake in this disposable database only.
    for (let i = 0; i < 119; i++) {
      expect((await client.post(endpoint, { data: { ...input, id: randomUUID(), message: "rate fixture" } })).status()).toBe(201);
    }
    expect((await client.post(endpoint, { data: { ...input, id: randomUUID() } })).status()).toBe(429);
    // A receipt retry still succeeds at capacity, without another row.
    expect((await client.post(endpoint, { data: input })).status()).toBe(200);
  } finally { await client.dispose(); }
});
