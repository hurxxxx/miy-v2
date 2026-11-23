import { browserUser, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";
import { openNewTab } from "./new-tab";
import { mockAdminUserPage } from "./admin-users-fixture";

// Admin UI for users, groups and app access with mocked APIs. These checks cover the
// requests the screen sends and what it shows; server authorization is covered by the
// isolated permissions regression.
const admin = {
  id: "admin",
  name: "관리자",
  admin: true,
  csrf: "fixture-csrf",
};
type Group = {
  id: string;
  name: string;
  source: "hr" | "local";
  parentId: string | null;
  active: boolean;
  members: number;
};

async function mock(page: Page) {
  await mockSurface(page, "console");
  const sent: {
    method: string;
    path: string;
    body: unknown;
    csrf: string | null;
  }[] = [];
  const groups: Group[] = [
    {
      id: "g-hr",
      name: "본부",
      source: "hr",
      parentId: null,
      active: true,
      members: 0,
    },
  ];
  const members: Record<string, { id: string; name: string; enabled: boolean }[]> = { "g-hr": [] };
  const access = {
    appId: "ledger",
    disabled: false,
    admission: { dev: "selected", prod: "selected" },
    userGrants: [{ userId: "alice", environment: "dev", permission: "use" }],
    groupGrants: [] as {
      groupId: string;
      environment: string;
      permission: string;
    }[],
  };
  const accounts = [
    { id: "admin", name: "관리자", admin: true, canDevelop: false, enabled: true, primaryGroupId: null as string | null },
    { id: "alice", name: "앨리스", admin: false, canDevelop: false, enabled: true, primaryGroupId: null as string | null },
  ];
  const apps = [{ id: "ledger", name: "장부" }];
  // Account links (GET/POST /api/v1/account-links); a test may turn the feature off.
  const links = {
    available: true,
    items: [] as Record<string, unknown>[],
  };
  const accessByApp: Record<string, typeof access> = { ledger: access };
  // Administrator runtime list (GET /api/v1/runtime/installations); empty unless a test fills it.
  const managed: Record<string, unknown>[] = [];
  const auditQueries: URLSearchParams[] = [];
  const audit = [
    {
      id: 3,
      at: "2026-11-03T03:00:00Z",
      actor: "admin",
      actorName: "관리자",
      action: "group.member_added",
      target: "group:g-hr:user:alice",
      requestId: "r3",
    },
    {
      id: 2,
      at: "2026-11-03T02:00:00Z",
      actor: "admin",
      actorName: "관리자",
      action: "POST:200",
      target: "/api/v1/groups",
      requestId: "r2",
    },
    {
      id: 1,
      at: "2026-11-03T01:00:00Z",
      actor: "admin",
      actorName: "관리자",
      action: "app.disabled",
      target: "app:ledger",
      requestId: "r1",
    },
  ];
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();
    if (path === "/api/v1/users/directory" && method === "GET") {
      return route.fulfill({ json: mockAdminUserPage(accounts, groups, url.searchParams) });
    }
    if (path === "/api/v1/audit") {
      auditQueries.push(url.searchParams);
      const cursor = url.searchParams.get("cursor");
      const matching = audit.filter((e) =>
        (!cursor || e.id < Number(cursor)) &&
        (!url.searchParams.get("actor") || e.actor === url.searchParams.get("actor")) &&
        (!url.searchParams.get("action") || e.action.startsWith(url.searchParams.get("action")!)),
      );
      const page = matching.slice(0, 2);
      return route.fulfill({
        status: 200,
        json: { entries: page, nextCursor: matching.length > 2 ? page.at(-1)!.id : null },
      });
    }
    if (method !== "GET") {
      const body = request.postDataJSON();
      sent.push({
        method,
        path,
        body,
        csrf: await request.headerValue("x-miy-csrf"),
      });
      if (path === "/api/v1/groups" && method === "POST") {
        const id = `g-${groups.length}`;
        groups.push({
          id,
          name: body.name,
          source: body.source,
          parentId: body.parentId,
          active: true,
          members: 0,
        });
        members[id] = [];
        return route.fulfill({ status: 200, json: { id } });
      }
      const parent = path.match(/^\/api\/v1\/groups\/([^/]+)\/parent$/);
      if (parent) groups.find((g) => g.id === parent[1])!.parentId = body.parentId;
      const group = path.match(/^\/api\/v1\/groups\/([^/]+)$/);
      if (group && method === "PATCH") Object.assign(groups.find((g) => g.id === group[1])!, body);
      const member = path.match(/^\/api\/v1\/groups\/([^/]+)\/members$/);
      if (member) {
        const list = members[member[1]];
        if (body.member)
          list.push({
            id: body.userId,
            name: body.userId === "alice" ? "앨리스" : body.userId,
            enabled: true,
          });
        else
          list.splice(
            list.findIndex((m) => m.id === body.userId),
            1,
          );
        groups.find((g) => g.id === member[1])!.members = list.length;
        return route.fulfill({ status: 200, json: { updated: true } });
      }
      const primary = path.match(/^\/api\/v1\/users\/([^/]+)\/primary-group$/);
      if (primary) {
        const userId = primary[1];
        const account = accounts.find((item) => item.id === userId);
        const current = groups.find((g) => g.source === "hr" && members[g.id].some((m) => m.id === userId));
        if ("expectedGroupId" in body && (current?.id ?? null) !== body.expectedGroupId)
          return route.fulfill({ status: 409, json: { error: { code: "primary_group_changed" } } });
        const details = account ?? members[current?.id ?? ""]?.find((m) => m.id === userId);
        if (current) {
          members[current.id] = members[current.id].filter((m) => m.id !== userId);
          current.members = members[current.id].length;
        }
        if (body.groupId && details) {
          members[body.groupId].push({ id: userId, name: details.name, enabled: details.enabled });
          groups.find((g) => g.id === body.groupId)!.members = members[body.groupId].length;
        }
        if (account) Object.assign(account, { primaryGroupId: body.groupId });
        return route.fulfill({ json: { userId, groupId: body.groupId } });
      }
      if (path === "/api/v1/account-links" && method === "POST") {
        if (!links.available)
          return route.fulfill({ status: 503, json: { error: { code: "account_links_unavailable" } } });
        const id = `link-${links.items.length + 1}`;
        const invitation = body.kind === "invitation";
        const target = accounts.find((item) => item.id === body.userId);
        links.items.unshift({
          id,
          kind: body.kind,
          name: invitation ? body.name : target!.name,
          canDevelop: invitation && body.canDevelop === true,
          userId: invitation ? null : body.userId,
          state: "pending",
          createdByName: "관리자",
          createdAt: "2026-11-20T01:00:00Z",
          expiresAt: "2026-11-27T01:00:00Z",
          completedAt: null,
        });
        return route.fulfill({
          status: 201,
          json: {
            id,
            kind: body.kind,
            name: invitation ? body.name : null,
            canDevelop: invitation && body.canDevelop === true,
            userId: invitation ? null : body.userId,
            expiresAt: "2026-11-27T01:00:00Z",
            url: `${url.origin}/${invitation ? "join" : "reset-password"}#${"s".repeat(42)}${links.items.length}`,
          },
        });
      }
      const revoked = path.match(/^\/api\/v1\/account-links\/([^/]+)\/revoke$/);
      if (revoked) {
        links.items.find((item) => item.id === revoked[1])!.state = "revoked";
        return route.fulfill({ status: 200, json: { id: revoked[1], state: "revoked" } });
      }
      if (path === "/api/v1/users" && method === "POST") {
        if (accounts.some((account) => account.id === body.id))
          return route.fulfill({ status: 409, json: { error: { code: "user_exists" } } });
        const created = { id: body.id, name: body.name, admin: false, canDevelop: body.canDevelop, enabled: true, primaryGroupId: null };
        accounts.push(created);
        return route.fulfill({ status: 200, json: created });
      }
      const account = path.match(/^\/api\/v1\/users\/([^/]+)(\/state)?$/);
      if (account) {
        const target = accounts.find((item) => item.id === account[1])!;
        if (body.admin === false && accounts.filter((item) => item.admin && item.enabled).length <= 1)
          return route.fulfill({ status: 409, json: { error: { code: "last_enabled_administrator" } } });
        Object.assign(target, body);
        return route.fulfill({ status: 200, json: target });
      }
      if (path.endsWith("/runtime/preflight"))
        return route.fulfill({ status: 200, json: { canSubmit: true, expectedState: {}, changes: [] } });
      if (path.endsWith("/runtime")) return route.fulfill({ status: 200, json: { id: "runtime-job" } });
      if (path === "/api/v1/apps/ledger/control") access.disabled = body.disabled;
      if (path === "/api/v1/apps/ledger/admission") access.admission[body.environment as "dev" | "prod"] = body.mode;
      if (path === "/api/v1/group-grants") {
        if (body.permission === "deploy")
          return route.fulfill({
            status: 400,
            json: { error: { code: "group_permission_not_allowed" } },
          });
        const target = accessByApp[body.appId];
        if (body.enabled)
          target.groupGrants.push({
            groupId: body.groupId,
            environment: body.environment,
            permission: body.permission,
          });
        else
          target.groupGrants = target.groupGrants.filter(
            (g) => g.groupId !== body.groupId || g.environment !== body.environment || g.permission !== body.permission,
          );
      }
      if (path === "/api/v1/grants" && accessByApp[body.appId]) {
        const target = accessByApp[body.appId];
        target.userGrants = target.userGrants.filter(
          (g) => g.userId !== body.userId || g.environment !== body.environment || g.permission !== body.permission,
        );
        if (body.enabled) target.userGrants.push({
          userId: body.userId,
          environment: body.environment,
          permission: body.permission,
        });
      }
      return route.fulfill({ status: 200, json: { updated: true } });
    }
    const appView = path.match(/^\/api\/v1\/apps\/([^/]+)$/);
    if (appView) {
      const app = apps.find(({ id }) => id === appView[1]);
      return route.fulfill(app ? {
        status: 200,
        json: {
          app: { ...app, distribution: "organization", disabled: accessByApp[app.id]?.disabled ?? false },
          installations: managed.filter((installation) => installation.appId === app.id),
        },
      } : { status: 404, json: { error: { code: "not_found" } } });
    }
    if (path === "/api/v1/group-grants")
      return route.fulfill({ json: Object.entries(accessByApp).flatMap(([appId, value]) =>
        value.groupGrants.map((grant) => ({ appId, ...grant }))) });
    const reply =
      path === "/api/v1/me"
        ? browserUser(route, admin)
        : path === "/api/v1/installations"
          ? []
          : path === "/api/v1/runtime/installations"
            ? managed
            : path === "/api/v1/runtime/jobs/runtime-job"
              ? { status: "succeeded", step: "completed" }
              : path === "/api/v1/app-registrations"
            ? []
            : path === "/api/v1/account-links"
              ? { available: links.available, links: links.items }
            : path === "/api/v1/users"
              ? accounts
              : path === "/api/v1/groups"
                ? groups
                : path.match(/^\/api\/v1\/groups\/[^/]+\/members$/)
                  ? members[path.split("/")[4]]
                  : path === "/api/v1/apps"
                    ? { apps, nextCursor: null }
                    : path.match(/^\/api\/v1\/apps\/[^/]+\/access$/)
                      ? accessByApp[path.split("/")[4]]
                      : null;
    if (reply === null)
      return route.fulfill({
        status: 404,
        json: { error: { code: "not_found" } },
      });
    await route.fulfill({ status: 200, json: reply });
  });
  return {
    accounts,
    links,
    sent,
    access,
    managed,
    auditQueries,
    groups,
    members,
    apps,
    accessByApp,
    audit,
  };
}

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function renderResponses(page: Page) {
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

test("an administrator creates a local group and manages its members", async ({ page }) => {
  const { sent, access } = await mock(page);
  await page.goto("/admin/groups");
  await page.getByRole("button", { name: "그룹 만들기" }).click();
  await page.getByLabel("이름", { exact: true }).fill("영업 1팀");
  await page.getByRole("button", { name: "만들기", exact: true }).click();
  await expect(page.getByRole("status")).toHaveText("그룹을 만들었습니다.");
  expect(sent.at(-1)).toMatchObject({
    method: "POST",
    path: "/api/v1/groups",
    body: { name: "영업 1팀", source: "local", parentId: null },
    csrf: "fixture-csrf",
  });
  await expect(page).toHaveURL(/\/admin\/groups\?group=g-1$/);
  const detail = page.getByRole("region", { name: "영업 1팀 구성원" });
  await detail.getByRole("combobox", { name: "추가할 사용자" }).fill("앨리스");
  await detail.getByRole("option", { name: "앨리스 (alice)", exact: true }).click();
  await detail.getByRole("button", { name: "추가", exact: true }).click();
  await expect(detail.getByRole("listitem")).toContainText("앨리스");
  await detail.getByRole("button", { name: "빼기" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "빼기" }).click();
  await expect(detail.getByText("구성원이 없습니다.")).toBeVisible();
  expect(sent.filter((s) => s.path.endsWith("/members")).map((s) => (s.body as { member: boolean }).member)).toEqual([true, false]);

  access.groupGrants.push({ groupId: "g-1", environment: "dev", permission: "use" });
  await page.reload();
  await expect(page.getByRole("link", { name: "ledger" })).toHaveAttribute("href", "/admin/apps/ledger/access");
  await page.getByRole("button", { name: "비활성화", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "비활성화", exact: true }).click();
  await expect(page.getByRole("status")).toHaveText("비활성화했습니다.");
});

test("HR groups take members through the primary group, not manual membership", async ({ page }) => {
  const { sent, accounts } = await mock(page);
  accounts.push({ ...accounts[1], id: "alice-other" });
  await page.goto("/admin/organization?group=g-hr");
  const detail = page.getByRole("region", { name: "본부 구성원" });
  const picker = detail.getByRole("combobox", { name: "주 소속으로 지정할 사용자" });
  await picker.fill("앨리스");
  await detail.getByRole("option", { name: "앨리스 (alice)", exact: true }).click();
  await detail.getByRole("option", { name: "앨리스 (alice-other)", exact: true }).click();
  await detail.getByRole("button", { name: "주 소속 지정" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("앨리스 (alice)");
  await expect(dialog).toContainText("앨리스 (alice-other)");
  await dialog.getByRole("button", { name: "주 소속 지정" }).click();
  const first = detail.getByRole("listitem").filter({ hasText: "앨리스 (alice)" });
  const other = detail.getByRole("listitem").filter({ hasText: "앨리스 (alice-other)" });
  await expect(first).toBeVisible();
  await expect(other).toBeVisible();
  expect(sent.slice(-2)).toMatchObject([
    { method: "PUT", path: "/api/v1/users/alice/primary-group", body: { groupId: "g-hr", expectedGroupId: null } },
    { method: "PUT", path: "/api/v1/users/alice-other/primary-group", body: { groupId: "g-hr", expectedGroupId: null } },
  ]);
  await other.getByRole("button", { name: "주 소속 해제" }).click();
  await expect(dialog).toContainText("앨리스 (alice-other)");
  await dialog.getByRole("button", { name: "주 소속 해제" }).click();
  await expect(other).toHaveCount(0);
  await expect(first).toBeVisible();
  expect(sent.at(-1)).toMatchObject({
    path: "/api/v1/users/alice-other/primary-group",
    body: { groupId: null, expectedGroupId: "g-hr" },
  });
  await expect(page.getByRole("link", { name: "이 조직의 사용자 보기" })).toHaveAttribute("href", "/admin/users?organization=g-hr");
});

test("a stale primary assignment requires selecting the user again with the current organization", async ({ page }) => {
  const { sent, accounts, groups, members } = await mock(page);
  groups.push({ id: "g-other", name: "다른 조직", source: "hr", parentId: null, active: true, members: 0 });
  members["g-other"] = [];
  await page.goto("/admin/organization?group=g-hr");
  const detail = page.getByRole("region", { name: "본부 구성원" });
  const picker = detail.getByRole("combobox", { name: "주 소속으로 지정할 사용자" });
  const assign = detail.getByRole("button", { name: "주 소속 지정" });
  const dialog = page.getByRole("dialog");
  await picker.fill("앨리스");
  await detail.getByRole("option", { name: "앨리스 (alice)", exact: true }).click();
  await assign.click();
  accounts[1].primaryGroupId = "g-other";
  members["g-other"].push({ id: "alice", name: "앨리스", enabled: true });
  await dialog.getByRole("button", { name: "주 소속 지정" }).click();
  await expect(detail.getByRole("alert")).toContainText("사용자의 주 소속이 변경되었습니다.");
  await expect(assign).toBeDisabled();
  expect(sent.at(-1)?.body).toEqual({ groupId: "g-hr", expectedGroupId: null });
  expect(accounts[1].primaryGroupId).toBe("g-other");

  await picker.click();
  await detail.getByRole("option", { name: /앨리스 \(alice\).*다른 조직/ }).click();
  await assign.click();
  await expect(dialog).toContainText("앨리스 (alice) · 다른 조직");
  await dialog.getByRole("button", { name: "주 소속 지정" }).click();
  await expect(detail.getByRole("listitem")).toContainText("앨리스 (alice)");
  expect(sent.at(-1)?.body).toEqual({ groupId: "g-hr", expectedGroupId: "g-other" });
});

test("the company switch asks first, and groups can receive use or develop but never deploy", async ({ page }) => {
  const { sent, access, groups, accounts } = await mock(page);
  accounts.push({ ...accounts[1], id: "alice-other" });
  access.userGrants.push({ userId: "alice-other", environment: "prod", permission: "deploy" });
  groups.push({ id: "inactive", name: "비활성 그룹", source: "local", parentId: null, active: false, members: 0 });
  await page.goto("/admin/apps");
  await page.getByRole("link", { name: "장부", exact: true }).click();
  await expect(page).toHaveURL(/\/admin\/apps\/ledger$/);
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "앱 접근" }) });
  await expect(panel.getByText("사용 중", { exact: true })).toBeVisible();
  await expect(panel.getByText("앨리스 (alice) · 테스트 · 사용")).toBeVisible();

  await panel.getByRole("button", { name: "사용 중지" }).click();
  const dialog = page.getByRole("dialog", { name: "앱 사용을 중지할까요?" });
  await expect(dialog).toContainText("관리자를 포함한 모든 사용자");
  await page.keyboard.press("Escape");
  expect(sent.some((s) => s.path.endsWith("/control"))).toBe(false);
  await panel.getByRole("button", { name: "사용 중지" }).click();
  await dialog.getByRole("button", { name: "사용 중지" }).click();
  await expect(panel.getByText("중지됨")).toBeVisible();
  expect(access.disabled).toBe(true);

  await panel.getByLabel("운영 입장 대상").selectOption("all");
  await expect.poll(() => access.admission.prod).toBe("all");

  const groupForm = panel.locator("form").filter({ has: page.getByRole("combobox", { name: "그룹", exact: true }) });
  const groupSelect = groupForm.getByRole("combobox", { name: "그룹", exact: true });
  await groupSelect.selectOption("g-hr");
  await expect(groupSelect.locator("option[value=inactive]")).toHaveCount(0);
  const permission = groupForm.getByLabel("권한");
  await expect(permission.locator("option")).toHaveCount(2);
  await expect(permission.locator("option[value=deploy]")).toHaveCount(0);
  await permission.selectOption("develop");
  await groupForm.getByRole("button", { name: "부여" }).click();
  await expect(panel.getByText("본부 · 테스트 · 개발")).toBeVisible();
  expect(sent.at(-1)).toMatchObject({
    path: "/api/v1/group-grants",
    body: {
      groupId: "g-hr",
      appId: "ledger",
      environment: "dev",
      permission: "develop",
      enabled: true,
    },
  });
  const userForm = panel.locator("form").filter({ has: page.getByRole("combobox", { name: "사용자", exact: true }) });
  const userSelect = userForm.getByRole("combobox", { name: "사용자", exact: true });
  await expect(userSelect.getByRole("option", { name: "앨리스 (alice-other)", exact: true })).toHaveAttribute("value", "alice-other");
  await userSelect.selectOption({ label: "앨리스 (alice)" });
  await userForm.getByRole("combobox", { name: "환경", exact: true }).selectOption("prod");
  await userForm.getByRole("combobox", { name: "권한", exact: true }).selectOption("deploy");
  await userForm.getByRole("button", { name: "부여" }).click();
  const deploy = panel.getByRole("listitem").filter({ hasText: "앨리스 (alice) · 운영 · 배포" });
  await expect(deploy).toBeVisible();
  expect(sent.at(-1)).toMatchObject({
    path: "/api/v1/grants",
    body: { appId: "ledger", userId: "alice", environment: "prod", permission: "deploy", enabled: true },
  });
  await deploy.getByRole("button", { name: "회수" }).click();
  await expect(deploy).toHaveCount(0);
  await expect(panel.getByText("앨리스 (alice-other) · 운영 · 배포")).toBeVisible();
  expect(sent.at(-1)).toMatchObject({
    path: "/api/v1/grants",
    body: { appId: "ledger", userId: "alice", environment: "prod", permission: "deploy", enabled: false },
  });
});

test("the audit log pages backwards and filters by actor, action and period", async ({ page }) => {
  const { auditQueries, audit } = await mock(page);
  await page.goto("/admin/audit");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "감사 기록" }) });
  const rows = panel.getByRole("row");
  await expect(rows).toHaveCount(3);
  await expect(panel.getByText("구성원 추가")).toBeVisible();
  await expect(panel.getByText("요청 POST · 200")).toBeVisible();
  await panel.getByRole("button", { name: "더 보기" }).click();
  await expect(panel.getByText("앱 사용 중지")).toBeVisible();
  await expect(panel.getByRole("button", { name: "더 보기" })).toHaveCount(0);
  expect(auditQueries.at(-1)?.get("cursor")).toBe("2");

  await panel.getByLabel("행위자").selectOption("alice");
  await panel.getByLabel("동작").selectOption("app.");
  await panel.getByLabel("시작일").fill("2026-11-01");
  await panel.getByLabel("종료일").fill("2026-11-03");
  await panel.getByRole("button", { name: "조회" }).click();
  await expect.poll(() => auditQueries.at(-1)?.get("actor")).toBe("alice");
  const last = auditQueries.at(-1)!;
  expect(last.get("action")).toBe("app.");
  expect(last.get("cursor")).toBeNull();
  expect(new Date(last.get("to")!).getTime() - new Date(last.get("from")!).getTime()).toBe(3 * 24 * 3600 * 1000);

  audit.unshift(
    { id: 5, at: "2026-11-03T05:00:00Z", actor: "admin", actorName: "관리자", action: "grant.disabled", target: "user:alice:app:ledger:prod:deploy:from:true:to:false", requestId: "r5" },
    { id: 4, at: "2026-11-03T04:00:00Z", actor: "admin", actorName: "관리자", action: "grant.enabled", target: "user:alice:app:ledger:prod:deploy:from:false:to:true", requestId: "r4" },
  );
  await panel.getByLabel("행위자").selectOption("");
  await panel.getByLabel("동작").selectOption("grant.");
  await panel.getByRole("button", { name: "조회", exact: true }).click();
  await expect(panel.getByText("사용자 권한 부여", { exact: true })).toBeVisible();
  await expect(panel.getByText("사용자 권한 회수", { exact: true })).toBeVisible();
  await expect(rows).toHaveCount(3);
  expect(auditQueries.at(-1)?.get("action")).toBe("grant.");

  audit.unshift(
    { id: 7, at: "2026-11-03T07:00:00Z", actor: "admin", actorName: "관리자", action: "user.enabled", target: "user:alice:from:false:to:true", requestId: "r7" },
    { id: 6, at: "2026-11-03T06:00:00Z", actor: "admin", actorName: "관리자", action: "user.disabled", target: "user:alice:from:true:to:false", requestId: "r6" },
  );
  await expect(panel.getByLabel("동작").locator("option", { hasText: "사용자 계정·주 소속" })).toHaveAttribute("value", "user.");
  await panel.getByLabel("동작").selectOption("user.");
  await panel.getByRole("button", { name: "조회", exact: true }).click();
  await expect(panel.getByText("사용자 다시 사용", { exact: true })).toBeVisible();
  await expect(panel.getByText("사용자 비활성화", { exact: true })).toBeVisible();
  expect(auditQueries.at(-1)?.get("action")).toBe("user.");
});

test("a company-disabled app stays in runtime management but is not offered for opening", async ({ page }) => {
  const { sent, managed, access } = await mock(page);
  access.disabled = true;
  const ledger = {
    appId: "ledger",
    name: "장부",
    releaseId: "release-1",
    version: "1.0.0",
    dbStatus: "ready",
    runtimeStatus: "running",
    desiredState: "running",
    kind: "docker_app",
    canManage: true,
    appDisabled: true,
    ui: { entry: "/unused-manifest.json", export: "./AppRoot", contractVersion: "miy.ui/v1alpha1" },
  };
  managed.push(
    { ...ledger, id: "ledger-dev", environment: "dev", canOpen: true },
    { ...ledger, id: "ledger-prod", environment: "prod", canOpen: false },
  );
  await page.goto("/admin/apps/ledger/runtime");
  const panel = (title: string) =>
    page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: title, exact: true }) });
  const prod = panel("운영");
  await expect(prod.getByText("회사 사용 중지", { exact: true })).toBeVisible();
  await expect(prod.getByRole("link", { name: "포털에서 앱 열기" })).toHaveCount(0);
  await expect(prod).toContainText("회사 사용 중지 중이라 열 수 없습니다.");
  await expect(panel("테스트").getByRole("link", { name: "포털에서 앱 열기" })).toBeVisible();
  const openDev = panel("테스트").getByRole("link", { name: "포털에서 앱 열기" });
  // Dev apps open on the dev portal origin.
  await page.context().route("https://dev-portal.example.test/**", (route) =>
    route.fulfill({ contentType: "text/html", body: "<p>업무 앱</p>" }),
  );
  await (await openNewTab(page, openDev)).close();

  await prod.getByRole("button", { name: "중지", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "중지", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect.poll(() => sent.filter((s) => s.path === "/api/v1/apps/ledger/runtime").length).toBe(1);
  expect(sent.find((s) => s.path === "/api/v1/apps/ledger/runtime")).toMatchObject({
    body: { action: "stop", environment: "prod", confirmed: true },
    csrf: "fixture-csrf",
  });
  await expect(prod.getByText("Docker 작업을 처리하고 있습니다…")).toHaveCount(0);
  await expect(prod.getByRole("alert")).toHaveCount(0);
});

test("switching HR groups uses the selected group's parent, including after a change", async ({ page }) => {
  const { groups, members, sent } = await mock(page);
  groups.push(
    { id: "g-a", name: "조직 A", source: "hr", parentId: "g-hr", active: true, members: 0 },
    { id: "g-b", name: "조직 B", source: "hr", parentId: null, active: true, members: 0 },
  );
  members["g-a"] = [];
  members["g-b"] = [];
  await page.goto("/admin/organization?group=g-a");
  await page.getByRole("button", { name: "상위 조직 변경" }).click();
  await expect(page.getByLabel("상위 조직")).toHaveValue("g-hr");
  await page.getByRole("button", { name: "조직 B", exact: true }).click();
  await page.getByRole("button", { name: "상위 조직 변경" }).click();
  await expect(page.getByLabel("상위 조직")).toHaveValue("");
  await page.getByLabel("상위 조직").selectOption("g-hr");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await expect.poll(() => groups.find((g) => g.id === "g-b")?.parentId).toBe("g-hr");
  expect(sent.at(-1)).toMatchObject({
    path: "/api/v1/groups/g-b/parent",
    body: { parentId: "g-hr" },
  });
  await page.getByRole("button", { name: "상위 조직 변경" }).click();
  await expect(page.getByLabel("상위 조직")).toHaveValue("g-hr");
  await expect(page.getByLabel("상위 조직").getByRole("option", { name: "조직 B" })).toHaveCount(0);
});

test("late app access responses cannot replace the selected app or change its revoke target", async ({ page }) => {
  const { apps, access, accessByApp, sent } = await mock(page);
  apps.push({ id: "other", name: "다른 앱" });
  access.groupGrants = [{ groupId: "g-hr", environment: "dev", permission: "develop" }];
  accessByApp.other = {
    ...access,
    appId: "other",
    userGrants: [],
    groupGrants: [{ groupId: "g-hr", environment: "prod", permission: "use" }],
  };
  const requested = deferred();
  const release = deferred();
  const completed = deferred();
  await page.route("**/api/v1/apps/ledger/access", async (route) => {
    requested.resolve();
    await release.promise;
    await route.fulfill({ json: access }).catch(() => {});
    completed.resolve();
  });
  try {
    await page.goto("/admin/apps/ledger/access");
    const panel = page.locator(".ui-panel").filter({
      has: page.getByRole("heading", { name: "앱 접근", exact: true }),
    });
    await requested.promise;
    await page.getByRole("link", { name: /앱 카탈로그/ }).click();
    await page.getByRole("link", { name: "다른 앱", exact: true }).click();
    await expect(panel.getByText("본부 · 운영 · 사용")).toBeVisible();
    release.resolve();
    await completed.promise;
    await renderResponses(page);
    await expect(panel.getByText("본부 · 테스트 · 개발")).toHaveCount(0);
    await expect(panel.getByText("본부 · 운영 · 사용")).toBeVisible();
    await panel.getByRole("button", { name: "회수", exact: true }).click();
    await expect
      .poll(() => sent.at(-1))
      .toMatchObject({
        path: "/api/v1/group-grants",
        body: {
          appId: "other",
          groupId: "g-hr",
          environment: "prod",
          permission: "use",
          enabled: false,
        },
      });
  } finally {
    release.resolve();
  }
});

test("late HR membership responses cannot expose another group's removal action", async ({ page }) => {
  const { groups, members, sent } = await mock(page);
  groups.push({ id: "g-b", name: "조직 B", source: "hr", parentId: null, active: true, members: 1 });
  members["g-b"] = [{ id: "bob", name: "밥", enabled: true }];
  const requested = deferred();
  const release = deferred();
  await page.route("**/api/v1/groups/g-hr/members", async (route) => {
    requested.resolve();
    await release.promise;
    await route.fulfill({ json: [{ id: "alice", name: "앨리스", enabled: true }] });
  });
  try {
    await page.goto("/admin/organization?group=g-hr");
    await requested.promise;
    await page.getByRole("button", { name: "조직 B", exact: true }).click();
    const detail = page.getByRole("region", { name: "조직 B 구성원" });
    await expect(detail.getByRole("listitem")).toContainText("밥");
    release.resolve();
    await renderResponses(page);
    await expect(detail.getByRole("listitem")).toContainText("밥");
    await expect(detail.getByRole("listitem")).not.toContainText("앨리스");
    await detail.getByRole("button", { name: "주 소속 해제" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "주 소속 해제" }).click();
    await expect.poll(() => sent.at(-1)).toMatchObject({
      path: "/api/v1/users/bob/primary-group",
      body: { groupId: null, expectedGroupId: "g-b" },
    });
  } finally {
    release.resolve();
  }
});

test("an app mutation finishing after navigation stays scoped to its original app", async ({ page }) => {
  const { apps, access, accessByApp } = await mock(page);
  apps.push({ id: "other", name: "다른 앱" });
  accessByApp.other = {
    ...access,
    appId: "other",
    admission: { ...access.admission },
    userGrants: [],
    groupGrants: [],
  };
  const requested = deferred();
  const release = deferred();
  let otherReads = 0;
  await page.route("**/api/v1/apps/other/access", async (route) => {
    otherReads++;
    await route.fallback();
  });
  await page.route("**/api/v1/apps/ledger/admission", async (route) => {
    requested.resolve();
    await release.promise;
    await route.fallback();
  });
  try {
    await page.goto("/admin/apps/ledger/access");
    const panel = page.locator(".ui-panel").filter({
      has: page.getByRole("heading", { name: "앱 접근", exact: true }),
    });
    await panel.getByLabel("운영 입장 대상").selectOption("all");
    await requested.promise;
    await page.getByRole("link", { name: /앱 카탈로그/ }).click();
    await page.getByRole("link", { name: "다른 앱", exact: true }).click();
    await expect(panel.getByLabel("운영 입장 대상")).toHaveValue("selected");
    release.resolve();
    await expect.poll(() => access.admission.prod).toBe("all");
    await renderResponses(page);
    expect(otherReads).toBe(1);
    await expect(page).toHaveURL(/\/admin\/apps\/other$/);
    await expect(panel.getByLabel("운영 입장 대상")).toHaveValue("selected");
  } finally {
    release.resolve();
  }
});

test("a membership mutation finishing after selection changes refreshes the current group", async ({ page }) => {
  const { groups, members } = await mock(page);
  groups.push({ id: "g-b", name: "조직 B", source: "hr", parentId: null, active: true, members: 1 });
  members["g-b"] = [{ id: "bob", name: "밥", enabled: true }];
  const requested = deferred();
  const release = deferred();
  let otherReads = 0;
  await page.route("**/api/v1/groups/g-b/members", async (route) => {
    otherReads++;
    await route.fallback();
  });
  await page.route("**/api/v1/users/alice/primary-group", async (route) => {
    requested.resolve();
    await release.promise;
    await route.fallback();
  });
  try {
    await page.goto("/admin/organization?group=g-hr");
    const first = page.getByRole("region", { name: "본부 구성원" });
    await first.getByRole("combobox", { name: "주 소속으로 지정할 사용자" }).fill("앨리스");
    await first.getByRole("option", { name: "앨리스 (alice)", exact: true }).click();
    await first.getByRole("button", { name: "주 소속 지정" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "주 소속 지정" }).click();
    await requested.promise;
    await page.getByRole("button", { name: "조직 B", exact: true }).click();
    const detail = page.getByRole("region", { name: "조직 B 구성원" });
    await expect(detail.getByRole("listitem")).toContainText("밥");
    release.resolve();
    await expect.poll(() => otherReads).toBe(2);
    await expect(detail.getByRole("listitem")).toContainText("밥");
  } finally {
    release.resolve();
  }
});

test("failed audit filters discard the previous cursor and retry from the new first page", async ({ page }) => {
  const { auditQueries, audit } = await mock(page);
  let fail = true;
  await page.route("**/api/v1/audit?*", async (route) => {
    const query = new URL(route.request().url()).searchParams;
    if (query.get("action") !== "app.") return route.fallback();
    auditQueries.push(query);
    if (fail)
      return route.fulfill({
        status: 503,
        json: { error: { code: "database_unavailable" } },
      });
    return route.fulfill({ json: { entries: [audit[2]], nextCursor: null } });
  });
  await page.goto("/admin/audit");
  const panel = page.locator(".ui-panel").filter({
    has: page.getByRole("heading", { name: "감사 기록", exact: true }),
  });
  await expect(panel.getByText("구성원 추가")).toBeVisible();
  await expect(panel.getByRole("button", { name: "더 보기" })).toBeVisible();
  await panel.getByLabel("동작").selectOption("app.");
  await panel.getByRole("button", { name: "조회", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("감사 기록을 불러오지 못했습니다");
  await expect(panel.getByText("구성원 추가")).toHaveCount(0);
  await expect(panel.getByRole("button", { name: "더 보기" })).toHaveCount(0);
  await expect(panel.getByText("감사 기록을 불러오지 못했습니다. 조건을 확인한 뒤 다시 조회하세요.")).toBeVisible();
  fail = false;
  await panel.getByRole("button", { name: "조회", exact: true }).click();
  await expect(panel.getByText("앱 사용 중지")).toBeVisible();
  await expect(panel.getByRole("row")).toHaveCount(2);
  expect(auditQueries.at(-1)?.get("cursor")).toBeNull();
  expect(auditQueries.at(-1)?.get("action")).toBe("app.");
});

test("an administrator registers a person, sets roles and suspends the account", async ({ page }) => {
  const { sent } = await mock(page);
  await page.goto("/admin/users");
  const subject = "3f0e2c5a-9d1b-4c7e-8a55-0123456789ab";
  const form = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "로그인 서버 계정 직접 등록", exact: true }) });
  await form.getByLabel("로그인 계정 ID").fill(subject);
  await form.getByLabel("표시 이름").fill("새 직원");
  await form.getByLabel("새 앱 등록 신청").selectOption("true");
  await form.getByRole("button", { name: "사용자 등록" }).click();
  await expect(page.getByRole("status")).toHaveText("사용자를 등록했습니다. 앱별 접근 설정에서 권한을 부여할 수 있습니다.");
  expect(sent.at(-1)).toMatchObject({
    method: "POST",
    path: "/api/v1/users",
    body: { id: subject, name: "새 직원", canDevelop: true },
    csrf: "fixture-csrf",
  });
  await expect(form.getByLabel("로그인 계정 ID")).toHaveValue("");
  // Inputs side by side keep one height: a hint under one must not stretch the other.
  const idBox = await form.getByLabel("로그인 계정 ID").boundingBox();
  const nameBox = await form.getByLabel("표시 이름").boundingBox();
  expect(Math.abs(idBox!.height - nameBox!.height)).toBeLessThan(2);
  expect(Math.abs(idBox!.y - nameBox!.y)).toBeLessThan(2);

  const list = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 계정", exact: true }) });
  const row = list.getByRole("listitem").filter({ hasText: "새 직원" });
  await expect(row).toContainText(subject);
  await expect(row.getByText("앱 개발", { exact: true })).toBeVisible();
  await row.getByRole("button", { name: "이름 변경" }).click();
  const renameDialog = page.getByRole("dialog", { name: "사용자 이름 변경" });
  await renameDialog.getByPlaceholder("표시 이름").fill("갱신한 직원");
  await renameDialog.getByRole("button", { name: "저장" }).click();
  await expect(list.getByRole("listitem").filter({ hasText: subject })).toContainText("갱신한 직원");
  expect(sent.at(-1)).toMatchObject({ method: "PATCH", path: `/api/v1/users/${subject}`, body: { name: "갱신한 직원" } });

  // The same ID again is refused by the server and explained.
  await form.getByLabel("로그인 계정 ID").fill(subject);
  await form.getByLabel("표시 이름").fill("중복");
  await form.getByRole("button", { name: "사용자 등록" }).click();
  await expect(page.getByRole("status")).toHaveText("이미 등록된 로그인 계정 ID입니다.");

  const updatedRow = list.getByRole("listitem").filter({ hasText: "갱신한 직원" });
  await updatedRow.getByRole("button", { name: "관리자 지정" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("모든 앱의 배포·제어");
  await dialog.getByRole("button", { name: "취소" }).click();
  expect(sent.filter((item) => item.method === "PATCH" && item.path === `/api/v1/users/${subject}`)).toHaveLength(1);
  await updatedRow.getByRole("button", { name: "관리자 지정" }).click();
  await dialog.getByRole("button", { name: "관리자 지정" }).click();
  await expect(page.getByRole("status")).toHaveText("관리자로 지정했습니다.");
  expect(sent.at(-1)).toMatchObject({ method: "PATCH", path: `/api/v1/users/${subject}`, body: { admin: true } });
  await expect(updatedRow.getByText("관리자", { exact: true })).toBeVisible();

  await updatedRow.getByRole("button", { name: "앱 개발 해제" }).click();
  await expect(page.getByRole("status")).toHaveText("앱 개발 신청 자격을 해제했습니다.");
  expect(sent.at(-1)).toMatchObject({ method: "PATCH", path: `/api/v1/users/${subject}`, body: { canDevelop: false } });

  await updatedRow.getByRole("button", { name: "사용 중지" }).click();
  await expect(dialog).toContainText("로그인·CLI·진행 중인 작업이 모두 차단");
  await dialog.getByRole("button", { name: "사용 중지" }).click();
  await expect(page.getByRole("status")).toHaveText("계정 사용을 중지했습니다.");
  expect(sent.at(-1)).toMatchObject({ method: "POST", path: `/api/v1/users/${subject}/state`, body: { enabled: false } });
  await expect(updatedRow.getByText("사용 중지", { exact: true }).first()).toBeVisible();
  await expect(updatedRow.getByRole("button", { name: "사용 재개" })).toBeVisible();
});

test("an administrator issues an invitation and a reset link, sees each address once and revokes a link", async ({ page }) => {
  const { sent, links } = await mock(page);
  await page.goto("/admin/users");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 초대", exact: true }) });
  await expect(panel.getByText("사용 전 0건")).toBeVisible();
  await panel.getByLabel("이름").fill("새 팀원");
  await panel.getByLabel("새 앱 등록 신청").selectOption("true");
  await panel.getByLabel("링크 유효 기간").selectOption("72");
  await panel.getByRole("button", { name: "초대 링크 만들기" }).click();
  await expect(page.getByRole("status").first()).toHaveText("초대 링크를 만들었습니다. 아래 주소를 초대할 사람에게 전달하세요.");
  expect(sent.at(-1)).toMatchObject({
    method: "POST",
    path: "/api/v1/account-links",
    body: { kind: "invitation", name: "새 팀원", canDevelop: true, expiresInHours: 72 },
    csrf: "fixture-csrf",
  });
  // The address is shown once, in a focused group, with what its holder can do.
  const issued = panel.getByRole("group", { name: "방금 만든 링크" });
  await expect(issued).toBeFocused();
  await expect(issued).toContainText("새 팀원에게 전달할 초대 링크");
  await expect(issued.getByLabel("링크 주소")).toHaveText(new RegExp(`/join#s{42}1$`));
  await expect(issued).toContainText("주소를 가진 사람이 계정을 만들 수 있으니");
  await expect(panel.getByLabel("이름")).toHaveValue("");
  const row = panel.getByRole("listitem").filter({ hasText: "새 팀원" });
  await expect(row.getByText("사용 전", { exact: true })).toBeVisible();
  await expect(row.getByText("앱 개발", { exact: true })).toBeVisible();
  // The list never carries a link address.
  await expect(row).not.toContainText("/join#");
  await issued.getByRole("button", { name: "닫기" }).click();
  await expect(panel.getByRole("group", { name: "방금 만든 링크" })).toHaveCount(0);

  // A reset link is issued for one enabled account after a confirmation.
  const list = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 계정", exact: true }) });
  const account = list.getByRole("listitem").filter({ hasText: "앨리스" });
  await account.getByRole("button", { name: "비밀번호 재설정 링크" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("이전에 만든 재설정 링크는 취소됩니다");
  await dialog.getByRole("button", { name: "취소" }).click();
  expect(sent.filter((item) => item.path === "/api/v1/account-links")).toHaveLength(1);
  await account.getByRole("button", { name: "비밀번호 재설정 링크" }).click();
  await dialog.getByRole("button", { name: "링크 만들기", exact: true }).click();
  expect(sent.at(-1)).toMatchObject({
    method: "POST",
    path: "/api/v1/account-links",
    body: { kind: "password_reset", userId: "alice" },
  });
  const reset = panel.getByRole("group", { name: "방금 만든 링크" });
  await expect(reset).toContainText("앨리스 계정의 비밀번호 재설정 링크");
  await expect(reset.getByLabel("링크 주소")).toHaveText(new RegExp(`/reset-password#s{42}2$`));
  await expect(reset).toContainText("주소를 가진 사람이 비밀번호를 바꿀 수 있으니");

  await row.getByRole("button", { name: "링크 취소" }).click();
  await expect(page.getByRole("status").first()).toHaveText("링크를 취소했습니다.");
  expect(sent.at(-1)).toMatchObject({ method: "POST", path: "/api/v1/account-links/link-1/revoke", csrf: "fixture-csrf" });
  await expect(row.getByText("취소됨", { exact: true })).toBeVisible();
  await expect(row.getByRole("button", { name: "링크 취소" })).toHaveCount(0);
  expect(links.items.map((item) => item.state)).toEqual(["pending", "revoked"]);

  // A suspended account is offered no reset link.
  await account.getByRole("button", { name: "사용 중지" }).click();
  await dialog.getByRole("button", { name: "사용 중지" }).click();
  await expect(account.getByRole("button", { name: "사용 재개" })).toBeVisible();
  await expect(account.getByRole("button", { name: "비밀번호 재설정 링크" })).toHaveCount(0);
});

test("a server without account links offers direct registration only", async ({ page }) => {
  const { links, sent } = await mock(page);
  links.available = false;
  await page.goto("/admin/users");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 초대", exact: true }) });
  await expect(panel).toContainText("이 서버에는 계정 링크 발급이 설정되지 않았습니다");
  await expect(panel.getByRole("button", { name: "초대 링크 만들기" })).toHaveCount(0);
  const list = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "사용자 계정", exact: true }) });
  await expect(list.getByRole("listitem").filter({ hasText: "앨리스" })).toBeVisible();
  await expect(list.getByRole("button", { name: "비밀번호 재설정 링크" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "로그인 서버 계정 직접 등록", exact: true })).toBeVisible();
  expect(sent.filter((item) => item.path === "/api/v1/account-links")).toEqual([]);
});
