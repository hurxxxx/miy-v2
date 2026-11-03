import { test, expect, type Page, type Route } from "@playwright/test";
import type { Installation } from "../packages/sdk/src/index";

async function harness(page: Page, holdPreflight = false) {
  const app: Installation = {
    id: "confirmation-installation",
    appId: "confirmation-app",
    name: "확인창 검증 앱",
    releaseId: "confirmation-release",
    environment: "prod",
    version: "1.0.0",
    dbStatus: "ready",
    runtimeStatus: "running",
    desiredState: "running",
    kind: "docker_app",
    canManage: true,
    ui: {
      entry: "/unused-manifest.json",
      export: "./AppRoot",
      contractVersion: "miy.ui/v1alpha1",
    },
  };
  const writes: string[] = [];
  let preflight: Route | undefined;
  let jobStatus = "running";
  let jobReads = 0;
  let installationReads = 0;
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() !== "GET") writes.push(path);
    if (path === "/api/v1/me")
      return route.fulfill({
        json: {
          id: "fixture",
          name: "확인창 검증 사용자",
          admin: true,
          csrf: "fixture",
        },
      });
    if (path === "/api/v1/installations") {
      installationReads++;
      return route.fulfill({ json: [app] });
    }
    if (path.endsWith("/runtime/preflight")) {
      if (holdPreflight) {
        preflight = route;
        return;
      }
      return route.fulfill({
        json: { canSubmit: true, expectedState: {}, changes: [] },
      });
    }
    if (path.endsWith("/runtime"))
      return route.fulfill({ json: { id: "confirmation-job" } });
    if (path === "/api/v1/runtime/jobs/confirmation-job") {
      jobReads++;
      return route.fulfill({
        json: {
          status: jobStatus,
          step: "fixture",
          errorCode: "fixture_job_failed",
        },
      });
    }
    return route.fulfill({ status: 404, json: {} });
  });
  await page.goto("/environments/prod");
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toBeVisible();
  return {
    app,
    writes,
    preflight: () => preflight,
    jobReads: () => jobReads,
    installationReads: () => installationReads,
    finishJob: (status: string) => {
      jobStatus = status;
    },
  };
}

for (const status of ["succeeded", "failed"]) {
  test(`confirmed runtime action restores focus after a delayed ${status} job`, async ({
    page,
  }) => {
    const state = await harness(page);
    const restart = page.getByRole("button", { name: "재시작", exact: true });
    await restart.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "재시작", exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect.poll(state.jobReads).toBeGreaterThan(0);
    await expect(restart).toBeDisabled();
    // Keep the job pending beyond the dialog's cleanup animation frame.
    await page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
    state.finishJob(status);
    await expect(restart).toBeEnabled();
    await expect(restart).toBeFocused();
    expect(
      state.writes.filter((path) => path.endsWith("/runtime")),
    ).toHaveLength(1);
  });
}

test("removal while confirming cancels the old operation before the installation is redeployed", async ({
  page,
}) => {
  const state = await harness(page);
  await page.getByRole("button", { name: "재시작", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  state.app.runtimeStatus = "removed";
  state.app.desiredState = "removed";
  await expect(
    page.getByText(
      "앱 실행 환경이 제거됐습니다. DB는 보존됩니다. CLI에서 다시 배포할 수 있습니다.",
      { exact: true },
    ),
  ).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  state.app.runtimeStatus = "running";
  state.app.desiredState = "running";
  state.app.releaseId = "replacement-release";
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toBeEnabled();
  expect(state.writes).toEqual([
    "/api/v1/apps/confirmation-app/runtime/preflight",
  ]);
});

test("a late preflight cannot open a confirmation or submit after management permission is withdrawn", async ({
  page,
}) => {
  const state = await harness(page, true);
  await page.getByRole("button", { name: "재시작", exact: true }).click();
  await expect.poll(state.preflight).toBeDefined();
  state.app.canManage = false;
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toHaveCount(0);
  await state
    .preflight()!
    .fulfill({ json: { canSubmit: true, expectedState: {}, changes: [] } });
  const reads = state.installationReads();
  await expect.poll(state.installationReads).toBeGreaterThan(reads);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  state.app.canManage = true;
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toBeEnabled();
  expect(state.writes).toEqual([
    "/api/v1/apps/confirmation-app/runtime/preflight",
  ]);
});

test("a preflight response for the previous release cannot open a new confirmation", async ({
  page,
}) => {
  const state = await harness(page, true);
  await page.getByRole("button", { name: "재시작", exact: true }).click();
  await expect.poll(state.preflight).toBeDefined();
  state.app.releaseId = "replacement-release";
  state.app.version = "2.0.0";
  await expect(page.getByText("v2.0.0", { exact: true })).toBeVisible();
  await state
    .preflight()!
    .fulfill({ json: { canSubmit: true, expectedState: {}, changes: [] } });
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toBeEnabled();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(state.writes).toEqual([
    "/api/v1/apps/confirmation-app/runtime/preflight",
  ]);
});

test("job completion preserves focus when the user has moved to another control", async ({
  page,
}) => {
  const state = await harness(page);
  await page.getByRole("button", { name: "재시작", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "재시작", exact: true })
    .click();
  await expect.poll(state.jobReads).toBeGreaterThan(0);
  const home = page.getByRole("link", { name: "홈", exact: true });
  await home.focus();
  state.finishJob("succeeded");
  await expect(
    page.getByRole("button", { name: "재시작", exact: true }),
  ).toBeEnabled();
  await expect(home).toBeFocused();
});
