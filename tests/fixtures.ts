import {
  test as base,
  chromium,
  expect,
  type BrowserContext,
  type Page,
} from "@playwright/test";
import { readFileSync } from "node:fs";
export const origin = "https://localhost:18443";
export const credentials = JSON.parse(
  readFileSync(".local/credentials.json", "utf8"),
) as Record<string, string>;
export const test = base.extend({
  browser: async ({}, use) => {
    const browser = await chromium.connectOverCDP(
      process.env.MIY_BROWSER_CDP || "http://127.0.0.1:19222",
    );
    await use(browser);
    await browser.close();
  },
});
export { expect };
export async function login(page: Page, user: string) {
  await page.goto(origin + "/auth/login");
  if (await page.locator("#username").count()) {
    await page.locator("#username").fill(user);
    await page.locator("#password").fill(credentials[user]);
    await page.locator("#kc-login").click();
  }
  await page.waitForURL(origin + "/");
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
}
export async function api(
  context: BrowserContext,
  path: string,
  method = "GET",
  data?: unknown,
) {
  const me = await context.request.get(origin + "/api/v1/me");
  const csrf = (await me.json()).csrf;
  return context.request.fetch(origin + "/api/v1" + path, {
    method,
    data,
    headers: { origin, "x-miy-csrf": csrf },
  });
}
