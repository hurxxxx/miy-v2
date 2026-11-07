import { test, expect, type Page } from "@playwright/test";
import { browserOptions, browserUser, mockSurface } from "./browser-surface";

// Mocked presentation/routing checks only. Backend purpose/Origin enforcement is
// verified independently against the isolated core; these are not server auth proof.
const user = {
  id: "developer",
  name: "사내 개발 담당자",
  csrf: "fixture-csrf",
  admin: false,
};
const app = {
  id: "fixture-installation",
  appId: "ledger",
  name: "<script>회사 장부</script>",
  releaseId: "fixture-release",
  environment: "prod",
  version: "9.8.7",
  kind: "docker_app",
  dbStatus: "ready",
  runtimeStatus: "running",
  desiredState: "running",
  canManage: true,
  appDisabled: false,
  canOpen: true,
  ui: {
    entry: "/artifacts/untrusted/mf-manifest.json",
    export: "./AppRoot",
    contractVersion: "miy.ui/v1alpha1",
  },
};

async function setup(page: Page, surface: "portal" | "console") {
  await mockSurface(page, surface);
  const reads: string[] = [];
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    reads.push(path);
    if (path === "/api/v1/me")
      return route.fulfill({ json: browserUser(route, user) });
    if (
      path === "/api/v1/installations" ||
      path === "/api/v1/runtime/installations"
    )
      return route.fulfill({ json: [app] });
    if (path === "/api/v1/apps")
      return route.fulfill({
        json: {
          apps: [{ id: "new-app", name: "아직 설치하지 않은 담당 앱" }],
          nextCursor: null,
        },
      });
    return route.fulfill({
      status: 404,
      json: { error: { code: "not_found" } },
    });
  });
  return reads;
}

test("portal shows business apps without management controls or operational metadata", async ({
  page,
}) => {
  const reads = await setup(page, "portal");
  await page.goto("/environments/prod?surface=console");
  await expect(
    page.getByRole("heading", { name: "운영 앱", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "앱 열기 →" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: /중지|재시작|제거|시작/ }),
  ).toHaveCount(0);
  await expect(page.getByText("DB ready", { exact: true })).toHaveCount(0);
  await expect(page.getByText("v9.8.7", { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole("link", { name: "관리 콘솔", exact: true }),
  ).toHaveAttribute("href", "https://console.example.test/environments/prod");
  await expect(
    page.getByRole("link", { name: "플랫폼 관리", exact: true }),
  ).toHaveCount(0);
  expect(reads).not.toContain("/api/v1/runtime/installations");
  expect(reads).not.toContain("/api/v1/apps");
});

test("a non-admin console user sees manageable apps, including disabled and uninstalled apps", async ({
  page,
}) => {
  const reads = await setup(page, "console");
  await page.route("**/api/v1/runtime/installations", (route) =>
    route.fulfill({ json: [{ ...app, appDisabled: true, canOpen: false }] }),
  );
  await page.goto("/environments/prod");
  await expect(
    page.getByRole("heading", { name: "내가 관리하는 앱", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("회사 사용 중지", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "중지", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("아직 설치하지 않은 담당 앱", { exact: false }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: /포털에서 앱 열기/ }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("link", { name: "플랫폼 관리", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("heading", { name: app.name, exact: true }),
  ).toBeVisible();
  await expect(page.locator("main script")).toHaveCount(0);
  expect(reads).not.toContain("/api/v1/installations");
  await page.goto("/admin");
  await expect(
    page.getByRole("heading", { name: "관리자 권한이 필요합니다" }),
  ).toBeVisible();
});

for (const path of [
  "/apps/ledger/nested",
  "/environments/prod/apps/ledger/nested",
]) {
  test(`console rejects business route ${path} before importing the remote runtime`, async ({
    page,
  }) => {
    const reads = await setup(page, "console");
    const loaded: string[] = [];
    page.on("request", (request) =>
      loaded.push(new URL(request.url()).pathname),
    );
    await page.goto(`${path}?surface=portal&next=https://attacker.invalid`);
    await expect(
      page.getByRole("heading", { name: "업무 앱은 포털에서 열어 주세요" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "업무 포털에서 열기" }),
    ).toHaveAttribute("href", `https://portal.example.test${path}`);
    expect(reads.filter((entry) => entry !== "/api/v1/me")).toEqual([]);
    expect(
      loaded.filter(
        (entry) =>
          entry.startsWith("/artifacts/") || /\/AppView[-.]/.test(entry),
      ),
    ).toEqual([]);
    await expect(page.locator("iframe")).toHaveCount(0);
  });
}

for (const path of [
  "/admin",
  "/cli/authorize/fixture-id",
  "/cli/device/fixture-id",
  "/cli/sessions",
]) {
  test(`portal sends ${path} to the fixed console without forwarding redirect input`, async ({
    page,
  }) => {
    const reads = await setup(page, "portal");
    await page.route("https://console.example.test/**", (route) =>
      route.fulfill({ contentType: "text/html", body: "<p>신뢰 콘솔</p>" }),
    );
    await page.goto(
      `${path}?next=https://attacker.invalid&surface=console#credential`,
    );
    await expect(page).toHaveURL(`https://console.example.test${path}`);
    expect(
      reads.filter((entry) =>
        /approve|decision|registrations|users/.test(entry),
      ),
    ).toEqual([]);
  });
}

test("missing or mismatched server surface configuration fails before loading an app", async ({
  page,
}) => {
  await setup(page, "console");
  await page.route("**/auth/options", (route) =>
    route.fulfill({
      json: { ...browserOptions(route), surface: "unexpected" },
    }),
  );
  await page.goto("/apps/ledger");
  await expect(page.getByRole("alert")).toContainText(
    "서버에 연결하지 못했습니다",
  );
  await expect(
    page.getByRole("link", { name: "업무 포털에서 열기" }),
  ).toHaveCount(0);
  await page.unroute("**/auth/options");
  await mockSurface(page, "portal");
  await page.route("**/api/v1/me", (route) =>
    route.fulfill({
      json: { ...browserUser(route, user), browserPurpose: "console" },
    }),
  );
  await page.reload();
  await expect(page.getByRole("alert")).toContainText(
    "서버에 연결하지 못했습니다",
  );
  await expect(page.locator(".miy-app-root")).toHaveCount(0);
});

test("console lists and revokes both device and legacy CLI connections with the current CSRF", async ({
  page,
}) => {
  await setup(page, "console");
  const requests: { path: string; csrf?: string }[] = [];
  const expiresAt = new Date(Date.now() + 3600000).toISOString();
  const device = [
    {
      id: "device-id",
      clientName: "회사 CLI",
      createdAt: "2026-11-06T00:00:00Z",
      expiresAt,
      revokedAt: null,
    },
  ];
  const legacy = [
    { id: "legacy-id", createdAt: "2026-11-06T00:00:00Z", expiresAt },
  ];
  await page.route("**/api/v1/cli/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const isLegacy = path.includes("legacy-sessions");
    if (route.request().method() === "DELETE") {
      requests.push({ path, csrf: route.request().headers()["x-miy-csrf"] });
      if (isLegacy) legacy.splice(0);
      else device.splice(0);
      return route.fulfill({ json: { revoked: true } });
    }
    return route.fulfill({
      json: { sessions: isLegacy ? legacy : device, nextCursor: null },
    });
  });
  await page.goto("/cli/sessions");
  await expect(
    page.getByRole("heading", { name: "내 CLI 연결", exact: true }),
  ).toBeVisible();
  for (const title of ["CLI 로그인 세션", "기존 방식 CLI 연결"]) {
    const panel = page
      .locator(".ui-panel")
      .filter({ has: page.getByRole("heading", { name: title, exact: true }) });
    await panel.getByRole("button", { name: "연결 철회", exact: true }).click();
    await expect(
      panel.getByText("표시할 연결이 없습니다.", { exact: true }),
    ).toBeVisible();
  }
  expect(requests).toEqual([
    { path: "/api/v1/cli/sessions/device-id", csrf: "fixture-csrf" },
    { path: "/api/v1/cli/legacy-sessions/legacy-id", csrf: "fixture-csrf" },
  ]);
});

test("legacy CLI approval remains available to an ordinary user only on the console", async ({
  page,
}) => {
  await setup(page, "console");
  const approvals: string[] = [];
  await page.route(
    "**/api/v1/cli-authorizations/request-id/approve",
    (route) => {
      approvals.push(route.request().headers()["x-miy-csrf"]!);
      return route.fulfill({ json: { approved: true } });
    },
  );
  await page.goto("/cli/authorize/request-id");
  await page
    .getByRole("button", { name: "이 CLI 연결 승인", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "CLI 연결을 승인했습니다",
  );
  expect(approvals).toEqual(["fixture-csrf"]);
});
