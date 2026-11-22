/**
 * AI-DO packages/core-web `buildAppRoutePath` (app-first canonical routes
 * `/<app>/s/<spaceKey><path>`). The ported PMS screens reach it only in their
 * app-first runtime branches, which never run in miy (see
 * platform/app-runtime/index.ts: `useOptionalAppRuntime()` is null). It
 * returns the app-relative path so a stray call still stays inside the app.
 */
export type AppRouteContext = { routeBase: string; spaceKey: string | null };

export function buildAppRoutePath(_context: AppRouteContext, path = ''): string {
  if (!path) return '/';
  return path.startsWith('/') ? path : `/${path}`;
}
