import { lazy, Suspense, type ReactNode } from "react";
import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { Shell } from "./layout/Shell";
import { Home } from "./pages/Home";
import { Admin } from "./pages/Admin";
import { CliInstall } from "./pages/CliInstall";
import { CliAuthorize, CliDeviceApprove } from "./pages/CliAuthorize";
import { useSurface } from "./data/session";
import { ManagedApps } from "./pages/ManagedApps";
import { CliSessions } from "./pages/CliSessions";
import { Loading } from "./ui";

// The console never imports the remote runtime module, including through a direct
// business-app URL. Shell rejects those URLs before rendering this route.
const AppView = lazy(() => import("./app/AppView").then((module) => ({ default: module.AppView })));
function AppRoute() {
  const { surface } = useSurface();
  if (surface === "console") return null;
  return <Suspense fallback={<Loading />}><AppView /></Suspense>;
}
function HomeRoute() {
  return useSurface().surface === "console" ? <ManagedApps /> : <Home />;
}

const root = createRootRoute({ component: Shell });
const page = (path: string, component: () => ReactNode) =>
  createRoute({ getParentRoute: () => root, path, component });

// Paths are a compatibility contract: environment URLs, legacy /apps, CLI approval and admin.
export const router = createRouter({
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
  ]),
});
