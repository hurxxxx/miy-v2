import {
  test as base,
  chromium,
  expect,
  type BrowserContext,
  type Page,
} from "@playwright/test";
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { join, resolve } from "node:path";

function required(name: string) {
  const value = process.env[name];
  if (!value)
    throw new Error(
      `${name} must explicitly identify the disposable test stack`,
    );
  return value;
}
export const stateDir = realpathSync(required("MIY_STATE_DIR"));
const canonical = (path: string) =>
  existsSync(path) ? realpathSync(path) : resolve(path);
if (stateDir === canonical(".local"))
  throw new Error("validation refuses the operational .local directory");
export const databaseContainer = required("MIY_DATABASE_CONTAINER");
if (
  !/^[a-zA-Z0-9][a-zA-Z0-9_.-]+$/.test(databaseContainer) ||
  /^miy-v2-validation(?:-|$)/.test(databaseContainer)
)
  throw new Error(
    "validation refuses an invalid or operational database container",
  );
export const origin = required("MIY_TEST_ORIGIN");
const address = new URL(origin);
if (
  address.origin !== origin ||
  !["https:", "http:"].includes(address.protocol) ||
  !["localhost", "127.0.0.1", "[::1]"].includes(address.hostname)
)
  throw new Error("validation requires an explicit loopback portal origin");
export const webDist = realpathSync(required("MIY_TEST_WEB_DIST"));
if (webDist === canonical("web/platform/dist"))
  throw new Error("validation requires an isolated web build");
required("MIY_FIXTURE_DIST");
const publicConfig = JSON.parse(
  readFileSync(join(stateDir, "public.json"), "utf8"),
);
if (publicConfig.origin !== origin)
  throw new Error("test origin must match the selected state directory");

export type Surface = "portal" | "dev_portal" | "console";
export async function surfaceOrigin(context: BrowserContext, surface: Surface) {
  const options = await (
    await context.request.get(origin + "/auth/options")
  ).json();
  if (options.portalOrigin !== origin || options.surface !== "portal")
    throw new Error("test surface mismatch");
  for (const separate of [options.consoleOrigin, options.devPortalOrigin]) {
    const address = new URL(separate);
    if (
      !["https:", "http:"].includes(address.protocol) ||
      !["localhost", "127.0.0.1", "[::1]"].includes(address.hostname) ||
      address.origin !== separate ||
      separate === origin
    )
      throw new Error("validation requires separate loopback console and dev portal origins");
  }
  if (options.consoleOrigin === options.devPortalOrigin)
    throw new Error("validation requires separate loopback console and dev portal origins");
  return (
    { portal: options.portalOrigin, dev_portal: options.devPortalOrigin, console: options.consoleOrigin }[surface]
  ) as string;
}
export const test = base.extend({
  browser: async ({}, use) => {
    const browser = await chromium.connectOverCDP(required("MIY_BROWSER_CDP"));
    await use(browser);
    await browser.close();
  },
});
export { expect };
export async function login(
  page: Page,
  user: string,
  surface: Surface = "portal",
) {
  const selectedOrigin = await surfaceOrigin(page.context(), surface);
  const credentials = JSON.parse(
    readFileSync(join(stateDir, "credentials.json"), "utf8"),
  ) as Record<string, string>;
  await page.goto(selectedOrigin + "/auth/login");
  if (await page.locator("#username").count()) {
    await page.locator("#username").fill(user);
    await page.locator("#password").fill(credentials[user]);
    await page.locator("#kc-login").click();
  }
  await page.waitForURL(selectedOrigin + "/");
  await expect(page.getByRole("button", { name: "로그아웃" })).toBeVisible();
}
export async function api(
  context: BrowserContext,
  path: string,
  method = "GET",
  data?: unknown,
  surface: Surface = "console",
) {
  const selectedOrigin = await surfaceOrigin(context, surface);
  const me = await context.request.get(selectedOrigin + "/api/v1/me");
  const csrf = (await me.json()).csrf;
  return context.request.fetch(selectedOrigin + "/api/v1" + path, {
    method,
    data,
    headers: { origin: selectedOrigin, "x-miy-csrf": csrf },
  });
}
