import { test, expect } from "./fixtures";

const origin = "http://localhost:18000";
test("development account buttons sign in and out without an identity redirect", async ({
  browser,
}) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  const identityRequests: string[] = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.startsWith("/identity/"))
      identityRequests.push(request.url());
  });
  await page.goto(origin);
  await expect(
    page.getByRole("button", { name: /관리자로 시작/ }),
  ).toBeVisible();
  await page.screenshot({
    path: ".local/evidence/development-accounts.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: /관리자로 시작/ }).click();
  await expect(page.locator("header")).toContainText("admin");
  const me = await (await context.request.get(origin + "/api/v1/me")).json();
  expect(me.admin).toBe(true);
  expect(me.development).toBe(true);
  const cookie = (await context.cookies()).find(
    (cookie) => cookie.name === "miy_development",
  );
  expect(cookie?.httpOnly).toBe(true);
  expect(cookie?.secure).toBe(false);
  await page.getByRole("button", { name: "로그아웃", exact: true }).click();
  await expect(
    page.getByRole("button", { name: /관리자로 시작/ }),
  ).toBeVisible();
  expect((await context.request.get(origin + "/api/v1/me")).status()).toBe(401);
  await page.getByRole("button", { name: /사용자 A/ }).click();
  await expect(page.locator("header")).toContainText("alice");
  const alice = await (await context.request.get(origin + "/api/v1/me")).json();
  expect(alice.admin).toBe(false);
  expect(alice.development).toBe(true);
  expect(identityRequests).toEqual([]);
  await page.screenshot({
    path: ".local/evidence/development-login.png",
    fullPage: true,
  });
  await context.close();
});

test("development login rejects foreign origins and unknown accounts", async ({
  browser,
}) => {
  const context = await browser.newContext();
  const foreign = await context.request.post(origin + "/auth/development", {
    headers: { origin: "https://untrusted.example" },
    data: { account: "admin" },
  });
  expect(foreign.status()).toBe(403);
  const unknown = await context.request.post(origin + "/auth/development", {
    headers: { origin },
    data: { account: "root" },
  });
  expect(unknown.status()).toBe(400);
  expect((await context.request.get(origin + "/api/v1/me")).status()).toBe(401);
  await context.close();
});
