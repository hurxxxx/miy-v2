import { test, expect, type Page } from "@playwright/test";

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
  const apps = [{ id: "ledger", name: "장부" }];
  const accessByApp: Record<string, typeof access> = { ledger: access };
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
      return route.fulfill({ status: 200, json: { updated: true } });
    }
    const reply =
      path === "/api/v1/me"
        ? admin
        : path === "/api/v1/installations"
          ? []
          : path === "/api/v1/app-registrations"
            ? []
            : path === "/api/v1/users"
              ? [
                  { id: "admin", name: "관리자" },
                  { id: "alice", name: "앨리스" },
                ]
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
    sent,
    access,
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
  const { sent } = await mock(page);
  await page.goto("/admin");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "그룹", exact: true }) });
  await panel.getByLabel("이름").fill("영업 1팀");
  await expect(panel.getByLabel("상위 조직").first()).toBeDisabled();
  await panel.getByRole("button", { name: "만들기" }).click();
  await expect(page.getByRole("status")).toHaveText("그룹을 만들었습니다.");
  expect(sent.at(-1)).toMatchObject({
    method: "POST",
    path: "/api/v1/groups",
    body: { name: "영업 1팀", source: "local", parentId: null },
    csrf: "fixture-csrf",
  });
  const row = panel.getByRole("row", { name: /영업 1팀/ });
  await row.getByRole("button", { name: "구성원" }).click();
  const detail = page.getByRole("region", { name: "영업 1팀 구성원" });
  await detail.getByLabel("추가할 사용자").selectOption("alice");
  await detail.getByRole("button", { name: "추가" }).click();
  await expect(detail.getByRole("listitem")).toContainText("앨리스");
  await detail.getByRole("button", { name: "빼기" }).click();
  await expect(detail.getByText("구성원이 없습니다.")).toBeVisible();
  expect(sent.filter((s) => s.path.endsWith("/members")).map((s) => (s.body as { member: boolean }).member)).toEqual([true, false]);

  const appPanel = page.locator(".ui-panel").filter({
    has: page.getByRole("heading", { name: "앱 접근", exact: true }),
  });
  await appPanel.getByRole("combobox", { name: "앱", exact: true }).selectOption("ledger");
  await appPanel.getByRole("combobox", { name: "그룹", exact: true }).selectOption({ label: "영업 1팀" });
  await appPanel.getByRole("button", { name: "부여", exact: true }).click();
  await expect(appPanel.getByText("영업 1팀 · 테스트 · 사용")).toBeVisible();
  await row.getByRole("button", { name: "비활성화", exact: true }).click();
  await expect(appPanel.getByRole("combobox", { name: "그룹", exact: true }).locator("option", { hasText: "영업 1팀" })).toHaveCount(0);
});

test("HR groups take members through the primary group, not manual membership", async ({ page }) => {
  const { sent } = await mock(page);
  await page.goto("/admin");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "그룹", exact: true }) });
  await panel.getByRole("row", { name: /본부/ }).getByRole("button", { name: "구성원" }).click();
  const detail = page.getByRole("region", { name: "본부 구성원" });
  await detail.getByLabel("주 소속으로 지정할 사용자").selectOption("alice");
  await detail.getByRole("button", { name: "주 소속 지정" }).click();
  expect(sent.at(-1)).toMatchObject({
    method: "PUT",
    path: "/api/v1/users/alice/primary-group",
    body: { groupId: "g-hr" },
  });
});

test("the company switch asks first, and groups can receive use or develop but never deploy", async ({ page }) => {
  const { sent, access } = await mock(page);
  await page.goto("/admin");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "앱 접근" }) });
  await panel.getByRole("combobox", { name: /^앱/ }).selectOption("ledger");
  await expect(panel.getByText("사용 중", { exact: true })).toBeVisible();
  await expect(panel.getByText("앨리스 · 테스트 · 사용")).toBeVisible();

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

  const permission = panel.getByLabel("권한");
  await expect(permission.locator("option")).toHaveText(["사용", "개발"]);
  await permission.selectOption("develop");
  await panel.getByRole("button", { name: "부여" }).click();
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
});

test("the audit log pages backwards and filters by actor, action and period", async ({ page }) => {
  const { auditQueries, audit } = await mock(page);
  await page.goto("/admin");
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
  await expect(panel.getByLabel("동작").locator("option", { hasText: "사용자 상태·주 소속" })).toHaveAttribute("value", "user.");
  await panel.getByLabel("동작").selectOption("user.");
  await panel.getByRole("button", { name: "조회", exact: true }).click();
  await expect(panel.getByText("사용자 다시 사용", { exact: true })).toBeVisible();
  await expect(panel.getByText("사용자 비활성화", { exact: true })).toBeVisible();
  expect(auditQueries.at(-1)?.get("action")).toBe("user.");
});

test("switching HR groups uses the selected group's parent, including after a change", async ({ page }) => {
  const { groups, members, sent } = await mock(page);
  groups.push(
    {
      id: "g-a",
      name: "조직 A",
      source: "hr",
      parentId: "g-hr",
      active: true,
      members: 0,
    },
    {
      id: "g-b",
      name: "조직 B",
      source: "hr",
      parentId: null,
      active: true,
      members: 0,
    },
  );
  members["g-a"] = [];
  members["g-b"] = [];
  await page.goto("/admin");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "그룹", exact: true }) });
  await panel
    .getByRole("row", { name: /조직 A/ })
    .getByRole("button", { name: "구성원" })
    .click();
  await expect(page.getByRole("region", { name: "조직 A 구성원" }).getByLabel("상위 조직")).toHaveValue("g-hr");
  await panel
    .getByRole("row", { name: /조직 B/ })
    .getByRole("button", { name: "구성원" })
    .click();
  const detail = page.getByRole("region", { name: "조직 B 구성원" });
  await expect(detail.getByLabel("상위 조직")).toHaveValue("");
  await detail.getByRole("button", { name: "상위 조직 변경" }).click();
  await expect
    .poll(() => sent.at(-1))
    .toMatchObject({
      path: "/api/v1/groups/g-b/parent",
      body: { parentId: null },
    });
  await detail.getByLabel("상위 조직").selectOption("g-hr");
  await detail.getByRole("button", { name: "상위 조직 변경" }).click();
  await expect.poll(() => groups.find((g) => g.id === "g-b")?.parentId).toBe("g-hr");
  await expect(detail.getByLabel("상위 조직")).toHaveValue("g-hr");
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
  await page.route("**/api/v1/apps/ledger/access", async (route) => {
    requested.resolve();
    await release.promise;
    await route.fulfill({ json: access });
  });
  try {
    await page.goto("/admin");
    const panel = page.locator(".ui-panel").filter({
      has: page.getByRole("heading", { name: "앱 접근", exact: true }),
    });
    await panel.getByRole("combobox", { name: "앱", exact: true }).selectOption("ledger");
    await requested.promise;
    await panel.getByRole("combobox", { name: "앱", exact: true }).selectOption("other");
    await expect(panel.getByText("본부 · 운영 · 사용")).toBeVisible();
    const response = page.waitForResponse("**/api/v1/apps/ledger/access");
    release.resolve();
    await (await response).finished();
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
  groups.push({
    id: "g-b",
    name: "조직 B",
    source: "hr",
    parentId: null,
    active: true,
    members: 1,
  });
  members["g-b"] = [{ id: "bob", name: "밥", enabled: true }];
  const requested = deferred();
  const release = deferred();
  await page.route("**/api/v1/groups/g-hr/members", async (route) => {
    requested.resolve();
    await release.promise;
    await route.fulfill({
      json: [{ id: "alice", name: "앨리스", enabled: true }],
    });
  });
  try {
    await page.goto("/admin");
    const panel = page.locator(".ui-panel").filter({
      has: page.getByRole("heading", { name: "그룹", exact: true }),
    });
    await panel.getByRole("row", { name: /본부/ }).getByRole("button", { name: "구성원" }).click();
    await requested.promise;
    await panel
      .getByRole("row", { name: /조직 B/ })
      .getByRole("button", { name: "구성원" })
      .click();
    const detail = page.getByRole("region", { name: "조직 B 구성원" });
    await expect(detail.getByRole("listitem")).toContainText("밥");
    const response = page.waitForResponse("**/api/v1/groups/g-hr/members");
    release.resolve();
    await (await response).finished();
    await renderResponses(page);
    await expect(detail.getByRole("listitem")).toContainText("밥");
    await expect(detail.getByRole("listitem")).not.toContainText("앨리스");
    await detail.getByRole("button", { name: "주 소속 해제" }).click();
    await expect
      .poll(() => sent.at(-1))
      .toMatchObject({
        path: "/api/v1/users/bob/primary-group",
        body: { groupId: null },
      });
  } finally {
    release.resolve();
  }
});

test("an app mutation finishing after selection changes refreshes the current app", async ({ page }) => {
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
    await page.goto("/admin");
    const panel = page.locator(".ui-panel").filter({
      has: page.getByRole("heading", { name: "앱 접근", exact: true }),
    });
    await panel.getByRole("combobox", { name: "앱", exact: true }).selectOption("ledger");
    await panel.getByLabel("운영 입장 대상").selectOption("all");
    await requested.promise;
    await panel.getByRole("combobox", { name: "앱", exact: true }).selectOption("other");
    await expect(panel.getByLabel("운영 입장 대상")).toHaveValue("selected");
    release.resolve();
    await expect.poll(() => otherReads).toBe(2);
    await expect(panel.getByRole("combobox", { name: "앱", exact: true })).toHaveValue("other");
    await expect(panel.getByLabel("운영 입장 대상")).toHaveValue("selected");
  } finally {
    release.resolve();
  }
});

test("a membership mutation finishing after selection changes refreshes the current group", async ({ page }) => {
  const { groups, members } = await mock(page);
  groups.push({
    id: "g-b",
    name: "조직 B",
    source: "hr",
    parentId: null,
    active: true,
    members: 1,
  });
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
    await page.goto("/admin");
    const panel = page.locator(".ui-panel").filter({
      has: page.getByRole("heading", { name: "그룹", exact: true }),
    });
    await panel.getByRole("row", { name: /본부/ }).getByRole("button", { name: "구성원" }).click();
    const first = page.getByRole("region", { name: "본부 구성원" });
    await first.getByLabel("주 소속으로 지정할 사용자").selectOption("alice");
    await first.getByRole("button", { name: "주 소속 지정" }).click();
    await requested.promise;
    await panel
      .getByRole("row", { name: /조직 B/ })
      .getByRole("button", { name: "구성원" })
      .click();
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
  await page.goto("/admin");
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
