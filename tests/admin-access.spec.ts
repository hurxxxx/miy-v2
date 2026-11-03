import { test, expect, type Page } from "@playwright/test";

// Admin UI for users, groups and app access with mocked APIs. These checks cover the
// requests the screen sends and what it shows; server authorization is covered by the
// isolated permissions regression.
const admin = { id: "admin", name: "관리자", admin: true, csrf: "fixture-csrf" };
type Group = { id: string; name: string; source: "hr" | "local"; parentId: string | null; active: boolean; members: number };

async function mock(page: Page) {
  const sent: { method: string; path: string; body: unknown; csrf: string | null }[] = [];
  const groups: Group[] = [{ id: "g-hr", name: "본부", source: "hr", parentId: null, active: true, members: 0 }];
  const members: Record<string, { id: string; name: string; enabled: boolean }[]> = { "g-hr": [] };
  const access = {
    appId: "ledger",
    disabled: false,
    admission: { dev: "selected", prod: "selected" },
    userGrants: [{ userId: "alice", environment: "dev", permission: "use" }],
    groupGrants: [] as { groupId: string; environment: string; permission: string }[],
  };
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    const method = request.method();
    if (method !== "GET") {
      const body = request.postDataJSON();
      sent.push({ method, path, body, csrf: await request.headerValue("x-miy-csrf") });
      if (path === "/api/v1/groups" && method === "POST") {
        const id = `g-${groups.length}`;
        groups.push({ id, name: body.name, source: body.source, parentId: body.parentId, active: true, members: 0 });
        members[id] = [];
        return route.fulfill({ status: 200, json: { id } });
      }
      const member = path.match(/^\/api\/v1\/groups\/([^/]+)\/members$/);
      if (member) {
        const list = members[member[1]];
        if (body.member) list.push({ id: body.userId, name: body.userId === "alice" ? "앨리스" : body.userId, enabled: true });
        else list.splice(list.findIndex((m) => m.id === body.userId), 1);
        groups.find((g) => g.id === member[1])!.members = list.length;
        return route.fulfill({ status: 200, json: { updated: true } });
      }
      if (path === "/api/v1/apps/ledger/control") access.disabled = body.disabled;
      if (path === "/api/v1/apps/ledger/admission") access.admission[body.environment as "dev" | "prod"] = body.mode;
      if (path === "/api/v1/group-grants") {
        if (body.permission === "deploy") return route.fulfill({ status: 400, json: { error: { code: "group_permission_not_allowed" } } });
        if (body.enabled) access.groupGrants.push({ groupId: body.groupId, environment: body.environment, permission: body.permission });
        else access.groupGrants = access.groupGrants.filter((g) => g.groupId !== body.groupId || g.permission !== body.permission);
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
                    ? { apps: [{ id: "ledger", name: "장부" }], nextCursor: null }
                    : path === "/api/v1/apps/ledger/access"
                      ? access
                      : null;
    if (reply === null) return route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
    await route.fulfill({ status: 200, json: reply });
  });
  return { sent, access };
}

test("an administrator creates a local group and manages its members", async ({ page }) => {
  const { sent } = await mock(page);
  await page.goto("/admin");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "그룹", exact: true }) });
  await panel.getByLabel("이름").fill("영업 1팀");
  await expect(panel.getByLabel("상위 조직").first()).toBeDisabled();
  await panel.getByRole("button", { name: "만들기" }).click();
  await expect(page.getByRole("status")).toHaveText("그룹을 만들었습니다.");
  expect(sent.at(-1)).toMatchObject({ method: "POST", path: "/api/v1/groups", body: { name: "영업 1팀", source: "local", parentId: null }, csrf: "fixture-csrf" });
  const row = panel.getByRole("row", { name: /영업 1팀/ });
  await row.getByRole("button", { name: "구성원" }).click();
  const detail = page.getByRole("region", { name: "영업 1팀 구성원" });
  await detail.getByLabel("추가할 사용자").selectOption("alice");
  await detail.getByRole("button", { name: "추가" }).click();
  await expect(detail.getByRole("listitem")).toContainText("앨리스");
  await detail.getByRole("button", { name: "빼기" }).click();
  await expect(detail.getByText("구성원이 없습니다.")).toBeVisible();
  expect(sent.filter((s) => s.path.endsWith("/members")).map((s) => (s.body as { member: boolean }).member)).toEqual([true, false]);
});

test("HR groups take members through the primary group, not manual membership", async ({ page }) => {
  const { sent } = await mock(page);
  await page.goto("/admin");
  const panel = page.locator(".ui-panel").filter({ has: page.getByRole("heading", { name: "그룹", exact: true }) });
  await panel.getByRole("row", { name: /본부/ }).getByRole("button", { name: "구성원" }).click();
  const detail = page.getByRole("region", { name: "본부 구성원" });
  await detail.getByLabel("주 소속으로 지정할 사용자").selectOption("alice");
  await detail.getByRole("button", { name: "주 소속 지정" }).click();
  expect(sent.at(-1)).toMatchObject({ method: "PUT", path: "/api/v1/users/alice/primary-group", body: { groupId: "g-hr" } });
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
  expect(sent.at(-1)).toMatchObject({ path: "/api/v1/group-grants", body: { groupId: "g-hr", appId: "ledger", environment: "dev", permission: "develop", enabled: true } });
});
