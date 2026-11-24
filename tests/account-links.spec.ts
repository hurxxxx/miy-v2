import { mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";

// The public invitation and password reset page with mocked APIs. These checks cover
// what the page sends and shows; the server's decisions are covered by the isolated
// accounts regression and the real-server acceptance script.
const token = "T".repeat(43);
// A field's label element also holds its hint ("…아이디와 같을 수 없습니다"), so
// inputs are addressed by their form names; visible labels are asserted separately.
const field = (page: Page, name: string) => page.locator(`input[name=${name}]`);
type Sent = { path: string; body: Record<string, unknown>; csrf: string | null; address: string };

async function mock(
  page: Page,
  link: { kind: "invitation" | "password_reset"; name: string | null } | null,
  complete: () => { status: number; json: unknown } = () => ({
    status: 200,
    json: { status: "completed", kind: link?.kind, loginUrl: "https://portal.example.test/" },
  }),
) {
  await mockSurface(page, "console");
  const sent: Sent[] = [];
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === "/api/v1/me") return route.fulfill({ status: 401, json: { error: { code: "unauthorized" } } });
    if (request.method() === "GET") return route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
    sent.push({
      path,
      body: request.postDataJSON(),
      csrf: await request.headerValue("x-miy-csrf"),
      address: request.url(),
    });
    if (path === "/api/v1/account-links/inspect")
      return link
        ? route.fulfill({
            status: 200,
            json: { ...link, expiresAt: "2026-11-27T01:00:00Z", passwordMinLength: 10, portalOrigin: "https://portal.example.test" },
          })
        : route.fulfill({ status: 404, json: { error: { code: "account_link_invalid" } } });
    if (path === "/api/v1/account-links/complete") return route.fulfill(complete());
    return route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  return sent;
}

test("an invited person creates their account; the link secret leaves the address bar and no request address", async ({
  page,
}) => {
  const sent = await mock(page, { kind: "invitation", name: "새 팀원" });
  const addresses: string[] = [];
  page.on("request", (request) => addresses.push(request.url()));
  await page.goto(`/join#${token}`);
  await expect(page.getByRole("heading", { name: "계정 만들기" })).toBeVisible();
  // No session is needed and the sign-in screen is not shown.
  await expect(page.getByRole("link", { name: "로그인", exact: true })).toHaveCount(0);
  expect(new URL(page.url()).hash).toBe("");
  expect(sent[0]).toMatchObject({ path: "/api/v1/account-links/inspect", body: { token }, csrf: null });
  await expect(field(page, "name")).toHaveValue("새 팀원");
  for (const label of ["아이디", "이름", "이메일", "비밀번호", "비밀번호 확인"])
    await expect(page.locator(".ui-field-label", { hasText: new RegExp(`^${label}$`) })).toBeVisible();
  await expect(page.getByText("10자 이상. 아이디와 같을 수 없습니다.")).toBeVisible();

  await field(page, "username").fill("new.member");
  await field(page, "email").fill("new.member@example.com");
  await field(page, "password").fill("correct-horse-1");
  await field(page, "confirm").fill("correct-horse-2");
  await page.getByRole("button", { name: "계정 만들기" }).click();
  await expect(page.getByRole("alert")).toHaveText("비밀번호와 비밀번호 확인이 서로 다릅니다.");
  expect(sent).toHaveLength(1);
  await field(page, "confirm").fill("correct-horse-1");
  await page.getByRole("button", { name: "계정 만들기" }).click();
  await expect(page.getByRole("heading", { name: "계정을 만들었습니다" })).toBeVisible();
  expect(sent[1]).toMatchObject({
    path: "/api/v1/account-links/complete",
    body: { token, username: "new.member", name: "새 팀원", email: "new.member@example.com", password: "correct-horse-1" },
    csrf: null,
  });
  await expect(page.getByRole("link", { name: "로그인하러 가기" })).toHaveAttribute("href", "https://portal.example.test/");
  expect(addresses.filter((address) => address.includes(token))).toEqual([]);
  // A finished link is forgotten: opening the page again has nothing to use.
  await page.reload();
  await expect(page.getByRole("heading", { name: "사용할 수 없는 링크입니다" })).toBeVisible();
});

test("the browser checks the sign-in name and password length before anything is sent", async ({ page }) => {
  const sent = await mock(page, { kind: "invitation", name: null });
  await page.goto(`/join#${token}`);
  await field(page, "username").fill("Has Space");
  await field(page, "name").fill("홍길동");
  await field(page, "email").fill("hong@example.com");
  await field(page, "password").fill("correct-horse-1");
  await field(page, "confirm").fill("correct-horse-1");
  await page.getByRole("button", { name: "계정 만들기" }).click();
  expect(await field(page, "username").evaluate((input: HTMLInputElement) => input.validity.patternMismatch)).toBe(true);
  await field(page, "username").fill("hong");
  await field(page, "password").fill("short");
  await field(page, "confirm").fill("short");
  await page.getByRole("button", { name: "계정 만들기" }).click();
  expect(await field(page, "password").evaluate((input: HTMLInputElement) => input.validity.tooShort)).toBe(true);
  expect(sent.filter((item) => item.path.endsWith("/complete"))).toEqual([]);
});

test("a refusal keeps the form and explains it; a reload in the same tab keeps the link", async ({ page }) => {
  let reply = { status: 409, json: { error: { code: "account_exists" } } as unknown };
  const sent = await mock(page, { kind: "invitation", name: "새 팀원" }, () => reply);
  await page.goto(`/join#${token}`);
  const fill = async (username: string) => {
    await field(page, "username").fill(username);
    await field(page, "email").fill(`${username}@example.com`);
    await field(page, "password").fill("correct-horse-1");
    await field(page, "confirm").fill("correct-horse-1");
    await page.getByRole("button", { name: "계정 만들기" }).click();
  };
  await fill("taken");
  await expect(page.getByRole("alert")).toHaveText("이미 사용 중인 아이디 또는 이메일입니다. 다른 값을 입력해 주세요.");
  await expect(field(page, "username")).toHaveValue("taken");
  reply = { status: 400, json: { error: { code: "password_policy" } } };
  await fill("taken2");
  await expect(page.getByRole("alert")).toContainText("비밀번호가 조건에 맞지 않습니다");
  reply = { status: 503, json: { error: { code: "identity_unavailable" } } };
  await fill("taken3");
  await expect(page.getByRole("alert")).toContainText("로그인 서버에 연결하지 못했습니다");
  // The address no longer carries the secret, yet this tab still holds the link.
  await page.reload();
  await expect(page.getByRole("heading", { name: "계정 만들기" })).toBeVisible();
  expect(sent.filter((item) => item.path.endsWith("/inspect")).at(-1)!.body).toEqual({ token });
  // The server says the link stopped working: the page says so and forgets it.
  reply = { status: 404, json: { error: { code: "account_link_invalid" } } };
  await fill("late");
  await expect(page.getByRole("heading", { name: "사용할 수 없는 링크입니다" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("heading", { name: "사용할 수 없는 링크입니다" })).toBeVisible();
  expect(sent.filter((item) => item.path.endsWith("/inspect"))).toHaveLength(2);
});

test("a reset link asks only for the new password", async ({ page }) => {
  const sent = await mock(page, { kind: "password_reset", name: "앨리스" });
  await page.goto(`/reset-password#${token}`);
  await expect(page.getByRole("heading", { name: "비밀번호 재설정" })).toBeVisible();
  await expect(page.getByText("앨리스 계정의 새 비밀번호를 정합니다.")).toBeVisible();
  await expect(field(page, "username")).toHaveCount(0);
  await expect(field(page, "name")).toHaveCount(0);
  await expect(field(page, "email")).toHaveCount(0);
  await expect(page.locator(".ui-field-label", { hasText: /^새 비밀번호$/ })).toBeVisible();
  await field(page, "password").fill("a-brand-new-password");
  await field(page, "confirm").fill("a-brand-new-password");
  await page.getByRole("button", { name: "비밀번호 변경" }).click();
  await expect(page.getByRole("heading", { name: "비밀번호를 변경했습니다" })).toBeVisible();
  expect(sent.at(-1)).toEqual({
    path: "/api/v1/account-links/complete",
    body: { token, password: "a-brand-new-password" },
    csrf: null,
    address: expect.stringMatching(/\/api\/v1\/account-links\/complete$/),
  });
  await expect(page.getByRole("status")).toContainText("이전에 로그인한 브라우저는 로그아웃됩니다");
});

for (const scenario of [
  {
    title: "an accepted reset waits for old sessions to end",
    response: {
      status: 202,
      json: { status: "ending_sessions", kind: "password_reset", loginUrl: "https://portal.example.test/" },
    },
    heading: "기존 로그인을 종료하고 있습니다",
  },
  {
    title: "an uncertain password change clears the link and removes the form",
    response: { status: 503, json: { error: { code: "password_reset_unknown" } } },
    heading: "비밀번호 변경 결과를 확인하지 못했습니다",
  },
]) {
  test(scenario.title, async ({ page }) => {
    const sent = await mock(page, { kind: "password_reset", name: "앨리스" }, () => scenario.response);
    await page.goto(`/reset-password#${token}`);
    await field(page, "password").fill("a-brand-new-password");
    await field(page, "confirm").fill("a-brand-new-password");
    await page.getByRole("button", { name: "비밀번호 변경" }).click();
    await expect(page.getByRole("heading", { name: scenario.heading })).toBeVisible();
    await expect(field(page, "password")).toHaveCount(0);
    expect(await page.evaluate(() => sessionStorage.getItem("miy.accountLink"))).toBeNull();
    expect(sent.filter((item) => item.path.endsWith("/complete"))).toHaveLength(1);
  });
}

test("a missing, malformed or refused link is explained without a form", async ({ page }) => {
  const sent = await mock(page, null);
  for (const address of ["/join", "/join#too-short", `/reset-password#${token}`]) {
    await page.goto("/cli/install");
    await page.evaluate(() => sessionStorage.clear());
    await page.goto(address);
    await expect(page.getByRole("heading", { name: "사용할 수 없는 링크입니다" })).toBeVisible();
    await expect(page.getByRole("textbox")).toHaveCount(0);
    expect(new URL(page.url()).hash).toBe("");
  }
  // Only a well-formed secret is ever sent for inspection.
  expect(sent.map((item) => item.body)).toEqual([{ token }]);
});

test("the sign-in page says how to get an account or a new password", async ({ page }) => {
  await mock(page, null);
  await page.goto("/");
  await expect(page.getByRole("link", { name: "로그인", exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "회원가입 신청" })).toBeVisible();
  await expect(page.getByText("비밀번호를 잊었다면 관리자에게 재설정 링크를 요청하세요.")).toBeVisible();
});

test("a public signup requests approval before the applicant sets a password", async ({ page }) => {
  await mock(page, null);
  const applications: unknown[] = [];
  await page.route("**/api/v1/signup-requests", route => {
    applications.push(route.request().postDataJSON());
    return route.fulfill({ status: 202, json: { status: "received" } });
  });
  await page.goto("/");
  await page.getByRole("link", { name: "회원가입 신청" }).click();
  await expect(page).toHaveURL(/\/signup$/);
  await field(page, "name").fill("가입 신청자");
  await field(page, "email").fill("applicant@example.test");
  await expect(page.locator('input[type="password"]')).toHaveCount(0);
  await page.getByRole("button", { name: "가입 신청하기" }).click();
  await expect(page.getByRole("heading", { name: "가입 신청을 접수했습니다" })).toBeVisible();
  expect(applications).toEqual([{ name: "가입 신청자", email: "applicant@example.test" }]);
});
