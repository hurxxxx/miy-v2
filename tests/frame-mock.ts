import type { Page, Route } from "@playwright/test";
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

export type FrameReply = { status?: number; json: unknown; headers?: Record<string, string> };
export type FrameApi = (
  installation: Installation,
  path: string,
  method: string,
  release: string | undefined,
) => FrameReply | Promise<FrameReply>;

type RenewalStage = "portal" | "exchange";
type RenewalReply = FrameReply | "network_error" | undefined;
export interface FrameMockOptions {
  tokenLifetimeMs?: number;
  frameLifetimeMs?: number;
  renewal?: (stage: RenewalStage, attempt: number) => RenewalReply | Promise<RenewalReply>;
}

function file(root: string, relative: string) {
  const path = resolve(root, relative);
  if (!path.startsWith(root + sep)) throw new Error("mock path escaped its directory");
  return { body: readFileSync(path), contentType: types[extname(path)] ?? "application/octet-stream" };
}

export async function mockFrames(page: Page, apps: () => Installation[], api: FrameApi, options: FrameMockOptions = {}) {
  const requests = { launches: 0, portal: 0, exchange: 0 };
  const origins = new Map<string, Installation>();
  const frameExpiries = new Map<string, number>();
  const tokenLifetimeMs = options.tokenLifetimeMs ?? 600_000;
  const frameLifetimeMs = options.frameLifetimeMs ?? 8 * 3_600_000;
  const renewalReply = async (stage: RenewalStage, route: Route) => {
    const attempt = ++requests[stage];
    const reply = await options.renewal?.(stage, attempt);
    if (reply === "network_error") {
      await route.abort("failed");
      return true;
    }
    if (reply) {
      await route.fulfill({ status: reply.status ?? 200, json: reply.json });
      return true;
    }
    return false;
  };
  const originOf = (installation: Installation) => {
    for (const [origin, value] of origins) if (value.id === installation.id) return origin;
    const origin = `http://127.0.0.1:${18190 + origins.size}`;
    origins.set(origin, installation);
    return origin;
  };
  // Registered after the suite's own "**/api/**" mock, so it answers first.
  await page.route("**/api/v1/app-frames", async (route) => {
    requests.launches++;
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
  await page.route("**/api/v1/app-frames/*/renew", async (route) => {
    if (await renewalReply("portal", route)) return;
    await route.fulfill({ json: { code: "r".repeat(43) } });
  });
  await page.route(/^http:\/\/127\.0\.0\.1:1819\d\//, async (route) => {
    const url = new URL(route.request().url());
    const installation = origins.get(url.origin);
    if (!installation) return route.fulfill({ status: 404, body: "" });
    if (url.pathname === "/_miy/launch") {
      // Match the browser clock so tests can advance real runtime timers.
      const now = await page.evaluate(() => Date.now());
      frameExpiries.set(url.origin, now + frameLifetimeMs);
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
        tokenExpiresAt: new Date(now + Math.min(tokenLifetimeMs, frameLifetimeMs)).toISOString(),
        frameExpiresAt: new Date(now + frameLifetimeMs).toISOString(),
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
    if (url.pathname === "/_miy/renew") {
      if (await renewalReply("exchange", route)) return;
      const now = await page.evaluate(() => Date.now());
      return route.fulfill({ json: {
        token: "miy_frame_" + "u".repeat(43),
        tokenExpiresAt: new Date(Math.min(now + tokenLifetimeMs, frameExpiries.get(url.origin)!)).toISOString(),
      } });
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
      return route.fulfill({ status: reply.status ?? 200, json: reply.json, headers: reply.headers });
    }
    return route.fulfill({ status: 404, body: "" });
  });
  return requests;
}
