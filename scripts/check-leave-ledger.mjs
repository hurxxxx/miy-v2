// Independent acceptance: this script is outside the Codex app sandbox.
import assert from "node:assert/strict";
import { chromium } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, readdirSync } from "node:fs";
const origin = "http://localhost:18000",
  appId = "leave-ledger",
  year = 2026;
const passed = [];
const contexts = [];
let page;
function check(name, condition) {
  assert.ok(condition, name);
  passed.push(name);
  console.log("PASS " + name);
}
function treeHash(root) {
  const entries = [];
  function walk(path) {
    for (const e of readdirSync(path, { withFileTypes: true })) {
      const p = path + "/" + e.name;
      if (e.isDirectory()) walk(p);
      else
        entries.push([
          p.slice(root.length + 1),
          createHash("sha256").update(readFileSync(p)).digest("hex"),
        ]);
    }
  }
  walk(root);
  entries.sort(([a], [b]) => a.localeCompare(b));
  return createHash("sha256")
    .update(entries.map(([p, h]) => p + "\0" + h + "\n").join(""))
    .digest("hex");
}
const browser = await chromium.connectOverCDP("http://127.0.0.1:19222");
try {
  async function login(account) {
    const context = await browser.newContext();
    contexts.push(context);
    const r = await context.request.post(origin + "/auth/development", {
      headers: { origin },
      data: { account },
    });
    assert.equal(r.status(), 200);
    const me = await (await context.request.get(origin + "/api/v1/me")).json();
    return { context, me };
  }
  const admin = await login("admin"),
    alice = await login("alice"),
    bob = await login("bob"),
    developer = await login("developer");
  const installations = await (
    await admin.context.request.get(origin + "/api/v1/installations")
  ).json();
  const inst = installations.find(
    (i) => i.appId === appId && i.environment === "dev",
  );
  check(
    "docker-installation-running",
    inst?.runtimeStatus === "running" && inst.kind === "docker_app",
  );
  async function api(user, path, method = "GET", data, headers = {}) {
    return user.context.request.fetch(
      origin + `/api/v1/installations/${inst.id}/api` + path,
      {
        method,
        data,
        headers: {
          origin,
          "x-miy-csrf": user.me.csrf,
          "x-miy-release": inst.releaseId,
          ...headers,
        },
      },
    );
  }
  async function status(response, code, name) {
    assert.equal(
      response.status(),
      code,
      name + ": " + (await response.text()),
    );
    passed.push(name);
    console.log("PASS " + name);
    return response;
  }
  const base = await (await api(alice, "/summary?year=" + year)).json();
  const existing = await (await api(admin, "/leaves?year=" + year)).json();
  assert.ok(Array.isArray(existing));
  function date(d) {
    return d.toISOString().slice(0, 10);
  }
  function free(userId, weekday = 1, span = 0) {
    for (let n = 0; n < 55; n++) {
      const start = new Date(Date.UTC(year, 10, 1 + n));
      if (start.getUTCFullYear() !== year || start.getUTCDay() !== weekday)
        continue;
      const end = new Date(start);
      end.setUTCDate(end.getUTCDate() + span);
      if (
        !existing.some(
          (r) =>
            r.user_id === userId &&
            ["pending", "approved"].includes(r.status) &&
            r.start_date <= date(end) &&
            r.end_date >= date(start),
        )
      )
        return [date(start), date(end)];
    }
    throw Error("No free synthetic date");
  }
  const [start, end] = free(alice.me.id, 5, 3);
  const input = {
    employee_name: "김민수(검증)",
    department: "개발팀",
    leave_type: "annual",
    start_date: start,
    end_date: end,
    reason: "통합 검증용 휴가 신청",
  };
  const created = await (
    await status(
      await api(alice, "/leaves", "POST", input),
      201,
      "create-leave-in-app-db",
    )
  ).json();
  existing.push(created);
  check(
    "weekend-excluded-and-owner-from-identity",
    created.days === 2 &&
      created.user_id === alice.me.id &&
      created.status === "pending",
  );
  await status(
    await api(alice, "/leaves", "POST", input),
    409,
    "overlapping-leave-rejected",
  );
  await status(
    await api(alice, "/leaves", "POST", {
      ...input,
      user_id: bob.me.id,
      admin: true,
    }),
    400,
    "body-identity-forgery-rejected",
  );
  await status(
    await api(alice, "/leaves", "POST", {
      ...input,
      start_date: "2026-11-07",
      end_date: "2026-11-08",
    }),
    400,
    "weekend-only-leave-rejected",
  );
  await status(
    await api(alice, "/leaves", "POST", {
      ...input,
      leave_type: "morning",
      end_date: end,
    }),
    400,
    "multi-day-half-leave-rejected",
  );
  for (const method of ["GET", "PATCH", "DELETE"])
    await status(
      await api(
        bob,
        "/leaves/" + created.id,
        method,
        method === "PATCH" ? input : undefined,
      ),
      403,
      "other-user-" + method.toLowerCase() + "-denied",
    );
  const bobRows = await (await api(bob, "/leaves?year=" + year)).json();
  check("other-user-list-isolated", !bobRows.some((r) => r.id === created.id));
  await status(
    await api(alice, "/leaves/" + created.id + "/decision", "POST", {
      status: "approved",
    }),
    403,
    "non-admin-decision-denied",
  );
  await status(
    await api(admin, "/leaves/" + created.id + "/decision", "POST", {
      status: "approved",
    }),
    200,
    "admin-approval",
  );
  await status(
    await api(alice, "/leaves/" + created.id, "PATCH", input),
    409,
    "approved-record-update-rejected",
  );
  await status(
    await api(alice, "/leaves/" + created.id, "DELETE"),
    409,
    "approved-record-delete-rejected",
  );
  const after = await (await api(alice, "/summary?year=" + year)).json();
  check(
    "annual-balance-from-real-data",
    after.approved === base.approved + 2 &&
      after.remaining === base.remaining - 2,
  );
  const [bs, be] = free(bob.me.id, 2);
  const half = {
    ...input,
    employee_name: "박서연(검증)",
    department: "운영팀",
    leave_type: "morning",
    start_date: bs,
    end_date: be,
  };
  const b = await (
    await status(
      await api(bob, "/leaves", "POST", half),
      201,
      "half-day-create",
    )
  ).json();
  check("half-day-is-point-five", b.days === 0.5);
  existing.push(b);
  await status(
    await api(admin, "/leaves/" + b.id + "/decision", "POST", {
      status: "rejected",
    }),
    400,
    "rejection-requires-reason",
  );
  const rejected = await (
    await status(
      await api(admin, "/leaves/" + b.id + "/decision", "POST", {
        status: "rejected",
        rejection_reason: "검증용 일정 조정",
      }),
      200,
      "admin-rejection",
    )
  ).json();
  check("rejection-recorded", rejected.rejection_reason === "검증용 일정 조정");
  const [ts, te] = free(alice.me.id, 2);
  const temp = { ...input, start_date: ts, end_date: te };
  const t = await (await api(alice, "/leaves", "POST", temp)).json();
  assert.ok(t.id);
  await status(
    await api(alice, "/leaves/" + t.id, "PATCH", {
      ...temp,
      reason: "수정 검증",
    }),
    200,
    "pending-record-update",
  );
  await status(
    await api(alice, "/leaves/" + t.id, "DELETE"),
    200,
    "pending-record-delete",
  );
  await status(
    await api(alice, "/leaves/" + t.id),
    404,
    "deleted-record-absent",
  );
  const parallel = await Promise.all([
    api(alice, "/leaves", "POST", temp),
    api(alice, "/leaves", "POST", temp),
  ]);
  check(
    "concurrent-overlap-serialized",
    parallel
      .map((r) => r.status())
      .sort()
      .join(",") === "201,409",
  );
  const pending = await parallel.find((r) => r.status() === 201).json();
  await status(
    await api(alice, "/leaves/" + pending.id, "PATCH", temp, {
      "x-miy-release": "00000000-0000-4000-8000-000000000000",
    }),
    409,
    "stale-ui-mutation-rejected",
  );
  await status(
    await api(alice, "/leaves/" + pending.id, "PATCH", temp, {
      origin: "https://untrusted.example",
    }),
    403,
    "cross-origin-mutation-rejected",
  );
  const denied = await developer.context.request.post(
    origin + `/api/v1/apps/${appId}/runtime`,
    {
      headers: { origin, "x-miy-csrf": developer.me.csrf },
      data: {
        action: "start",
        environment: "prod",
        idempotencyKey: crypto.randomUUID(),
      },
    },
  );
  await status(denied, 403, "developer-production-control-denied");
  const name = "miy-app-" + inst.id.replaceAll("-", "");
  const meta = JSON.parse(
    execFileSync("docker", ["inspect", name], { encoding: "utf8" }),
  )[0];
  const port = meta.NetworkSettings.Ports["8080/tcp"][0].HostPort;
  await status(
    await alice.context.request.get(`http://127.0.0.1:${port}/api/me`, {
      headers: { "x-miy-user-id": alice.me.id, "x-miy-admin": "true" },
    }),
    401,
    "direct-container-forged-identity-rejected",
  );
  check("running-image-equals-deployed-image", meta.Image === inst.imageId);
  check(
    "container-runtime-limits",
    meta.HostConfig.ReadonlyRootfs &&
      meta.HostConfig.Memory === 268435456 &&
      meta.HostConfig.NanoCpus === 1000000000 &&
      meta.HostConfig.PidsLimit === 128 &&
      meta.Config.User === "10001:10001" &&
      meta.HostConfig.CapDrop.includes("ALL"),
  );
  // This is an actual browser, not an API-only rendering test.
  page = await admin.context.newPage();
  await page.goto(origin + "/apps/" + appId);
  await page
    .getByRole("heading", { name: "휴가 관리 대장", exact: true })
    .waitFor();
  await page
    .getByRole("row")
    .filter({ hasText: input.employee_name })
    .first()
    .waitFor();
  check(
    "browser-remote-ui-no-iframe",
    (await page.locator("iframe").count()) === 0,
  );
  await page.getByRole("button", { name: "+ 휴가 신청", exact: true }).click();
  const [us, ue] = free(admin.me.id, 3);
  const uiName = "이수진(화면 검증)";
  await page.getByLabel("직원명", { exact: true }).fill(uiName);
  await page.getByLabel("부서", { exact: true }).fill("경영지원팀");
  await page.getByLabel("시작일", { exact: true }).fill(us);
  await page.getByLabel("종료일", { exact: true }).fill(ue);
  await page
    .getByLabel("사유", { exact: true })
    .fill("실제 브라우저 신청과 영속 저장 검증");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  const uiRow = page.getByRole("row").filter({ hasText: uiName });
  await uiRow.first().waitFor();
  await uiRow.first().getByRole("link", { name: "상세 보기" }).click();
  await page
    .getByRole("heading", { name: "휴가 신청 상세", exact: true })
    .waitFor();
  const detailUrl = page.url();
  await page.reload();
  await page
    .getByRole("heading", { name: "휴가 신청 상세", exact: true })
    .waitFor();
  check("browser-create-detail-and-reload", page.url() === detailUrl);
  await page.screenshot({
    path: ".local/evidence/leave-detail.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "대장 목록", exact: true }).click();
  await page.getByRole("row").filter({ hasText: uiName }).first().waitFor();
  await page.screenshot({
    path: ".local/evidence/leave-ledger.png",
    fullPage: true,
  });
  // Exercise lifecycle from the platform web UI and verify the same DB afterwards.
  await page.goto(origin);
  const card = page
    .getByRole("heading", { name: "휴가 관리 대장", exact: true })
    .locator("..");
  await card.getByRole("button", { name: "중지", exact: true }).click();
  async function runtimeStatus() {
    const items = await (
      await admin.context.request.get(origin + "/api/v1/installations")
    ).json();
    return items.find((i) => i.id === inst.id)?.runtimeStatus;
  }
  async function waitStatus(target) {
    for (let n = 0; n < 100; n++) {
      if ((await runtimeStatus()) === target) return;
      await new Promise((r) => setTimeout(r, 300));
    }
    throw Error("Runtime did not become " + target);
  }
  await waitStatus("stopped");
  await status(
    await api(alice, "/leaves/" + created.id),
    503,
    "stopped-app-api-unavailable",
  );
  await card.getByRole("button", { name: "시작", exact: true }).waitFor();
  await card.getByRole("button", { name: "시작", exact: true }).click();
  await waitStatus("running");
  const preserved = await (await api(alice, "/leaves/" + created.id)).json();
  check(
    "web-start-stop-and-data-preservation",
    preserved.id === created.id && preserved.status === "approved",
  );
  await card.getByRole("button", { name: "재시작", exact: true }).waitFor();
  await card.getByRole("button", { name: "재시작", exact: true }).click();
  await waitStatus("running");
  // Wait for the actual control to finish, rather than observing the pre-restart status.
  for (let n = 0; n < 60; n++) {
    if (
      await card
        .getByRole("button", { name: "재시작", exact: true })
        .isEnabled()
    )
      break;
    await new Promise((r) => setTimeout(r, 500));
  }
  check(
    "web-restart-data-preservation",
    (await (await api(alice, "/leaves/" + created.id)).json()).id ===
      created.id,
  );
  // Real OAuth remains a separate path from the convenient development buttons.
  const oauth = await browser.newContext();
  contexts.push(oauth);
  const oauthPage = await oauth.newPage();
  await oauthPage.goto("https://localhost:18443/auth/login");
  const credentials = JSON.parse(
    readFileSync(".local/credentials.json", "utf8"),
  );
  await oauthPage.locator("#username").fill("alice");
  await oauthPage.locator("#password").fill(credentials.alice);
  await oauthPage.locator("#kc-login").click();
  await oauthPage.waitForURL("https://localhost:18443/");
  const normal = await (
    await oauth.request.get(
      `https://localhost:18443/api/v1/installations/${inst.id}/api/leaves/${created.id}`,
    )
  ).json();
  check("real-oauth-session-to-docker-app", normal.id === created.id);
  const before = JSON.parse(
    readFileSync(".local/evidence/leave-platform-before.json", "utf8"),
  );
  check(
    "platform-build-unchanged-after-app-deployment",
    treeHash("web/platform/dist") === before.hostHash,
  );
  const result = {
    date: new Date().toISOString(),
    appId,
    installationId: inst.id,
    releaseId: inst.releaseId,
    imageId: inst.imageId,
    sourceCommit: inst.sourceCommit,
    hostHash: before.hostHash,
    container: {
      name,
      imageId: meta.Image,
      user: meta.Config.User,
      memoryBytes: meta.HostConfig.Memory,
      nanoCpus: meta.HostConfig.NanoCpus,
      pidsLimit: meta.HostConfig.PidsLimit,
      readOnlyRoot: meta.HostConfig.ReadonlyRootfs,
      network: meta.HostConfig.NetworkMode,
    },
    records: { approved: created.id, rejected: b.id, pending: pending.id },
    passed,
  };
  writeFileSync(
    ".local/evidence/leave-ledger.json",
    JSON.stringify(result, null, 2),
  );
  console.log(`Acceptance passed: ${passed.length} checks.`);
} catch (error) {
  if (page)
    await page
      .screenshot({ path: ".local/evidence/leave-failure.png", fullPage: true })
      .catch(() => {});
  throw error;
} finally {
  for (const context of contexts) await context.close();
  await browser.close();
}
