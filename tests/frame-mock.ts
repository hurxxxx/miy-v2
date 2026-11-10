import type { Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { extname, resolve, sep } from "node:path";
import type { Installation } from "../packages/sdk/src/index";

// Mocked business app frames (docs/APP-FRAMES.md) for portal UI suites: the launch
// API, each installation's own frame origin (http://127.0.0.1:18190-18199, never
// served for real), the built platform frame runtime and the fixture remote. App
// API calls reach `api` with the installation that the frame origin belongs to.
// Presentation only: these mocks are no evidence for server-side checks.
const evidence = resolve(".local/evidence/runtime-health-ui");
const runtimeDir = resolve(evidence, "frame-runtime");
const remoteDir = resolve(evidence, "remote");
const types: Record<string, string> = {
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".html": "text/html",
  ".woff2": "font/woff2",
};

export type FrameReply = { status?: number; json: unknown };
export type FrameApi = (
  installation: Installation,
  path: string,
  method: string,
  release: string | undefined,
) => FrameReply | Promise<FrameReply>;

function file(root: string, relative: string) {
  const path = resolve(root, relative);
  if (!path.startsWith(root + sep)) throw new Error("mock path escaped its directory");
  return { body: readFileSync(path), contentType: types[extname(path)] ?? "application/octet-stream" };
}

export async function mockFrames(page: Page, apps: () => Installation[], api: FrameApi) {
  const origins = new Map<string, Installation>();
  const originOf = (installation: Installation) => {
    for (const [origin, value] of origins) if (value.id === installation.id) return origin;
    const origin = `http://127.0.0.1:${18190 + origins.size}`;
    origins.set(origin, installation);
    return origin;
  };
  // Registered after the suite's own "**/api/**" mock, so it answers first.
  await page.route("**/api/v1/app-frames", async (route) => {
    const { installationId } = route.request().postDataJSON() as { installationId: string };
    const installation = apps().find((value) => value.id === installationId);
    if (!installation) return route.fulfill({ status: 404, json: { error: { code: "not_found" } } });
    const frameOrigin = originOf(installation);
    await route.fulfill({
      json: {
        frameId: crypto.randomUUID(),
        installationId,
        releaseId: installation.releaseId,
        frameOrigin,
        launchUrl: frameOrigin + "/_miy/launch",
        code: "c".repeat(43),
        expiresAt: new Date(Date.now() + 60_000).toISOString(),
      },
    });
  });
  await page.route(/^http:\/\/127\.0\.0\.1:1819\d\//, async (route) => {
    const url = new URL(route.request().url());
    const installation = origins.get(url.origin);
    if (!installation) return route.fulfill({ status: 404, body: "" });
    if (url.pathname === "/_miy/launch") {
      const form = new URLSearchParams(route.request().postData() ?? "");
      const bootstrap = {
        protocol: "miy.frame/v1",
        frameId: crypto.randomUUID(),
        installationId: installation.id,
        appId: installation.appId,
        releaseId: installation.releaseId,
        environment: installation.environment,
        parentOrigin: new URL(page.url()).origin,
        basePath: form.get("basePath") ?? "/",
        location: { pathname: form.get("pathname") || "/", search: form.get("search") ?? "", hash: form.get("hash") ?? "" },
        token: "miy_frame_" + "t".repeat(43),
        tokenExpiresAt: new Date(Date.now() + 600_000).toISOString(),
        frameExpiresAt: new Date(Date.now() + 8 * 3_600_000).toISOString(),
        assetBase: "/_miy/a/capability/ui/",
        moduleEntry: "mf-manifest.json",
        moduleExport: "./AppRoot",
        contractVersion: installation.ui.contractVersion,
      };
      const html = readFileSync(resolve(runtimeDir, "index.html"), "utf8").replace(
        "</head>",
        `<script id="miy-frame-bootstrap" type="application/json">${JSON.stringify(bootstrap).replaceAll("<", "\\u003c")}</script></head>`,
      );
      return route.fulfill({ contentType: "text/html", body: html });
    }
    if (url.pathname.startsWith("/_miy/runtime/"))
      return route.fulfill(file(runtimeDir, url.pathname.slice("/_miy/runtime/".length)));
    if (url.pathname.startsWith("/_miy/a/capability/ui/"))
      return route.fulfill(file(remoteDir, url.pathname.slice("/_miy/a/capability/ui/".length)));
    if (url.pathname.startsWith("/_miy/api/") || url.pathname === "/_miy/platform/directory") {
      const reply = await api(
        installation,
        url.pathname.replace(/^\/_miy\/api/, ""),
        route.request().method(),
        route.request().headers()["x-miy-release"],
      );
      return route.fulfill({ status: reply.status ?? 200, json: reply.json });
    }
    return route.fulfill({ status: 404, body: "" });
  });
}
