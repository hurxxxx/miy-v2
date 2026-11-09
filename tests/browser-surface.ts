import type { Page, Route } from "@playwright/test";

export type Surface = "portal" | "dev_portal" | "console";
// "portal" by default follows the page's environment: each environment has its own
// business portal origin (prod portal, dev portal). Mocked pages share one origin, so
// the mock answers as the portal that a real server would serve for that path.
const surfaces = new WeakMap<Page, Surface>();

function portalFor(route: Route): Surface {
  let path = "/";
  try {
    path = new URL(route.request().frame().url()).pathname;
  } catch {
    // keep the default
  }
  return /^\/(?:environments\/dev(?:\/|$)|apps(?:\/|$))/.test(path)
    ? "dev_portal"
    : "portal";
}

export function browserOptions(route: Route, surface?: Surface) {
  const chosen =
    surface ?? surfaces.get(route.request().frame().page()) ?? "portal";
  const current = chosen === "portal" && !surface ? portalFor(route) : chosen;
  const origin = new URL(route.request().url()).origin;
  return {
    surface: current,
    portalOrigin: current === "portal" ? origin : "https://portal.example.test",
    devPortalOrigin:
      current === "dev_portal" ? origin : "https://dev-portal.example.test",
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
    devPortalOrigin: options.devPortalOrigin,
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
