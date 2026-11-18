export { createNavigationGuards, type NavigationGuard } from "./navigation";
export { ApiError, checkedResponse, fetchResponse, readJson, readBlob, request, requestBlob, requestResponse, requestHeaders, withAbort,
  type ApiErrorKind } from "./transport";
export { completeDirectoryPage, directoryQuery, type DirectoryQuery, type DirectoryRequestOptions, type DirectoryReader } from "./directory";
import type { DirectoryQuery, DirectoryReader, DirectoryRequestOptions } from "./directory";
export const UI_CONTRACT = "miy.ui/v1alpha1" as const;
export type {
  AcceptanceRecord,
  App,
  AppDistribution,
  AppFrameLaunch,
  AppFrameRenewal,
  AppFrameToken,
  FrameBootstrap,
  FrameLocation,
  AppDetail,
  AuditEntry,
  AuditPage,
  Environment,
  Group,
  GroupSource,
  DirectoryPage,
  DirectoryUser,
  DirectoryGroup,
  Health,
  HealthStatus,
  Installation,
  InstallationKind,
  InstallationPage,
  LastOperation,
  RecordedCheck,
  RuntimeJob,
  RuntimeJobPage,
  UiModule,
} from "./api.gen";
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
   * Report unsaved input or work in progress whenever it changes. The portal asks
   * before leaving the app, signing out or reloading while `dirty` is true.
   */
  setNavigationState: (state: NavigationState) => void;
  theme: "light";
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
