import { browserUser, defaultPreferences, mockSurface } from "./browser-surface";
import { test, expect, type Page } from "@playwright/test";

// The portal end of a user switch: the code leaves the address before it is
// sent, goes only to this portal's /auth/switch, and success opens the portal.
// Mocked API; the Core regression covers who may switch and the session.
async function open(
  page: Page,
  reply: { status: number; json?: unknown },
  shared = { switched: false, signedIn: false },
) {
  await mockSurface(page, "portal");
  const exchanges: { body: unknown; origin: string | null }[] = [];
  await page.route("**/auth/switch", async (route) => {
    exchanges.push({ body: route.request().postDataJSON(), origin: route.request().headers()["origin"] ?? null });
    if (reply.status === 204) shared.switched = true;
    await route.fulfill(reply.status === 204 ? { status: 204, body: "" } : { status: reply.status, json: reply.json });
  });
  await page.route("**/api/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === "/api/v1/me")
      return shared.switched
        ? route.fulfill({ json: browserUser(route, { id: "alice", name: "앨리스", admin: false, csrf: "s-csrf", preferences: defaultPreferences }) })
        : shared.signedIn
          ? route.fulfill({ json: browserUser(route, { id: "previous", name: "이전 사용자", admin: false, csrf: "old-csrf", preferences: defaultPreferences }) })
          : route.fulfill({ status: 401, json: { error: { code: "unauthorized" } } });
    if (path === "/api/v1/installations") return route.fulfill({ json: [] });
    if (path === "/api/v1/home/cards") return route.fulfill({ json: { cards: [] } });
    if (path === "/api/v1/notifications/unread-count") return route.fulfill({ json: { unread: 0 } });
    if (path === "/api/v1/launcher/categories") return route.fulfill({ json: { categories: [] } });
    await route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
  });
  return { exchanges };
}

test("a switch link signs this portal in as the person and opens the portal", async ({ page }) => {
  const { exchanges } = await open(page, { status: 204 });
  await page.goto("/switch-user#one-time-code");
  await expect(page.getByRole("link", { name: "앨리스" })).toBeVisible();
  expect(exchanges).toHaveLength(1);
  expect(exchanges[0]!.body).toEqual({ code: "one-time-code" });
  expect(exchanges[0]!.origin).toBe(new URL(page.url()).origin);
  expect(page.url()).not.toContain("one-time-code");
});

test("an expired switch link says so and keeps the code out of the address", async ({ page }) => {
  await open(page, { status: 401, json: { error: { code: "unauthorized" } } });
  await page.goto("/switch-user#used-code");
  await expect(page.getByRole("alert")).toContainText("전환 링크가 만료됐거나 이미 사용됐습니다");
  await expect(page).toHaveURL(/\/switch-user$/);
});

test("a switch reloads the signed-in identity in another portal tab", async ({ page, context }) => {
  const shared = { switched: false, signedIn: true };
  await open(page, { status: 204 }, shared);
  await page.goto("/");
  await expect(page.getByRole("link", { name: "이전 사용자" })).toBeVisible();

  const switching = await context.newPage();
  await open(switching, { status: 204 }, shared);
  await switching.goto("/switch-user#one-time-code");
  await expect(switching.getByRole("link", { name: "앨리스" })).toBeVisible();
  await expect(page.getByRole("link", { name: "앨리스" })).toBeVisible();
  await expect(page.getByRole("link", { name: "이전 사용자" })).toHaveCount(0);
});
