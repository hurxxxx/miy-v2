import type { BinaryHandlers, BinaryConnection } from "./live-binary";
export { connectBinary, type BinaryHandlers, type BinaryConnection } from "./live-binary";
export { createNavigationGuards, type NavigationGuard } from "./navigation";
export { ApiError, isApiError, checkedResponse, fetchResponse, readJson, readBlob, request, requestBlob, requestResponse, requestHeaders, withAbort,
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
/** `fileApi` request options: an upload may report its progress. */
export type FileRequestInit = RequestInit & { onUploadProgress?: (sent: number, total: number) => void };
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
/** Handlers of a live connection (`runtime.live`). */
export interface LiveHandlers {
  /** The app accepted the connection (also after each reconnection). */
  onOpen?: () => void;
  /** A text message from the app's `/api/live`. */
  onMessage: (data: string) => void;
  /**
   * The connection closed. `retrying` is true when the runtime reconnects by
   * itself; false when the frame session ended, access was revoked, the release
   * changed or `close()` was called.
   */
  onClose?: (event: { code: number; retrying: boolean }) => void;
}
export interface LiveConnection {
  /** Send a text message (up to 64 KiB) while open. */
  send: (data: string) => void;
  close: () => void;
}
/**
 * Route and display updates produce a new runtime value. The methods keep
 * their identity for the lifetime of the frame or local preview session.
 */
export interface AppRuntime {
  installationId: string;
  appId: string;
  releaseId: string;
  basePath: string;
  location: { pathname: string; search: string; hash: string };
  navigate: (relative: string, options?: { replace?: boolean }) => void;
  /**
   * The full portal address of an app-relative path (for links people copy
   * or share). The frame cannot read the portal address itself.
   */
  portalUrl?: (relative: string) => string;
  api: <T>(path: string, init?: RequestInit) => Promise<T>;
  /** A binary response of this app's own API (downloads, previews). */
  apiBlob: (path: string, init?: RequestInit) => Promise<Blob>;
  /**
   * App-owned file routes with the platform's separate bounded binary gateway.
   * `onUploadProgress(sent, total)` reports an upload's progress (a Blob,
   * ArrayBuffer or string body; not a stream).
   */
  fileApi: (path: string, init?: FileRequestInit) => Promise<Response>;
  /**
   * Start a browser download from a link the app's backend got with
   * `Identity::files_download_link` (`/_miy/downloads/...`). The bytes stream
   * to the browser's downloads without passing through memory; the platform
   * rechecks access for every request. Not available in local preview.
   */
  download?: (url: string) => void;
  /**
   * Start a native whole-file download and wait for Core's receipt.
   * With `checkOnly`, resume checking the same link without starting a download.
   * A confirmed failure rejects with `file_download_failed`; other errors leave
   * the transfer unconfirmed. Aborting stops the wait.
   */
  downloadAndWait?: (url: string, signal?: AbortSignal, options?: { checkOnly?: boolean }) => Promise<void>;
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
  /**
   * A live connection to this app's `/api/live` (a WebSocket relayed by the
   * platform with the person's verified identity). Use it for updates that
   * should arrive at once; keep reads and writes on `api`.
   */
  liveBinary: (handlers: BinaryHandlers) => BinaryConnection;
  live: (handlers: LiveHandlers) => LiveConnection;
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
