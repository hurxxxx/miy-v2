import { test, expect, type Page } from "@playwright/test";

const id = "93e7411d-b4d6-4a43-a9a6-a9e3998dc408";
const approvalPath = `/cli/device/${id}`;
const statusPath = `/api/v1/cli/devices/${id}`;

test("a status network failure after a rejected code does not expire a live request", async ({
  page,
}) => {
  await setup(page);
  let reads = 0;
  await page.route(`**${statusPath}`, (route) =>
    route.fulfill(
      ++reads === 1
        ? { json: { status: "pending", expiresIn: 180 } }
        : { status: 503, json: { error: { code: "identity_unavailable" } } },
    ),
  );
  await page.route(`**${statusPath}/decision`, (route) =>
    route.fulfill({
      status: 409,
      json: { error: { code: "login_request_expired_or_decided" } },
    }),
  );
  await page.goto(approvalPath);
  await page.getByLabel("인증 코드").fill("WRONG-CODE");
  await page.getByRole("button", { name: "이 계정으로 CLI 연결 승인" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "상태를 다시 확인하지 못했습니다",
  );
  await expect(page.getByLabel("인증 코드")).toHaveValue("WRONG-CODE");
  await expect(page.getByRole("timer")).toBeVisible();
});

async function setup(page: Page, signedIn = true) {
  await page.route("**/api/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    return route.fulfill({
      status: path === "/api/v1/me" && !signedIn ? 401 : 200,
      json:
        path === "/api/v1/me"
          ? {
              id: "fixture",
              name: "검증 사용자",
              admin: false,
              csrf: "fixture-csrf",
            }
          : [],
    });
  });
  await page.route("**/auth/options", (route) =>
    route.fulfill({
      json: {
        developmentLogin: false,
        developmentOrigin: "",
        accounts: [],
        oauthUrl: "/auth/login",
      },
    }),
  );
}

test("approval shows the server lifetime and submits the exact code with browser CSRF", async ({
  page,
}, info) => {
  await setup(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route(`**${statusPath}`, (route) =>
    route.fulfill({ json: { status: "pending", expiresIn: 120 } }),
  );
  let submission: { code: string; csrf?: string } | undefined;
  await page.route(`**${statusPath}/decision`, async (route) => {
    submission = {
      code: route.request().postDataJSON().userCode,
      csrf: route.request().headers()["x-miy-csrf"],
    };
    await route.fulfill({ json: { status: "approved" } });
  });
  await page.goto(approvalPath);
  await expect(page.getByRole("timer")).toHaveText(/1분 5\d초|2분 0초/);
  await page.getByLabel("인증 코드").fill("ABCDE-FGHJK");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: info.outputPath("mobile-approval.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "이 계정으로 CLI 연결 승인" }).click();
  await expect(page.getByRole("status")).toContainText("승인했습니다");
  expect(submission).toEqual({ code: "ABCDE-FGHJK", csrf: "fixture-csrf" });
});

test("expired or missing requests show a fresh-login command and cannot approve", async ({
  page,
}, info) => {
  await setup(page);
  let writes = 0;
  page.on("request", (request) => {
    if (request.method() === "POST") writes++;
  });
  await page.route(`**${statusPath}`, (route) =>
    route.fulfill({ status: 404, json: { error: { code: "not_found" } } }),
  );
  await page.goto(approvalPath);
  await expect(page.getByRole("status")).toContainText("만료됐거나");
  await expect(page.locator("pre")).toHaveText("miy auth login");
  await expect(page.getByLabel("인증 코드")).toHaveCount(0);
  expect(writes).toBe(0);
  await page.screenshot({
    path: info.outputPath("expired-request.png"),
    fullPage: true,
  });
});

test("pending countdown disables an expired request without sending another approval", async ({
  page,
}) => {
  await setup(page);
  await page.route(`**${statusPath}`, (route) =>
    route.fulfill({ json: { status: "pending", expiresIn: 2 } }),
  );
  await page.goto(approvalPath);
  await expect(page.getByRole("timer")).toBeVisible();
  await expect(page.getByRole("status")).toContainText("만료됐거나", {
    timeout: 6000,
  });
  await expect(
    page.getByRole("button", { name: "이 계정으로 CLI 연결 승인" }),
  ).toHaveCount(0);
});

test("wrong code preserves a pending request so the user can correct it", async ({
  page,
}) => {
  await setup(page);
  await page.route(`**${statusPath}`, (route) =>
    route.fulfill({ json: { status: "pending", expiresIn: 180 } }),
  );
  await page.route(`**${statusPath}/decision`, (route) =>
    route.fulfill({
      status: 409,
      json: { error: { code: "login_request_expired_or_decided" } },
    }),
  );
  await page.goto(approvalPath);
  await page.getByLabel("인증 코드").fill("WRONG-CODE");
  await page.getByRole("button", { name: "이 계정으로 CLI 연결 승인" }).click();
  await expect(page.getByRole("alert")).toContainText("최신 코드를 확인");
  await expect(page.getByLabel("인증 코드")).toHaveValue("WRONG-CODE");
  await expect(
    page.getByRole("button", { name: "이 계정으로 CLI 연결 승인" }),
  ).toBeEnabled();
});

test("a failed metadata read can be retried without starting a new device request", async ({
  page,
}) => {
  await setup(page);
  let reads = 0;
  await page.route(`**${statusPath}`, (route) =>
    route.fulfill(
      ++reads === 1
        ? { status: 503, json: { error: { code: "identity_unavailable" } } }
        : { json: { status: "pending", expiresIn: 120 } },
    ),
  );
  await page.goto(approvalPath);
  await page.getByRole("button", { name: "요청 다시 확인" }).click();
  await expect(page.getByRole("timer")).toBeVisible();
  expect(reads).toBe(2);
});

test("OAuth recovery offers a new login and never reflects arbitrary callback input", async ({
  page,
}) => {
  await setup(page, false);
  await page.goto("/?login=expired&error=untrusted-callback-value");
  await expect(page.getByRole("alert")).toContainText("새 로그인을 시작");
  await expect(
    page.getByRole("link", { name: "로그인", exact: true }),
  ).toHaveAttribute("href", "/auth/login");
  await expect(page.locator("body")).not.toContainText(
    "untrusted-callback-value",
  );
  await page.goto("/?login=forbidden");
  await expect(page.getByRole("alert")).toContainText(
    "관리자에게 계정 상태와 권한",
  );
});

test("390px shell leaves content usable, opens navigation and closes it after routing", async ({
  page,
}, info) => {
  await setup(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/environments/dev");
  const toggle = page.getByRole("button", { name: "메뉴 열기" });
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator("#platform-navigation")).toBeHidden();
  expect(
    (await page.locator(".workspace > main").boundingBox())!.width,
  ).toBeGreaterThanOrEqual(380);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await toggle.click();
  await expect(page.getByRole("button", { name: "메뉴 닫기" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  await page.getByRole("link", { name: "운영 영역", exact: true }).click();
  await expect(page).toHaveURL(/\/environments\/prod$/);
  await expect(page.locator("#platform-navigation")).toBeHidden();
  await page.screenshot({
    path: info.outputPath("mobile-shell.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(page.locator("#platform-navigation")).toBeVisible();
  await expect(page.getByRole("button", { name: /메뉴/ })).toBeHidden();
});
