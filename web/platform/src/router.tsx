import type { ReactNode } from "react";
import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { Shell } from "./layout/Shell";
import { Home } from "./pages/Home";
import { Admin } from "./pages/Admin";
import { CliInstall } from "./pages/CliInstall";
import { CliAuthorize, CliDeviceApprove } from "./pages/CliAuthorize";
import { AppView } from "./app/AppView";

const root = createRootRoute({ component: Shell });
const page = (path: string, component: () => ReactNode) =>
  createRoute({ getParentRoute: () => root, path, component });

// Paths are a compatibility contract: environment URLs, legacy /apps, CLI approval and admin.
export const router = createRouter({
  routeTree: root.addChildren([
    page("/", Home),
    page("/apps/$appId/$", AppView),
    page("/apps/$appId", AppView),
    page("/environments/$environment", Home),
    page("/environments/$environment/apps/$appId", AppView),
    page("/environments/$environment/apps/$appId/$", AppView),
    page("/cli/install", CliInstall),
    page("/cli/authorize/$id", CliAuthorize),
    page("/cli/device/$id", CliDeviceApprove),
    page("/admin", Admin),
  ]),
});
