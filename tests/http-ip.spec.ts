import { test, expect } from "@playwright/test";

test("HTTP IP runtime controls create a valid request without secure-context APIs", async ({ page }) => {
  const app = {
    id: "aaaaaaaa-1111-4222-8333-bbbbbbbbbbbb", appId: "http-probe", name: "HTTP 검증 앱",
    releaseId: "cccccccc-1111-4222-8333-dddddddddddd", environment: "dev", version: "1.0.0",
    dbStatus: "ready", runtimeStatus: "running", desiredState: "running", kind: "docker_app", canManage: true,
    health: { status: "healthy", consecutiveFailures: 0, checkedAt: new Date().toISOString() },
    ui: { entry: "/unused", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
  };
  const expectedState = { target: { appId: app.appId, environment: "dev" }, installation: { installationId: app.id, generation: 1 } };
  const mutations: Record<string, unknown>[] = [];
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/me") return route.fulfill({ json: { id: "http-user", name: "HTTP 개발자", admin: false, csrf: "fixture-csrf" } });
    if (path === "/api/v1/installations") return route.fulfill({ json: [app] });
    if (path.endsWith("/runtime/preflight")) return route.fulfill({ json: { canSubmit: true, expectedState, changes: ["stop_runtime"] } });
    if (path.endsWith("/runtime")) {
      expect(route.request().headers()["x-miy-csrf"]).toBe("fixture-csrf");
      mutations.push(route.request().postDataJSON());
      return route.fulfill({ status: 409, json: { error: { code: "state_changed", message: "state_changed" } } });
    }
    return route.fulfill({ json: [] });
  });
  await page.goto("/environments/dev");
  expect(await page.evaluate(() => window.isSecureContext)).toBe(false);
  expect(await page.evaluate(() => typeof crypto.randomUUID)).toBe("undefined");
  await page.getByRole("button", { name: "중지", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("state_changed");
  expect(mutations).toHaveLength(1);
  expect(mutations[0]).toMatchObject({ action: "stop", environment: "dev", expectedState });
  expect(mutations[0]!.idempotencyKey).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});
