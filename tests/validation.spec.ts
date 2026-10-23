import { test, expect, login, api, origin } from "./fixtures";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

function treeHash(root: string): string {
  const files: string[] = [];
  function walk(path: string) {
    for (const f of readdirSync(path, { withFileTypes: true })) {
      const p = join(path, f.name);
      if (f.isDirectory()) walk(p);
      else files.push(p);
    }
  }
  walk(root);
  return createHash("sha256")
    .update(
      files
        .sort()
        .map(
          (p) =>
            p +
            ":" +
            createHash("sha256").update(readFileSync(p)).digest("hex"),
        )
        .join("\n"),
    )
    .digest("hex");
}
test("runtime modules, OAuth session, permissions and DB preparation", async ({
  browser,
}) => {
  const run = Date.now().toString(36);
  const appIds = ["reference-" + run, "diagnosis-" + run];
  const hostHash = treeHash("web/platform/dist");
  const admin = await browser.newContext({ baseURL: origin });
  const page = await admin.newPage();
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await login(page, "admin");
  const installations: any[] = [];
  for (const app of appIds) {
    const artifact = JSON.parse(
      execFileSync("python3", ["scripts/import-fixture.py", app], {
        encoding: "utf8",
      }),
    );
    const a = await api(admin, "/apps", "POST", {
      id: app,
      name: app.startsWith("reference") ? "기준정보" : "UI 진단",
    });
    expect(a.status()).toBe(200);
    const release = await api(admin, `/apps/${app}/releases`, "POST", {
      artifactHash: artifact.artifactHash,
    });
    expect(release.status()).toBe(200);
    const r = await release.json();
    const i = await api(admin, "/installations", "POST", {
      appId: app,
      environment: "dev",
      releaseId: r.id,
    });
    expect(i.status()).toBe(200);
    installations.push({
      ...(await i.json()),
      releaseId: r.id,
      artifactHash: artifact.artifactHash,
    });
  }
  const production = await api(admin, "/installations", "POST", {
    appId: appIds[0],
    environment: "prod",
    releaseId: installations[0].releaseId,
  });
  expect(production.status()).toBe(400);
  expect((await production.json()).error.code).toBe(
    "use_checked_release_promotion_for_prod",
  );
  // Isolated DB-per-environment fixture, not a production deployment. The public
  // installation endpoint must reject it; Docker promotion is tested separately.
  const prod = { id: crypto.randomUUID() };
  execFileSync(
    "docker",
    [
      "exec",
      "-i",
      "miy-v2-validation-postgres-1",
      "psql",
      "-U",
      "postgres",
      "-d",
      "miy_core",
      "-v",
      "ON_ERROR_STOP=1",
      "-v",
      `id=${prod.id}`,
      "-v",
      `app_id=${appIds[0]}`,
      "-v",
      `release_id=${installations[0].releaseId}`,
    ],
    {
      input:
        "INSERT INTO installations(id,app_id,environment,release_id) VALUES(:'id',:'app_id','prod',:'release_id');\n",
    },
  );
  const userIds = {
    developer: "00000000-0000-4000-8000-000000000002",
    alice: "00000000-0000-4000-8000-000000000003",
    bob: "00000000-0000-4000-8000-000000000004",
  };
  for (const [userId, permission] of [
    [userIds.developer, "develop"],
    [userIds.alice, "use"],
  ]) {
    expect(
      (
        await api(admin, "/grants", "PUT", {
          userId,
          appId: appIds[0],
          environment: "dev",
          permission,
          enabled: true,
        })
      ).ok(),
    ).toBeTruthy();
  }
  await page.goto(`/apps/${appIds[0]}`);
  await expect(page.getByTestId("app-root")).toBeVisible();
  await page.getByRole("button", { name: "사무 장비 보기" }).click();
  await expect(page).toHaveURL(
    new RegExp(`/apps/${appIds[0]}/detail/1\\?view=full`),
  );
  await expect(page.getByRole("heading", { name: "분류 상세" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("heading", { name: "분류 상세" })).toBeVisible();
  await page.getByRole("button", { name: "목록으로" }).click();
  await page.goBack();
  await expect(page.getByRole("heading", { name: "분류 상세" })).toBeVisible();
  await page
    .locator(`nav a[href="/environments/dev/apps/${appIds[1]}"]`)
    .click();
  await expect(page.getByTestId("app-root")).toBeVisible();
  expect(await page.locator("iframe").count()).toBe(0);
  await page.goto(`/apps/${appIds[0]}/crash`);
  await expect(
    page.getByRole("heading", { name: "앱을 열 수 없습니다" }),
  ).toBeVisible();
  await expect(page.locator("header")).toBeVisible();
  await page.locator("nav").getByText("홈", { exact: true }).click();
  expect(
    errors.filter((e) => !e.includes("intentional_fixture_render_failure")),
  ).toEqual([]);
  const dev = await browser.newContext({ baseURL: origin });
  const devPage = await dev.newPage();
  await login(devPage, "developer");
  const visible = await (await api(dev, "/installations")).json();
  expect(visible.map((a: any) => a.id)).toContain(installations[0].id);
  expect(visible.map((a: any) => a.id)).not.toContain(prod.id);
  const prepare = {
    installationId: installations[0].id,
    releaseId: installations[0].releaseId,
    idempotencyKey: "db-" + run,
  };
  const [one, two] = await Promise.all([
    api(dev, "/database/prepare", "POST", prepare),
    api(dev, "/database/prepare", "POST", prepare),
  ]);
  expect(one.status()).toBe(200);
  expect(two.status()).toBe(200);
  const job = await one.json();
  expect((await two.json()).id).toBe(job.id);
  await expect
    .poll(
      async () => {
        const r = await api(dev, "/jobs/" + job.id);
        return (await r.json()).status;
      },
      { timeout: 60000 },
    )
    .toBe("succeeded");
  expect(
    (
      await api(dev, "/database/prepare", "POST", {
        ...prepare,
        installationId: prod.id,
        idempotencyKey: "forbidden-" + run,
      })
    ).status(),
  ).toBe(403);
  expect(
    (
      await api(dev, "/database/prepare", "POST", {
        ...prepare,
        releaseId: installations[1].releaseId,
      })
    ).status(),
  ).toBe(400);
  const invalid = JSON.parse(
    readFileSync(
      ".local/artifacts/" + installations[0].artifactHash + "/manifest.json",
      "utf8",
    ),
  );
  invalid.contractVersion = "wrong";
  expect(
    (await api(dev, "/contracts/validate", "POST", invalid)).status(),
  ).toBe(400);
  const spec = await (await api(admin, "/openapi.json")).json();
  const catalog = await (await api(admin, "/catalog")).json();
  for (const entry of catalog)
    expect(spec.paths[entry.path][entry.method.toLowerCase()].operationId).toBe(
      entry.id,
    );
  const alice = await browser.newContext({ baseURL: origin });
  const alicePage = await alice.newPage();
  await login(alicePage, "alice");
  const bob = await browser.newContext({ baseURL: origin });
  const bobPage = await bob.newPage();
  await login(bobPage, "bob");
  expect(
    (await (await api(alice, "/installations")).json()).map((a: any) => a.id),
  ).toContain(installations[0].id);
  expect(
    (await (await api(bob, "/installations")).json()).map((a: any) => a.id),
  ).not.toContain(installations[0].id);
  expect(
    (
      await api(alice, "/database/prepare", "POST", {
        ...prepare,
        idempotencyKey: "alice-" + run,
      })
    ).status(),
  ).toBe(403);
  await alicePage.getByRole("button", { name: "로그아웃" }).click();
  await alicePage.waitForURL((url) =>
    url.pathname.includes("/protocol/openid-connect/logout"),
  );
  await expect(
    alicePage.getByText("Invalid redirect uri", { exact: false }),
  ).toHaveCount(0);
  if (await alicePage.locator("#kc-logout").count())
    await alicePage.locator("#kc-logout").click();
  await alicePage.waitForURL(origin + "/");
  await expect(alicePage.getByRole("link", { name: /로그인/ })).toBeVisible();
  await expect
    .poll(async () => (await alice.request.get(origin + "/api/v1/me")).status())
    .toBe(401);
  // Access tokens last 45s. This request must use a refreshed server-side session.
  await new Promise((r) => setTimeout(r, 47000));
  expect((await api(dev, "/me")).status()).toBe(200);
  expect(
    (
      await api(admin, "/grants", "PUT", {
        userId: userIds.developer,
        appId: appIds[0],
        environment: "dev",
        permission: "develop",
        enabled: false,
      })
    ).ok(),
  ).toBeTruthy();
  expect((await api(dev, "/jobs/" + job.id)).status()).toBe(403);
  const anon = await browser.newContext({ baseURL: origin });
  expect((await anon.request.get(origin + "/api/v1/me")).status()).toBe(401);
  expect(
    (
      await anon.request.post(origin + "/mcp", {
        data: { jsonrpc: "2.0", id: 1, method: "tools/list" },
      })
    ).status(),
  ).toBe(401);
  expect(treeHash("web/platform/dist")).toBe(hostHash);
  mkdirSync(".local/evidence", { recursive: true });
  writeFileSync(
    ".local/evidence/runtime.json",
    JSON.stringify(
      {
        run,
        hostHash,
        appIds,
        installations,
        prod,
        jobId: job.id,
        errors,
        passed: [
          "runtime-loading",
          "deep-link",
          "reload",
          "back",
          "app-switch",
          "no-iframe",
          "error-boundary",
          "permissions",
          "db-idempotency",
          "session-refresh",
          "grant-revocation",
          "unauthorized-rejection",
          "api-catalog-consistency",
          "alice-bob-separation",
          "use-role-write-denied",
          "web-logout",
        ],
      },
      null,
      2,
    ),
  );
  await page.goto(`/apps/${appIds[0]}`);
  await expect(page.getByTestId("app-root")).toBeVisible();
  await page.screenshot({
    path: ".local/evidence/runtime-ui.png",
    fullPage: true,
  });
  await admin.close();
  await dev.close();
  await anon.close();
  await alice.close();
  await bob.close();
});
