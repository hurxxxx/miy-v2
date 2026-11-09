import type { BrowserSurface, LoginOptions } from "../data/session";

export type Environment = "dev" | "prod";

export function environmentPath(environment: string, app?: string) {
  return `/environments/${environment}` + (app ? `/apps/${app}` : "");
}

/**
 * The only app environment a portal runs; the console runs no apps. A server from
 * before the dev portal split serves both environments on its portal.
 */
export function portalEnvironment(
  surface: BrowserSurface,
  legacyPortal = false,
): Environment | null {
  if (legacyPortal) return null;
  return surface === "portal" ? "prod" : surface === "dev_portal" ? "dev" : null;
}

/**
 * Pages without an explicit environment use the surface's own environment (the
 * console defaults to dev management). Legacy `/apps/{app}` links are dev apps.
 */
export function currentEnvironment(
  path: string,
  fallback: Environment = "dev",
): Environment | "invalid" {
  const parts = path.split("/");
  if (parts[1] === "apps") return "dev";
  if (parts[1] !== "environments") return fallback;
  return parts[2] === "dev" || parts[2] === "prod" ? parts[2] : "invalid";
}

/** The environment a path explicitly names, or null for platform pages. */
export function pathEnvironment(path: string): Environment | null {
  const first = path.split("/")[1];
  if (first === "apps") return "dev";
  if (first !== "environments") return null;
  const environment = currentEnvironment(path);
  return environment === "invalid" ? null : environment;
}

export function environmentOrigin(
  options: Pick<LoginOptions, "portalOrigin" | "devPortalOrigin">,
  environment: Environment,
) {
  return environment === "prod" ? options.portalOrigin : options.devPortalOrigin;
}

export function isCliApprovalPath(path: string | null | undefined) {
  return !!path && /^\/cli\/(authorize|device)\/[^/?#\\]+$/.test(path);
}

export function isConsolePath(path: string) {
  const normalized = path.replace(/\/+$/, "");
  return normalized === "/admin" || normalized === "/cli/sessions" ||
    /^\/cli\/(authorize|device)(?:\/|$)/.test(normalized);
}

export function isAppPath(path: string) {
  return /^\/(?:environments\/[^/]+\/)?apps(?:\/|$)/.test(path);
}

/** Setting pathname cannot turn app input into an authority, query or fragment. */
export function surfaceUrl(origin: string, path = "/") {
  const url = new URL(origin);
  url.pathname = path;
  return url.href;
}

// getRandomValues is available on internal HTTP origins as well as HTTPS.
// randomUUID requires a secure context and would break runtime controls on IP HTTP.
export function requestId() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
