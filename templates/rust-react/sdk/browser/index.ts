export { createNavigationGuards, type NavigationGuard } from "./navigation";
export { ApiError, checkedResponse, fetchResponse, readJson, readBlob, request, requestBlob, requestResponse, requestHeaders, withAbort,
  type ApiErrorKind } from "./transport";
export { completeDirectoryPage, directoryQuery, type DirectoryQuery, type DirectoryRequestOptions, type DirectoryReader } from "./directory";
export type { DirectoryPage, DirectoryUser, DirectoryGroup } from "./api.gen";
import type { DirectoryQuery, DirectoryReader, DirectoryRequestOptions } from "./directory";
export const UI_CONTRACT = "miy.ui/v1alpha1" as const;
export interface Installation {
  id: string;
  appId: string;
  name: string;
  environment: "dev" | "prod";
  releaseId: string;
  version?: string;
  canPromote?: boolean;
  dbStatus: string;
  runtimeStatus: string;
  desiredState: string;
  kind: "validation_fixture" | "docker_app";
  ui: { entry: string; export: "./AppRoot"; contractVersion: string };
}
/** Display settings the person chose in the portal. Presentation only: never an access input. */
export interface AppSettings {
  /** The theme in effect; a `system` preference is already resolved. */
  theme: "light" | "dark";
  themePreference: "system" | "light" | "dark";
  locale: "ko-KR" | "en-US";
  /** IANA time zone name this browser knows. */
  timeZone: string;
  dateFormat: "korean" | "iso" | "us" | "european" | "locale";
}
export interface AppRuntime {
  installationId: string;
  appId: string;
  releaseId: string;
  basePath: string;
  location: { pathname: string; search: string; hash: string };
  navigate: (relative: string, options?: { replace?: boolean }) => void;
  api: <T>(path: string, init?: RequestInit) => Promise<T>;
  /** A binary response of this app's own API (downloads, previews). */
  apiBlob: (path: string, init?: RequestInit) => Promise<Blob>;
  /** App-owned file routes with the platform's separate bounded binary gateway. */
  fileApi: (path: string, init?: RequestInit) => Promise<Response>;
  /**
   * Untyped directory read. Without a query it reads one page and rejects with
   * `directory_page_incomplete` when more entries exist; use `directory` to
   * search and follow `nextCursor`.
   */
  platformApi: <T>(resource: "directory", query?: DirectoryQuery, options?: DirectoryRequestOptions) => Promise<T>;
  directory: DirectoryReader;
  appApi: <T>(appId: string, path: string, init?: RequestInit) => Promise<T>;
  /**
   * Report unsaved input or work in progress whenever it changes. The platform asks
   * before leaving the app, signing out or reloading while `dirty` is true.
   */
  setNavigationState: (state: NavigationState) => void;
  /** Same as `settings.theme`. */
  theme: "light" | "dark";
  /**
   * Display settings. When the person changes one, AppRoot renders again with a
   * new runtime; the platform has already set the theme and `lang` on this document.
   */
  settings: AppSettings;
}
export interface NavigationState {
  dirty: boolean;
  /** Shown when the user is asked to confirm leaving. */
  message?: string;
}
export interface AppProps {
  runtime: AppRuntime;
}
export function appTarget(basePath: string, relative: string): string {
  if (!relative.startsWith("/") || relative.startsWith("//"))
    throw new Error("Use an app-relative path");
  const u = new URL(basePath + relative, "https://miy.invalid");
  if (
    u.origin !== "https://miy.invalid" ||
    !u.pathname.startsWith(basePath + "/")
  )
    throw new Error("Navigation outside this app is denied");
  return u.pathname + u.search + u.hash;
}
