import { browserUser, mockSurface } from "./browser-surface";
import { test, expect } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";
import { openNewTab } from "./new-tab";
import { mockFrames } from "./frame-mock";

test("service health changes independently from process controls", async ({
  page,
}, testInfo) => {
  await mockSurface(page, "console");
  const app: Installation = {
    id: "test-installation",
    appId: "health-probe",
    name: "상태 검증 앱",
    releaseId: "test-release",
    environment: "dev",
    version: "1.0.0",
    dbStatus: "ready",
    runtimeStatus: "running",
    desiredState: "running",
    kind: "docker_app",
    canManage: true,
    health: {
      status: "healthy",
      consecutiveFailures: 0,
      checkedAt: new Date().toISOString(),
      errorCode: null,
    },
    ui: {
      entry: "/unused-manifest.json",
      export: "./AppRoot",
      contractVersion: "miy.ui/v1alpha1",
    },
  };
  let writes = 0;
  await page.route("**/api/**", async (route) => {
    if (route.request().method() !== "GET") writes++;
    const url = new URL(route.request().url());
    const value =
      url.pathname === "/api/v1/me"
        ? browserUser(route, { id: "fixture", name: "검증 관리자", admin: true, csrf: "fixture" })
        : url.pathname === "/api/v1/installations"
          ? [app]
          : url.pathname === "/api/v1/runtime/installations"
            ? [{ ...app, appDisabled: false, canOpen: true }]
            : url.pathname === "/api/v1/apps" ? { apps: [], nextCursor: null } : [];
    await route.fulfill({ json: value });
  });
  await page.goto("/environments/dev");
  const status = page.getByRole("status", { name: "앱 실행 및 서비스 상태" });
  await expect(status).toContainText("컨테이너: running · 서비스: 정상");
  await page.context().route("https://dev-portal.example.test/**", (route) =>
    route.fulfill({ contentType: "text/html", body: "<p>업무 앱</p>" }),
  );
  await (await openNewTab(page, page.getByRole("link", { name: "포털에서 앱 열기", exact: true }))).close();
  app.health = {
    ...app.health!,
    status: "degraded",
    consecutiveFailures: 1,
    errorCode: "health_http_failed",
  };
  await expect(status).toContainText("응답 확인 중 (1/3회 실패)");
  app.health = { ...app.health!, status: "unhealthy", consecutiveFailures: 3 };
  await expect(status).toContainText("컨테이너: running · 서비스: 응답 비정상");
  await expect(
    page.getByRole("button", { name: "중지", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "시작", exact: true }),
  ).toHaveCount(0);
  await page.screenshot({
    path: testInfo.outputPath("unhealthy.png"),
    fullPage: true,
  });
  await page.goto("/admin");
  await expect(status).toContainText("서비스: 응답 비정상");
  app.health = {
    ...app.health!,
    status: "healthy",
    consecutiveFailures: 0,
    errorCode: null,
    checkedAt: new Date().toISOString(),
  };
  await expect(status).toContainText("서비스: 정상");
  app.health.checkedAt = new Date(Date.now() - 60000).toISOString();
  await expect(status).toContainText("서비스: 확인 지연");
  app.runtimeStatus = "stopped";
  app.desiredState = "stopped";
  app.health = {
    status: "not_applicable",
    consecutiveFailures: 0,
    checkedAt: null,
    errorCode: null,
  };
  await expect(status).toContainText(
    "컨테이너: stopped · 서비스: 검사 대상 아님",
  );
  await expect(
    page.getByRole("button", { name: "시작", exact: true }),
  ).toBeVisible();
  expect(writes).toBe(0);
});

const app = (page: import("@playwright/test").Page) => page.frameLocator("iframe.app-frame");
// Load a real Federation module in its frame, with synthetic API data and no platform writes.
async function appHarness(page: import("@playwright/test").Page) {
  await mockSurface(page);
  const { resolve, sep } = await import("node:path");
  const remote = resolve(".local/evidence/runtime-health-ui/remote");
  const first: Installation = {
    id: "10000000-0000-4000-8000-000000000001",
    appId: "first-app",
    name: "첫 번째 앱",
    releaseId: "20000000-0000-4000-8000-000000000001",
    environment: "dev",
    version: "1.0.0",
    dbStatus: "ready",
    runtimeStatus: "running",
    desiredState: "running",
    kind: "docker_app",
    canManage: true,
    health: {
      status: "healthy",
      consecutiveFailures: 0,
      checkedAt: new Date().toISOString(),
      errorCode: null,
    },
    ui: {
      entry: "/artifacts/health-fixture/mf-manifest.json",
      export: "./AppRoot",
      contractVersion: "miy.ui/v1alpha1",
    },
  };
  const second: Installation = {
    ...first,
    id: "10000000-0000-4000-8000-000000000002",
    appId: "second-app",
    name: "두 번째 앱",
    releaseId: "20000000-0000-4000-8000-000000000002",
  };
  const production: Installation = {
    ...first,
    id: "10000000-0000-4000-8000-000000000003",
    environment: "prod",
  };
  const apps = [first, second, production],
    apiCalls: string[] = [];
  let hold: ((route: import("@playwright/test").Route) => void) | undefined;
  await page.route("**/api/**", async (route) => {
    expect(route.request().method()).toBe("GET");
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/installations" && hold) {
      const pending = hold;
      hold = undefined;
      pending(route);
      return;
    }
    await route.fulfill({
      json:
        path === "/api/v1/me"
          ? browserUser(route, { id: "fixture", name: "검증 사용자", admin: true, csrf: "fixture" })
          : path === "/api/v1/installations"
            ? apps
            : { title: "저장된 제목" },
    });
  });
  // Apps run in their own frames; their API reaches this mock by frame origin. A
  // call signed with an old release is refused like the server does (409).
  await mockFrames(page, () => apps, (installation, path, _method, release) => {
    if (release !== installation.releaseId)
      return { status: 409, json: { error: { code: "release_changed_reload_required" } } };
    apiCalls.push(`/api/v1/installations/${installation.id}/api${path}`);
    return { json: { title: "저장된 제목" } };
  });
  await page.goto("/environments/dev/apps/first-app");
  await expect(app(page).getByLabel("문서 제목")).toHaveValue("저장된 제목");
  return {
    apps,
    first,
    second,
    production,
    apiCalls,
    holdNextPoll: () =>
      new Promise<import("@playwright/test").Route>((resolve) => {
        hold = resolve;
      }),
  };
}

test("health observations preserve SDK callbacks and unsaved input; release change still blocks APIs", async ({
  page,
}) => {
  const { first, apiCalls } = await appHarness(page);
  const input = app(page).getByLabel("문서 제목");
  await input.fill("미저장 내용");
  const reads = apiCalls.length;
  first.health = {
    ...first.health!,
    status: "degraded",
    consecutiveFailures: 1,
    checkedAt: new Date().toISOString(),
  };
  await expect(
    page.getByRole("status", { name: "앱 이용 안내" }),
  ).toContainText("앱의 응답을 확인하고 있습니다");
  await expect(page.getByText(/컨테이너:|마지막 서비스 검사:|검사 결과:/)).toHaveCount(0);
  await expect(input).toHaveValue("미저장 내용");
  expect(apiCalls.length).toBe(reads);
  first.health = {
    ...first.health,
    status: "healthy",
    consecutiveFailures: 0,
    checkedAt: new Date().toISOString(),
  };
  await expect(
    page.getByRole("status", { name: "앱 이용 안내" }),
  ).toHaveCount(0);
  await expect(input).toHaveValue("미저장 내용");
  expect(apiCalls.length).toBe(reads);
  first.releaseId = "30000000-0000-4000-8000-000000000001";
  await expect(page.getByText("앱 상태가 변경되었습니다")).toBeVisible();
  // The frame's call carries its launched release: refused, and the frame stops.
  await app(page).getByRole("button", { name: "API 확인" }).click();
  await expect(app(page).getByTestId("api-error")).toContainText(
    "release_changed_reload_required",
  );
  await app(page).getByRole("button", { name: "API 확인" }).click();
  await expect(input).toHaveValue("미저장 내용");
  expect(apiCalls.length).toBe(reads);
});

// Environments are separate portal origins, so a late response can no longer cross
// an environment inside one document; the switch opens the other portal instead.
test("the environment switch on a portal opens the other environment's portal", async ({
  page,
}) => {
  await appHarness(page);
  const switcher = page
    .getByRole("navigation", { name: "실행 환경" })
    .filter({ visible: true });
  const prod = switcher.getByRole("link", { name: "운영 영역" });
  await expect(prod).toHaveAttribute("href", "https://portal.example.test/environments/prod");
  await expect(prod).toHaveAttribute("target", "_blank");
  await expect(switcher.getByRole("link", { name: "테스트 영역" })).not.toHaveAttribute("target");
});

for (const target of ["app"] as const) {
  test(`late polling response cannot cross the ${target} boundary`, async ({
    page,
  }) => {
    const harness = await appHarness(page);
    const pending = await harness.holdNextPoll();
    const next = harness.second;
    await page.getByRole("link", { name: "두 번째 앱", exact: true }).click();
    await expect(app(page).getByTestId("installation")).toHaveText(next.id);
    harness.first.health = {
      ...harness.first.health!,
      checkedAt: new Date(Date.now() + 5000).toISOString(),
    };
    await pending.fulfill({ json: harness.apps });
    await app(page).getByRole("button", { name: "API 확인" }).click();
    await expect
      .poll(() => harness.apiCalls.at(-1))
      .toBe(`/api/v1/installations/${next.id}/api/probe`);
    await expect(app(page).getByTestId("installation")).toHaveText(next.id);
    await expect(page).toHaveURL(
      new RegExp(`/environments/${next.environment}/apps/${next.appId}$`),
    );
  });
}

test("device approval displays origin/account and sends the typed code with CSRF exactly once", async ({ page }) => {
  await mockSurface(page, "console");
  const id="11111111-2222-4333-8444-555555555555";
  const decisions: unknown[]=[];
  await page.route("**/api/**",async route=>{
    const path=new URL(route.request().url()).pathname;
    if(path==="/api/v1/me") return route.fulfill({json:browserUser(route,{id:"fixture-user",name:"승인할 계정",admin:false,csrf:"fixture-csrf"})});
    if(path===`/api/v1/cli/devices/${id}`) return route.fulfill({json:{status:"pending",expiresIn:300}});
    if(path===`/api/v1/cli/devices/${id}/decision`){
      expect(route.request().headers()["x-miy-csrf"]).toBe("fixture-csrf");
      decisions.push(route.request().postDataJSON());return route.fulfill({json:{approved:true}});
    }
    return route.fulfill({json:[]});
  });
  await page.goto(`/cli/device/${id}`);
  await expect(page.getByRole("heading",{name:"CLI 로그인 승인"})).toBeVisible();
  await expect(page.locator("main").getByText("승인할 계정",{exact:true})).toBeVisible();
  await expect(page.getByText("http://127.0.0.1:18147",{exact:true})).toBeVisible();
  const approve=page.getByRole("button",{name:"이 계정으로 CLI 연결 승인"});
  await expect(approve).toBeDisabled();
  await page.getByLabel("인증 코드").fill("ABCDE-FGHJK");
  await approve.click();
  await expect(page.getByRole("status")).toContainText("CLI 연결을 승인했습니다");
  expect(decisions).toEqual([{userCode:"ABCDE-FGHJK",decision:"approve"}]);
  await expect(approve).toHaveCount(0);
});

test("production controls confirm the exact target and preserve the inspected expected state", async ({ page }) => {
  await mockSurface(page, "console");
  const app={id:"aaaaaaaa-1111-4222-8333-bbbbbbbbbbbb",appId:"confirmed-app",name:"확인 대상",releaseId:"cccccccc-1111-4222-8333-dddddddddddd",environment:"prod",version:"1.0.0",dbStatus:"ready",runtimeStatus:"running",desiredState:"running",kind:"docker_app",canManage:true,health:{status:"healthy",consecutiveFailures:0,checkedAt:new Date().toISOString()},ui:{entry:"/unused",export:"./AppRoot",contractVersion:"miy.ui/v1alpha1"}};
  const expectedState={target:{appId:app.appId,environment:"prod"},installation:{installationId:app.id,generation:12}};
  const mutations: Record<string,unknown>[]=[];
  await page.route("**/api/**",async route=>{
    const path=new URL(route.request().url()).pathname;
    if(path==="/api/v1/me")return route.fulfill({json:browserUser(route,{id:"fixture",name:"관리자",admin:true,csrf:"fixture-csrf"})});
    if(path==="/api/v1/runtime/installations")return route.fulfill({json:[app]});
    if(path==="/api/v1/apps")return route.fulfill({json:{apps:[],nextCursor:null}});
    if(path.endsWith("/runtime/preflight")){
      expect(route.request().postDataJSON()).toEqual({target:{appId:app.appId,environment:"prod"},action:"stop"});
      return route.fulfill({json:{canSubmit:true,expectedState,changes:["stop_runtime"]}});
    }
    if(path.endsWith("/runtime")){
      mutations.push(route.request().postDataJSON());
      return route.fulfill({status:409,json:{error:{code:"state_changed",message:"state_changed"}}});
    }
    return route.fulfill({json:[]});
  });
  await page.goto("/environments/prod");
  await page.getByRole("button",{name:"중지",exact:true}).click();
  const confirmation=page.getByRole("dialog");
  await expect(confirmation).toContainText("confirmed-app");
  await expect(confirmation).toContainText("prod");
  await confirmation.getByRole("button",{name:"취소"}).click();
  await expect(confirmation).toHaveCount(0);
  await expect(page.getByRole("button",{name:"중지",exact:true})).toBeEnabled();
  expect(mutations).toHaveLength(0);
  await page.getByRole("button",{name:"중지",exact:true}).click();
  await page.getByRole("dialog").getByRole("button",{name:"중지",exact:true}).click();
  await expect(page.getByRole("alert")).toContainText("state_changed");
  expect(mutations).toHaveLength(1);
  expect(mutations[0]).toMatchObject({action:"stop",environment:"prod",expectedState,confirmed:true});
  await expect(page.getByRole("button",{name:"중지",exact:true})).toBeEnabled();
});
