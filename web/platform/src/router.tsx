import { lazy, Suspense, type ReactNode } from "react";
import { createRootRoute, createRoute, createRouter, defaultParseSearch, defaultStringifySearch } from "@tanstack/react-router";
import { Shell } from "./layout/Shell";
import { Home } from "./pages/Home";
import { Admin } from "./pages/Admin";
import { CliInstall } from "./pages/CliInstall";
import { CliAuthorize, CliDeviceApprove } from "./pages/CliAuthorize";
import { useSurface } from "./data/session";
import { ManagedApps } from "./pages/ManagedApps";
import { CliSessions } from "./pages/CliSessions";
import { SettingsPage } from "./pages/settings/SettingsPage";
import { Search } from "./pages/Search";
import { Loading } from "./ui";

// The console never imports the remote runtime module, including through a direct
// business-app URL. Shell rejects those URLs before rendering this route.
const AppView = lazy(() => import("./app/AppView").then((module) => ({ default: module.AppView })));
function AppRoute() {
  const { surface } = useSurface();
  if (surface === "console") return null;
  return <Suspense fallback={<Loading />}><AppView /></Suspense>;
}
function SearchRoute() {
  // The console has no business apps to search.
  return useSurface().surface === "console" ? <ManagedApps /> : <Search />;
}
function HomeRoute() {
  return useSurface().surface === "console" ? <ManagedApps /> : <Home />;
}

const root = createRootRoute({ component: Shell });
const page = (path: string, component: () => ReactNode) =>
  createRoute({ getParentRoute: () => root, path, component });

// Paths are a compatibility contract: environment URLs, legacy /apps, CLI approval and admin.
export const router = createRouter({
  // q is literal search text, including values that happen to be valid JSON.
  parseSearch: (search) => {
    const params = defaultParseSearch(search);
    const q = new URLSearchParams(search).get("q");
    return q === null ? params : { ...params, q };
  },
  stringifySearch: (search) => {
    const encoded = defaultStringifySearch(search);
    if (typeof search.q !== "string") return encoded;
    const params = new URLSearchParams(encoded);
    params.set("q", search.q);
    return `?${params}`;
  },
  routeTree: root.addChildren([
    page("/", HomeRoute),
    page("/apps/$appId/$", AppRoute),
    page("/apps/$appId", AppRoute),
    page("/environments/$environment", HomeRoute),
    page("/environments/$environment/apps/$appId", AppRoute),
    page("/environments/$environment/apps/$appId/$", AppRoute),
    page("/cli/install", CliInstall),
    page("/cli/authorize/$id", CliAuthorize),
    page("/cli/device/$id", CliDeviceApprove),
    page("/cli/sessions", CliSessions),
    page("/admin", Admin),
    page("/settings", SettingsPage),
    page("/settings/$section", SettingsPage),
    page("/search", SearchRoute),
    // The end of a user switch; the shell renders it before any session check.
    page("/switch-user", () => null),
    // Public account-link pages; the shell renders them before any session check.
    page("/join", () => null),
    page("/reset-password", () => null),
  ]),
});
