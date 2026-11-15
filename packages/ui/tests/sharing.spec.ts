import { expect, test } from "@playwright/test";

const initial = {
  revision: "1",
  canManage: true,
  entries: [
    {
      subject: { kind: "user", id: "owner", name: "소유자" },
      role: "manager",
      locked: true,
    },
    {
      subject: { kind: "user", id: "shared", name: "기존 사용자" },
      role: "viewer",
    },
  ],
  roles: [
    { id: "viewer", label: "조회" },
    { id: "editor", label: "편집" },
    { id: "manager", label: "공유 관리" },
  ],
};

test("share editing protects locked entries and saves one revision-bound request", async ({
  page,
}) => {
  const requests: Record<string, unknown>[] = [];
  await page.route("**/fixture-share", async (route) => {
    if (route.request().method() === "GET")
      return route.fulfill({ json: initial });
    const input = route.request().postDataJSON();
    requests.push(input);
    await route.fulfill({
      json: { ...initial, revision: "2", entries: input.entries },
    });
  });
  await page.goto("/?view=share");
  await expect(
    page.getByRole("combobox", { name: "소유자 역할" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "소유자 공유 해제" }),
  ).toBeDisabled();
  await page
    .getByRole("combobox", { name: "기존 사용자 역할" })
    .selectOption("editor");
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(true);
  await page.getByRole("button", { name: "공유 설정 저장" }).click();
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(false);
  expect(requests).toHaveLength(1);
  expect(requests[0].expectedRevision).toBe("1");
  expect(requests[0].requestId).toMatch(/^[0-9a-f-]{36}$/);
  await expect(
    page.getByRole("combobox", { name: "기존 사용자 역할" }),
  ).toHaveValue("editor");
});

test("conflict preserves the draft and explicit reload requires discard confirmation", async ({
  page,
}) => {
  let reads = 0;
  await page.route("**/fixture-share", (route) => {
    if (route.request().method() === "POST")
      return route.fulfill({
        status: 409,
        json: { error: { code: "acl_revision_conflict" } },
      });
    reads += 1;
    return route.fulfill({ json: initial });
  });
  await page.goto("/?view=share");
  await page
    .getByRole("combobox", { name: "기존 사용자 역할" })
    .selectOption("editor");
  await page.getByRole("button", { name: "공유 설정 저장" }).click();
  await expect(page.getByRole("status")).toContainText("다른 곳에서 변경");
  await expect(
    page.getByRole("combobox", { name: "기존 사용자 역할" }),
  ).toHaveValue("editor");
  await expect(
    page.getByRole("button", { name: "공유 설정 저장" }),
  ).toBeDisabled();
  const readsBefore = reads;
  await page
    .getByRole("button", { name: "다시 불러오기", exact: true })
    .click();
  await page.getByRole("dialog").getByRole("button", { name: "취소" }).click();
  expect(reads).toBe(readsBefore);
  await page
    .getByRole("button", { name: "다시 불러오기", exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "다시 불러오기", exact: true })
    .click();
  await expect(
    page.getByRole("combobox", { name: "기존 사용자 역할" }),
  ).toHaveValue("viewer");
});

test("lost write response resolves by request ID without repeating the write", async ({
  page,
}) => {
  let writes = 0;
  let saved = initial;
  let requestId = "";
  await page.route("**/fixture-share", (route) => {
    if (route.request().method() === "GET")
      return route.fulfill({ json: initial });
    writes += 1;
    const input = route.request().postDataJSON();
    requestId = input.requestId;
    saved = { ...initial, revision: "2", entries: input.entries };
    return route.abort("connectionreset");
  });
  await page.route("**/fixture-share/results/*", (route) => {
    expect(new URL(route.request().url()).pathname.split("/").at(-1)).toBe(
      requestId,
    );
    return route.fulfill({ json: saved });
  });
  await page.goto("/?view=share");
  await page
    .getByRole("combobox", { name: "기존 사용자 역할" })
    .selectOption("editor");
  await page.getByRole("button", { name: "공유 설정 저장" }).click();
  await expect(page.getByRole("status")).toContainText(
    "저장 결과를 확인할 수 없습니다",
  );
  await expect(
    page.getByRole("button", { name: "공유 설정 저장" }),
  ).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.pendingRequest))
    .toBe(requestId);
  await page.getByRole("button", { name: "저장 결과 조회" }).click();
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.pendingRequest))
    .toBe(null);
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(false);
  expect(writes).toBe(1);
});

test("unknown result remains unresolved when lookup has no completed result", async ({
  page,
}) => {
  let writes = 0;
  await page.route("**/fixture-share", (route) => {
    if (route.request().method() === "GET")
      return route.fulfill({ json: initial });
    writes += 1;
    return route.abort("connectionreset");
  });
  await page.route("**/fixture-share/results/*", (route) =>
    route.fulfill({ body: "null", contentType: "application/json" }),
  );
  await page.goto("/?view=share");
  await page.getByRole("button", { name: "기존 사용자 공유 해제" }).click();
  await page.getByRole("button", { name: "공유 설정 저장" }).click();
  await page.getByRole("button", { name: "저장 결과 조회" }).click();
  await expect(page.getByRole("status")).toContainText(
    "완료된 저장 결과가 아직 없습니다",
  );
  await expect(
    page.getByRole("button", { name: "공유 설정 저장" }),
  ).toHaveCount(0);
  expect(writes).toBe(1);
});

test("current read-only permission disables changing and saving entries", async ({
  page,
}) => {
  await page.route("**/fixture-share", (route) =>
    route.fulfill({ json: { ...initial, canManage: false } }),
  );
  await page.goto("/?view=share");
  await expect(
    page.getByRole("combobox", { name: "공유 대상", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("combobox", { name: "기존 사용자 역할" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "기존 사용자 공유 해제" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "공유 설정 저장" }),
  ).toBeDisabled();
});

test("restored unknown write requires result lookup before editing the current snapshot", async ({
  page,
}) => {
  let writes = 0;
  await page.addInitScript(() => {
    window.uiInitialShareRequest = "previous-write";
  });
  await page.route("**/fixture-share", (route) => {
    if (route.request().method() === "POST") writes += 1;
    return route.fulfill({ json: initial });
  });
  await page.route("**/fixture-share/results/previous-write", (route) =>
    route.fulfill({ json: { ...initial, revision: "2" } }),
  );
  await page.goto("/?view=share");
  await expect(page.getByRole("status")).toContainText("이전 저장 요청");
  await expect(
    page.getByRole("combobox", { name: "기존 사용자 역할" }),
  ).toBeDisabled();
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(true);
  await page.getByRole("button", { name: "저장 결과 조회" }).click();
  await expect(
    page.getByRole("combobox", { name: "기존 사용자 역할" }),
  ).toBeEnabled();
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(false);
  expect(writes).toBe(0);
});

test("a new subject gets no role by position: the person chooses unless the app names a default", async ({
  page,
}) => {
  await page.route("**/fixture-directory?**", (route) =>
    route.fulfill({
      json: {
        items: [{ kind: "user", id: "new", name: "새 사용자" }],
        nextCursor: null,
      },
    }),
  );
  // The first listed role can manage sharing; it must never be implied.
  let snapshot: Record<string, unknown> = {
    ...initial,
    roles: [...initial.roles].reverse(),
  };
  const writes: Array<{ entries: Array<{ role: string }> }> = [];
  await page.route("**/fixture-share", (route) => {
    if (route.request().method() === "GET")
      return route.fulfill({ json: snapshot });
    writes.push(route.request().postDataJSON());
    return route.fulfill({
      json: { ...snapshot, revision: "2", entries: writes[0].entries },
    });
  });
  const pick = async () => {
    await page.goto("/?view=share");
    await page
      .getByRole("combobox", { name: "공유 대상", exact: true })
      .focus();
    await page.getByRole("option", { name: /새 사용자/ }).click();
  };
  await pick();
  const role = page.getByRole("combobox", { name: "새 사용자 역할" });
  const save = page.getByRole("button", { name: "공유 설정 저장" });
  await expect(role).toHaveValue("");
  await expect(save).toBeDisabled();
  await expect(page.getByText("역할을 선택하지 않은 대상")).toBeVisible();
  await role.selectOption("viewer");
  await save.click();
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.dirty))
    .toBe(false);
  expect(writes).toHaveLength(1);
  expect(writes[0].entries.map((entry) => entry.role).sort()).toEqual([
    "manager",
    "viewer",
    "viewer",
  ]);
  snapshot = { ...snapshot, defaultRole: "viewer" };
  await pick();
  await expect(role).toHaveValue("viewer");
  // A default that is not one of the roles is ignored, not trusted.
  snapshot = { ...snapshot, defaultRole: "administrator" };
  await pick();
  await expect(role).toHaveValue("");
});
