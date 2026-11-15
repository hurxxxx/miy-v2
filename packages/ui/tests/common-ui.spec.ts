import { test, expect, type Page } from "@playwright/test";

const user = (id: string, name: string) => ({ kind: "user", id, name });
const initial = [
  user("other", "다른 사용자"),
  user("me", "내 이름"),
  user("locked", "고정 사용자"),
];
async function directory(page: Page) {
  await page.route("**/fixture-directory?**", (route) => {
    const query = new URL(route.request().url()).searchParams;
    if (query.get("kind") === "group")
      return route.fulfill({
        json: {
          items: [{ kind: "group", id: "me", name: "설계 그룹" }],
          nextCursor: null,
        },
      });
    if (query.has("cursor"))
      return route.fulfill({
        json: {
          items: [initial[0], user("later", "다음 페이지 사용자")],
          nextCursor: null,
        },
      });
    return route.fulfill({ json: { items: initial, nextCursor: "page-2" } });
  });
}

test("picker retains locked subjects, prioritizes self and uses typed IDs across pages", async ({
  page,
}) => {
  await directory(page);
  await page.goto("/");
  const search = page.getByRole("combobox", { name: "공유 대상" });
  await search.focus();
  await expect(page.getByRole("option").first()).toHaveText(/내 이름.*본인/);
  await expect(
    page.getByRole("button", { name: "고정 사용자 제거" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("option", { name: /고정 사용자/ }),
  ).toBeDisabled();
  await page
    .getByRole("option", { name: /고정 사용자/ })
    .dispatchEvent("click");
  await expect(page.getByTestId("selection")).toHaveText("user:locked");
  await search.press("ArrowDown");
  await search.press("Enter");
  await expect(page.getByTestId("selection")).toContainText("user:me");
  await page.getByRole("button", { name: "더 보기" }).click();
  await expect(page.getByRole("option")).toHaveCount(4);
  await expect(page.getByRole("option", { name: "다른 사용자" })).toHaveCount(
    1,
  );
  await page.getByRole("button", { name: "그룹", exact: true }).click();
  await page.getByRole("option", { name: "설계 그룹" }).click();
  await expect(page.getByTestId("selection")).toContainText("group:me");
  await expect(page.getByTestId("selection")).toContainText("user:me");
  await search.press("Escape");
  await expect(search).toHaveAttribute("aria-expanded", "false");
  await expect(search).toBeFocused();
});

test("typing reads the directory once per pause, not once per keystroke", async ({
  page,
}) => {
  const queries: string[] = [];
  await page.route("**/fixture-directory?**", (route) => {
    queries.push(
      new URL(route.request().url()).searchParams.get("query") ?? "",
    );
    return route.fulfill({ json: { items: initial, nextCursor: null } });
  });
  await page.goto("/");
  const search = page.getByRole("combobox", { name: "공유 대상" });
  await search.focus();
  await expect(page.getByRole("option")).toHaveCount(3);
  await search.pressSequentially("설계 담당", { delay: 20 });
  await expect.poll(() => queries.at(-1)).toBe("설계 담당");
  // Opening the list reads at once; the five typed characters read once.
  expect(queries).toEqual(["", "설계 담당"]);
});

test("superseded searches abort and cannot replace fresh results even if a provider ignores abort", async ({
  page,
}) => {
  await page.route("**/fixture-directory?**", async (route) => {
    const query = new URL(route.request().url()).searchParams.get("query");
    if (query === "slow")
      await new Promise((resolve) => setTimeout(resolve, 350));
    await route.fulfill({
      json: { items: query ? [user(query, query)] : initial, nextCursor: null },
    });
  });
  await page.goto("/?ignore-abort");
  const search = page.getByRole("combobox", { name: "공유 대상" });
  await search.fill("slow");
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.requests.length))
    .toBeGreaterThan(0);
  await search.fill("fresh");
  await expect(
    page.getByRole("option", { name: "fresh", exact: true }),
  ).toBeVisible();
  await page.waitForTimeout(450);
  await expect(
    page.getByRole("option", { name: "slow", exact: true }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.uiFixture.requests.some((signal) => signal.aborted),
    ),
  ).toBe(true);
});

test("a revoked page clears candidates and retry starts at the first page", async ({
  page,
}) => {
  let revoked = true;
  await page.route("**/fixture-directory?**", (route) => {
    const query = new URL(route.request().url()).searchParams;
    if (query.has("cursor") && revoked) {
      revoked = false;
      return route.fulfill({
        status: 403,
        json: { error: { code: "forbidden" } },
      });
    }
    return route.fulfill({ json: { items: initial, nextCursor: "page-2" } });
  });
  await page.goto("/");
  await page.getByRole("combobox").focus();
  await page.getByRole("button", { name: "더 보기" }).click();
  await expect(page.getByRole("alert")).toContainText("접근 권한");
  await expect(page.getByRole("option")).toHaveCount(0);
  await page.getByRole("button", { name: "다시 시도" }).click();
  await expect(page.getByRole("option")).toHaveCount(3);
});

test("changing focus or unmounting cancels the outstanding directory request", async ({
  page,
}) => {
  await page.route("**/fixture-directory?**", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 300));
    await route
      .fulfill({ json: { items: initial, nextCursor: null } })
      .catch(() => {});
  });
  await page.goto("/");
  await page.getByRole("combobox").focus();
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.requests.length))
    .toBeGreaterThan(0);
  await page.getByRole("button", { name: "선택기 숨기기" }).click();
  expect(
    await page.evaluate(() =>
      window.uiFixture.requests.every((signal) => signal.aborted),
    ),
  ).toBe(true);
});

test("dialog confines focus in its document, Escape returns focus, prompt validates and resolves", async ({
  page,
}) => {
  await page.goto("/");
  const open = page.getByRole("button", { name: "확인 대화상자 열기" });
  await open.click();
  await expect(page.getByRole("dialog", { name: "변경 확인" })).toBeVisible();
  await page
    .locator("#outside")
    .evaluate((element) => (element as HTMLElement).focus());
  expect(
    await page.evaluate(() =>
      document.querySelector("dialog")!.contains(document.activeElement),
    ),
  ).toBe(true);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(open).toBeFocused();
  await expect(page.getByTestId("feedback-result")).toHaveText("false");
  await page.getByRole("button", { name: "이름 입력 열기" }).click();
  const name = page.getByRole("textbox", { name: "이름", exact: true });
  await expect(name).toBeFocused();
  await name.fill(" ");
  await name.press("Enter");
  await expect(page.getByRole("alert")).toHaveText("값을 입력해 주세요.");
  await name.fill(" 새 이름 ");
  await name.press("Enter");
  await expect(page.getByTestId("feedback-result")).toHaveText("새 이름");
  await expect(
    page.getByRole("button", { name: "이름 입력 열기" }),
  ).toBeFocused();
});

test("superseded and unmounted confirmations settle without leaking pending promises", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "동시 확인" }).click();
  await expect(page.getByRole("dialog", { name: "새 확인" })).toBeVisible();
  await expect(page.getByTestId("feedback-result")).toHaveText(
    "previous:false",
  );
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "확인 대화상자 열기" }).click();
  await page.evaluate(() => window.uiFixture.unmountFeedback?.());
  await expect
    .poll(() => page.evaluate(() => window.uiFixture.result))
    .toBe("false");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("untrusted labels remain text and light/dark mobile layouts fit the viewport", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route("**/fixture-directory?**", (route) =>
    route.fulfill({
      json: {
        items: [user("html", '<img src=x onerror="alert(1)">')],
        nextCursor: null,
      },
    }),
  );
  await page.goto("/");
  await page.getByRole("combobox").focus();
  await expect(page.getByRole("option")).toContainText("<img src=x");
  await expect(page.locator("img")).toHaveCount(0);
  await page.screenshot({
    path: ".local/evidence/common-ui/light-mobile.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "테마 전환" }).click();
  await page.getByRole("combobox").focus();
  await expect(page.locator("main")).toHaveAttribute("data-miy-theme", "dark");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: ".local/evidence/common-ui/dark-mobile.png",
    fullPage: true,
  });
});
