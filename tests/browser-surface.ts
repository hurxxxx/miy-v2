import type { Page, Route } from "@playwright/test";

type Surface = "portal" | "console";
const surfaces = new WeakMap<Page, Surface>();

export function browserOptions(route: Route, surface?: Surface) {
  const current =
    surface ?? surfaces.get(route.request().frame().page()) ?? "portal";
  const origin = new URL(route.request().url()).origin;
  return {
    surface: current,
    portalOrigin: current === "portal" ? origin : "https://portal.example.test",
    consoleOrigin:
      current === "console" ? origin : "https://console.example.test",
    developmentLogin: false,
    developmentOrigin: origin,
    accounts: [],
    oauthUrl: "/auth/login",
  };
}

export function browserUser(route: Route, user: object) {
  const options = browserOptions(route);
  return {
    ...user,
    browserPurpose: options.surface,
    portalOrigin: options.portalOrigin,
    consoleOrigin: options.consoleOrigin,
    canManageConsole: true,
  };
}

export async function mockSurface(page: Page, surface: Surface = "portal") {
  surfaces.set(page, surface);
  await page.route("**/auth/options", (route) =>
    route.fulfill({ json: browserOptions(route) }),
  );
}
