import { test, expect } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";

test("service health changes independently from process controls", async ({
  page,
}, testInfo) => {
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
        ? { id: "fixture", name: "검증 관리자", admin: true, csrf: "fixture" }
        : url.pathname === "/api/v1/installations"
          ? [app]
          : [];
    await route.fulfill({ json: value });
  });
  await page.goto("/environments/dev");
  const status = page.getByRole("status", { name: "앱 실행 및 서비스 상태" });
  await expect(status).toContainText("컨테이너: running · 서비스: 정상");
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

// Load a real Federation module, with synthetic API data and no platform writes.
async function appHarness(page: import("@playwright/test").Page) {
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
  await page.route("**/artifacts/health-fixture/**", async (route) => {
    const path = resolve(
      remote,
      new URL(route.request().url()).pathname.slice(
        "/artifacts/health-fixture/".length,
      ),
    );
    expect(path.startsWith(remote + sep)).toBe(true);
    await route.fulfill({ path });
  });
  await page.route("**/api/**", async (route) => {
    expect(route.request().method()).toBe("GET");
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/installations" && hold) {
      const pending = hold;
      hold = undefined;
      pending(route);
      return;
    }
    if (path.includes("/api/document") || path.includes("/api/probe"))
      apiCalls.push(path);
    await route.fulfill({
      json:
        path === "/api/v1/me"
          ? { id: "fixture", name: "검증 사용자", admin: true, csrf: "fixture" }
          : path === "/api/v1/installations"
            ? apps
            : { title: "저장된 제목" },
    });
  });
  await page.goto("/environments/dev/apps/first-app");
  await expect(page.getByLabel("문서 제목")).toHaveValue("저장된 제목");
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
  const input = page.getByLabel("문서 제목");
  await input.fill("미저장 내용");
  const reads = apiCalls.length;
  first.health = {
    ...first.health!,
    status: "degraded",
    consecutiveFailures: 1,
    checkedAt: new Date().toISOString(),
  };
  await expect(
    page.getByRole("status", { name: "앱 실행 및 서비스 상태" }),
  ).toContainText("1/3회 실패");
  await expect(input).toHaveValue("미저장 내용");
  expect(apiCalls.length).toBe(reads);
  first.health = {
    ...first.health,
    status: "healthy",
    consecutiveFailures: 0,
    checkedAt: new Date().toISOString(),
  };
  await expect(
    page.getByRole("status", { name: "앱 실행 및 서비스 상태" }),
  ).toContainText("서비스: 정상");
  await expect(input).toHaveValue("미저장 내용");
  expect(apiCalls.length).toBe(reads);
  first.releaseId = "30000000-0000-4000-8000-000000000001";
  await expect(page.getByText("앱 상태가 변경되었습니다")).toBeVisible();
  await page.getByRole("button", { name: "API 확인" }).click();
  await expect(page.getByTestId("api-error")).toContainText(
    "release_changed_reload_required",
  );
  await expect(input).toHaveValue("미저장 내용");
  expect(apiCalls.length).toBe(reads);
});

for (const target of ["app", "environment"] as const) {
  test(`late polling response cannot cross the ${target} boundary`, async ({
    page,
  }) => {
    const harness = await appHarness(page);
    const pending = await harness.holdNextPoll();
    const next = target === "app" ? harness.second : harness.production;
    if (target === "app")
      await page.getByRole("link", { name: "두 번째 앱", exact: true }).click();
    else {
      await page.getByRole("link", { name: "운영 영역", exact: true }).click();
      await page.getByRole("link", { name: "첫 번째 앱", exact: true }).click();
    }
    await expect(page.getByTestId("installation")).toHaveText(next.id);
    harness.first.health = {
      ...harness.first.health!,
      checkedAt: new Date(Date.now() + 5000).toISOString(),
    };
    await pending.fulfill({ json: harness.apps });
    await page.getByRole("button", { name: "API 확인" }).click();
    await expect
      .poll(() => harness.apiCalls.at(-1))
      .toBe(`/api/v1/installations/${next.id}/api/probe`);
    await expect(page.getByTestId("installation")).toHaveText(next.id);
    await expect(page).toHaveURL(
      new RegExp(`/environments/${next.environment}/apps/${next.appId}$`),
    );
  });
}

test("device approval displays origin/account and sends the typed code with CSRF exactly once", async ({ page }) => {
  const id="11111111-2222-4333-8444-555555555555";
  const decisions: unknown[]=[];
  await page.route("**/api/**",async route=>{
    const path=new URL(route.request().url()).pathname;
    if(path==="/api/v1/me") return route.fulfill({json:{id:"fixture-user",name:"승인할 계정",admin:false,csrf:"fixture-csrf"}});
    if(path===`/api/v1/cli/devices/${id}/decision`){
      expect(route.request().headers()["x-miy-csrf"]).toBe("fixture-csrf");
      decisions.push(route.request().postDataJSON());return route.fulfill({json:{approved:true}});
    }
    return route.fulfill({json:[]});
  });
  await page.goto(`/cli/device/${id}`);
  await expect(page.getByRole("heading",{name:"CLI 로그인 승인"})).toBeVisible();
  await expect(page.getByText("승인할 계정",{exact:true})).toBeVisible();
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
  const app={id:"aaaaaaaa-1111-4222-8333-bbbbbbbbbbbb",appId:"confirmed-app",name:"확인 대상",releaseId:"cccccccc-1111-4222-8333-dddddddddddd",environment:"prod",version:"1.0.0",dbStatus:"ready",runtimeStatus:"running",desiredState:"running",kind:"docker_app",canManage:true,health:{status:"healthy",consecutiveFailures:0,checkedAt:new Date().toISOString()},ui:{entry:"/unused",export:"./AppRoot",contractVersion:"miy.ui/v1alpha1"}};
  const expectedState={target:{appId:app.appId,environment:"prod"},installation:{installationId:app.id,generation:12}};
  const mutations: Record<string,unknown>[]=[];
  await page.route("**/api/**",async route=>{
    const path=new URL(route.request().url()).pathname;
    if(path==="/api/v1/me")return route.fulfill({json:{id:"fixture",name:"관리자",admin:true,csrf:"fixture-csrf"}});
    if(path==="/api/v1/installations")return route.fulfill({json:[app]});
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
  page.once("dialog",async dialog=>{expect(dialog.message()).toContain("confirmed-app");expect(dialog.message()).toContain("prod");await dialog.dismiss();});
  await page.getByRole("button",{name:"중지",exact:true}).click();
  await expect(page.getByRole("button",{name:"중지",exact:true})).toBeEnabled();
  expect(mutations).toHaveLength(0);
  page.once("dialog",async dialog=>{await dialog.accept();});
  await page.getByRole("button",{name:"중지",exact:true}).click();
  await expect(page.getByRole("alert")).toContainText("state_changed");
  expect(mutations).toHaveLength(1);
  expect(mutations[0]).toMatchObject({action:"stop",environment:"prod",expectedState,confirmed:true});
  await expect(page.getByRole("button",{name:"중지",exact:true})).toBeEnabled();
});
