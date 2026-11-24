/**
 * Platform frame runtime (docs/APP-FRAMES.md): runs one installation's `./AppRoot`
 * in that installation's own origin. API calls go to this origin with the frame's
 * in-memory token; the bridge to the portal carries screen state only.
 */
import "./frame.css";
import {
  Component,
  Suspense,
  lazy,
  useEffect,
  useMemo,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";
import { loadRemote, registerRemotes } from "@module-federation/enhanced/runtime";
import {
  UI_CONTRACT,
  ApiError,
  appTarget,
  checkedResponse,
  completeDirectoryPage,
  directoryQuery,
  fetchResponse,
  readBlob,
  readJson,
  requestHeaders,
  withAbort,
  type AppFrameToken,
  type AppProps,
  type AppRuntime,
  type AppSettings,
  type FrameBootstrap,
  type LiveConnection,
  type LiveHandlers,
  connectBinary,
  type BinaryHandlers,
  type NavigationState,
  type DirectoryQuery,
  type DirectoryRequestOptions,
} from "@miy/sdk";
import {
  DEFAULT_SETTINGS,
  PROTOCOL,
  envelope,
  platformShortcut,
  readParentMessage,
  relativePath,
  renewalFailure,
  settings as readSettings,
  type FrameLocation,
  type FrameMessage,
  type FrameSettings,
  type RenewalFailure,
} from "@miy/frame-protocol";

function readBootstrap(): FrameBootstrap {
  const node = document.getElementById("miy-frame-bootstrap");
  if (!node?.textContent) throw new Error("frame_bootstrap_missing");
  const value = JSON.parse(node.textContent) as FrameBootstrap;
  // The token lives only in memory from here on.
  node.remove();
  if (value.protocol !== PROTOCOL || window.parent === window) throw new Error("frame_bootstrap_invalid");
  return value;
}

const boot = readBootstrap();
const post = (message: FrameMessage) => window.parent.postMessage(envelope(message), boot.parentOrigin);

let token = boot.token;
let tokenExpiresAt = Date.parse(boot.tokenExpiresAt);
const frameExpiresAt = Date.parse(boot.frameExpiresAt);
let stopped: Exclude<RenewalFailure, "token_renewal_unavailable"> | null = null;
let renewal: Promise<void> | null = null;
let pending: { resolve: (code: string) => void; reject: (error: Error) => void } | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
let expiryTimer: ReturnType<typeof setTimeout> | undefined;

function renewalError(reason: RenewalFailure): Error {
  if (reason !== "token_renewal_unavailable" && !stopped) {
    stopped = reason;
    clearTimeout(timer);
    clearTimeout(expiryTimer);
    pending?.reject(new Error(reason));
    if (reason === "release_changed_reload_required") post({ type: "release.changed", payload: {} });
    else post({ type: "frame.error", payload: { code: reason } });
  }
  return new Error(reason);
}

/** One renewal at a time: ask the portal for a code, exchange it on this origin. */
function renew(): Promise<void> {
  if (stopped) return Promise.reject(new Error(stopped));
  if (renewal) return renewal;
  if (Date.now() >= frameExpiresAt) return Promise.reject(renewalError("frame_session_ended"));
  let timeout: ReturnType<typeof setTimeout>;
  renewal = new Promise<string>((resolve, reject) => {
    pending = { resolve, reject };
    post({ type: "token.renew", payload: {} });
    timeout = setTimeout(() => reject(new Error("token_renewal_unavailable")), 15_000);
  })
    .then(async (code) => {
      clearTimeout(timeout);
      if (stopped) throw new Error(stopped);
      if (Date.now() >= frameExpiresAt) throw renewalError("frame_session_ended");
      const response = await fetch("/_miy/renew", {
        method: "POST",
        mode: "same-origin",
        credentials: "omit",
        referrerPolicy: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code }),
      });
      if (!response.ok) {
        const value = await response.json().catch(() => null);
        throw renewalError(renewalFailure(response.status, value?.error?.code));
      }
      const value = (await response.json()) as AppFrameToken;
      if (stopped) throw new Error(stopped);
      if (Date.now() >= frameExpiresAt) throw renewalError("frame_session_ended");
      token = value.token;
      tokenExpiresAt = Date.parse(value.tokenExpiresAt);
      schedule();
    })
    .catch((error: unknown) => {
      const reason = error instanceof Error ? error.message : "";
      throw renewalError(
        reason === "frame_session_ended" || reason === "frame_access_denied" || reason === "release_changed_reload_required"
          ? reason
          : "token_renewal_unavailable",
      );
    })
    .finally(() => {
      clearTimeout(timeout);
      renewal = null;
      pending = null;
    });
  return renewal;
}
function schedule() {
  clearTimeout(timer);
  if (stopped) return;
  if (tokenExpiresAt >= frameExpiresAt) return;
  const wait = Math.max(5_000, tokenExpiresAt - Date.now() - 60_000);
  timer = setTimeout(scheduledRenewal, wait);
}
function scheduledRenewal() {
  void renew().catch(() => {
    // A transient failure does not end the frame or rotate it into a new session.
    if (!stopped) timer = setTimeout(scheduledRenewal, 30_000);
  });
}
// Absolute expiry also applies while renewal is pending or waiting to retry.
expiryTimer = setTimeout(() => renewalError("frame_session_ended"), Math.max(0, frameExpiresAt - Date.now()));
schedule();

/** Close codes of the Core's live relay (crates/miy/src/live.rs). */
const LIVE_SESSION_ENDED = 4401;
const LIVE_ACCESS_REVOKED = 4403;
const LIVE_RELEASE_CHANGED = 4409;
const LIVE_BAD_AUTH = 4400;
const LIVE_RETRY_MAX = 30_000;

/**
 * A live connection to the app's `/api/live`, relayed by the platform. The
 * frame token goes in the first message, never in the address. The runtime
 * reconnects with backoff after drops and stops for good when the frame
 * session ends, access is revoked or the release changes.
 */
function live(handlers: LiveHandlers): LiveConnection {
  let socket: WebSocket | null = null;
  let closed = false;
  let attempt = 0;
  let retry: ReturnType<typeof setTimeout> | undefined;
  const retryOpen = () => {
    if (closed || stopped) return;
    attempt += 1;
    retry = setTimeout(() => void open(), Math.min(LIVE_RETRY_MAX, 1_000 * 2 ** Math.min(attempt, 5)));
  };
  const open = async () => {
    if (closed || stopped) return;
    try {
      if (Date.now() > tokenExpiresAt - 5_000) await renew();
    } catch {
      retryOpen();
      return;
    }
    if (closed || stopped) return;
    const url = new URL("/_miy/live", window.location.href);
    url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const current = new WebSocket(url.href);
    socket = current;
    let ready = false;
    current.onopen = () => current.send(JSON.stringify({ type: "auth", token }));
    current.onmessage = (event) => {
      if (typeof event.data !== "string") return;
      if (!ready) {
        // The Core's first message says the app accepted the connection.
        ready = event.data === '{"type":"miy.ready"}';
        if (ready) {
          attempt = 0;
          handlers.onOpen?.();
        }
        return;
      }
      handlers.onMessage(event.data);
    };
    current.onclose = (event) => {
      if (socket === current) socket = null;
      if (event.code === LIVE_SESSION_ENDED) renewalError("frame_session_ended");
      else if (event.code === LIVE_ACCESS_REVOKED) renewalError("frame_access_denied");
      else if (event.code === LIVE_RELEASE_CHANGED) renewalError("release_changed_reload_required");
      const final =
        closed || !!stopped || event.code === LIVE_BAD_AUTH || event.code === LIVE_SESSION_ENDED ||
        event.code === LIVE_ACCESS_REVOKED || event.code === LIVE_RELEASE_CHANGED;
      handlers.onClose?.({ code: event.code, retrying: !final });
      if (!final) retryOpen();
    };
  };
  void open();
  return {
    send(data: string) {
      if (!socket || socket.readyState !== WebSocket.OPEN) throw new Error("live_not_open");
      if (new Blob([data]).size > 64 * 1024) throw new Error("live_message_too_large");
      socket.send(data);
    },
    close() {
      closed = true;
      clearTimeout(retry);
      socket?.close(1000);
    },
  };
}

function apiPath(path: string) {
  if (!relativePath(path) || path.split("/").some((part) => part === ".." || part === "."))
    throw new Error("invalid_api_path");
  return path;
}

/** Core's mark on its own 401 (crates/miy/src/app_frames.rs CREDENTIAL_REFUSED). */
const CREDENTIAL_REFUSED = "x-miy-frame-credential";

/** A request to this frame origin only: no cookies, the frame token, its release. */
/** `fileApi` options: an upload may report its progress (sent bytes of total). */
type FileInit = RequestInit & { onUploadProgress?: (sent: number, total: number) => void };

/**
 * The same request through XMLHttpRequest, only to report upload progress
 * (fetch cannot). Headers, method, body, abort and the answer are the same.
 */
function progressResponse(path: string, init: FileInit, headers: Headers): Promise<Response> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open(init.method ?? "GET", path);
    headers.forEach((value, name) => xhr.setRequestHeader(name, value));
    xhr.responseType = "blob";
    const body = init.body ?? null;
    const size =
      body instanceof Blob ? body.size
      : body instanceof ArrayBuffer ? body.byteLength
      : ArrayBuffer.isView(body) ? body.byteLength
      : typeof body === "string" ? new Blob([body]).size
      : 0;
    let reported = -1;
    const report = (sent: number) => {
      if (sent <= reported) return;
      reported = sent;
      init.onUploadProgress?.(sent, size);
    };
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) report(Math.min(event.loaded, size));
    };
    xhr.onload = () => {
      // Every completed upload ends at 100%, whatever events the browser gave.
      report(size);
      const answer = new Headers();
      for (const line of xhr.getAllResponseHeaders().trim().split(/\r?\n/)) {
        const at = line.indexOf(":");
        if (at > 0) answer.append(line.slice(0, at).trim(), line.slice(at + 1).trim());
      }
      const empty = [101, 204, 205, 304].includes(xhr.status);
      resolve(new Response(empty ? null : (xhr.response as Blob), { status: xhr.status, statusText: xhr.statusText, headers: answer }));
    };
    xhr.onerror = () => reject(new ApiError("network", "network_unavailable"));
    xhr.onabort = () => reject(new ApiError("abort", "request_aborted"));
    init.signal?.addEventListener("abort", () => xhr.abort(), { once: true });
    xhr.send(body as XMLHttpRequestBodyInit | null);
  });
}

async function call(path: string, init: FileInit = {}): Promise<Response> {
  if (init.signal?.aborted) throw new ApiError("abort", "request_aborted");
  if (stopped) throw new Error(stopped);
  if (Date.now() >= frameExpiresAt) throw renewalError("frame_session_ended");
  if (Date.now() > tokenExpiresAt - 5_000) await withAbort(renew(), init.signal);
  const send = () => {
    if (init.signal?.aborted) throw new ApiError("abort", "request_aborted");
    if (stopped) throw new Error(stopped);
    if (Date.now() >= frameExpiresAt) throw renewalError("frame_session_ended");
    const headers = requestHeaders(init);
    headers.set("Authorization", `Bearer ${token}`);
    headers.set("x-miy-release", boot.releaseId);
    const { onUploadProgress, ...request } = init;
    const streamed = typeof ReadableStream !== "undefined" && request.body instanceof ReadableStream;
    if (onUploadProgress && request.body && !streamed) return progressResponse(path, init, headers);
    return fetchResponse(path, {
      ...request,
      headers,
      mode: "same-origin",
      credentials: "omit",
      referrerPolicy: "same-origin",
    });
  };
  let response = await send();
  // Only a 401 that Core marks as a refused frame credential is resent: that
  // request never reached the app, so resending cannot repeat a business write.
  // An app's own 401 is a business answer and is returned as it is.
  if (
    response.status === 401 &&
    response.headers.get(CREDENTIAL_REFUSED) === "rejected" &&
    Date.now() < frameExpiresAt &&
    !(typeof ReadableStream !== "undefined" && init.body instanceof ReadableStream)
  ) {
    await response.body?.cancel().catch(() => {});
    await withAbort(renew(), init.signal);
    response = await send();
  }
  if (response.status === 409) {
    const data = await response
      .clone()
      .json()
      .catch(() => null);
    if (data?.error?.code === "release_changed_reload_required") renewalError("release_changed_reload_required");
  }
  // A business API can also return 401. Only a refused renewal or absolute
  // frame expiry establishes that the platform session ended.
  if (Date.now() >= frameExpiresAt) throw renewalError("frame_session_ended");
  return response;
}
async function json<T>(path: string, init?: RequestInit): Promise<T> {
  return readJson<T>(await call(path, init), init);
}
async function blob(path: string, init?: RequestInit): Promise<Blob> {
  return readBlob(await call(path, init), init);
}

function downloadPath(url: string): string {
  if (!/^\/_miy\/downloads\/[A-Za-z0-9_-]{43}$/.test(url)) throw new Error("invalid_download_link");
  return url;
}

function startNativeDownload(path: string) {
  const anchor = document.createElement("a");
  anchor.href = path;
  anchor.download = "";
  anchor.rel = "noopener";
  anchor.hidden = true;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}

const downloadAndWait: NonNullable<AppRuntime["downloadAndWait"]> = async (url, signal, options) => {
  if (signal?.aborted) throw new ApiError("abort", "request_aborted");
  const path = downloadPath(url);
  if (!options?.checkOnly) startNativeDownload(`${path}?track=1`);
  const startDeadline = Date.now() + 30_000;
  let streaming = false;
  let failures = 0;
  for (;;) {
    try {
      const { status } = await json<{ status: string }>(`${path}/status`, { signal });
      if (status === "sent") return;
      if (status === "failed") throw new ApiError("http", "file_download_failed");
      if (status === "streaming") streaming = true;
      else if (status !== "pending") throw new Error("file_download_status_invalid");
      else if (streaming || Date.now() >= startDeadline) {
        throw new Error("file_download_start_unconfirmed");
      }
      failures = 0;
    } catch (error) {
      if (!(error instanceof ApiError) || !error.retryable || ++failures >= 3) throw error;
    }
    await withAbort(new Promise<void>((resolve) => window.setTimeout(resolve, 1_000)), signal);
  }
};

// Display settings. An app built before settings existed is never switched:
// its document stays light and Korean, as it was written.
const DARK = window.matchMedia("(prefers-color-scheme: dark)");
const settingsListeners = new Set<(settings: FrameSettings) => void>();
let currentSettings: FrameSettings = boot.settingsAware
  ? (readSettings(boot.settings) ?? DEFAULT_SETTINGS)
  : { ...DEFAULT_SETTINGS, theme: "light" };
function knownZone(zone: string): string {
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: zone });
    return zone;
  } catch {
    return DEFAULT_SETTINGS.timeZone;
  }
}
function appSettings(settings: FrameSettings, systemDark: boolean): AppSettings {
  return {
    theme: settings.theme === "system" ? (systemDark ? "dark" : "light") : settings.theme,
    themePreference: settings.theme,
    locale: settings.locale,
    timeZone: knownZone(settings.timeZone),
    dateFormat: settings.dateFormat,
  };
}
/** `.dark` is what AI-DO styles key on; the attribute is what common UI 1.1.0 keys on. */
function applyDocumentSettings(settings: AppSettings) {
  if (!boot.settingsAware) return;
  const root = document.documentElement;
  root.classList.toggle("dark", settings.theme === "dark");
  root.dataset.miyTheme = settings.theme;
  root.lang = settings.locale;
}
applyDocumentSettings(appSettings(currentSettings, DARK.matches));

// The bridge listener: only the parent window at the expected origin, only
// validated messages. Route changes reach the app through a subscription.
let tabActive = true;
const activityListeners = new Set<() => void>();
const activity = {
  getSnapshot: () => tabActive,
  subscribe(listener: () => void) {
    activityListeners.add(listener);
    return () => { activityListeners.delete(listener); };
  },
};
const routeListeners = new Set<(location: FrameLocation) => void>();
let currentLocation: FrameLocation = boot.location;
window.addEventListener("message", (event) => {
  if (event.source !== window.parent || event.origin !== boot.parentOrigin) return;
  const message = readParentMessage(event.data);
  if (!message) return;
  if (message.type === "activity.changed") {
    if (tabActive !== message.payload.active) {
      tabActive = message.payload.active;
      for (const listener of activityListeners) listener();
    }
  } else if (message.type === "route.changed") {
    currentLocation = message.payload;
    for (const listener of routeListeners) listener(message.payload);
  } else if (message.type === "settings.changed") {
    if (!boot.settingsAware) return;
    currentSettings = message.payload;
    for (const listener of settingsListeners) listener(message.payload);
  } else if (message.type === "token.code") pending?.resolve(message.payload.code);
  else if (message.type === "token.failed") pending?.reject(new Error(message.payload.reason));
});
post({ type: "frame.ready", payload: {} });

// Input state for the portal's leave checks. A legacy callback guard cannot be
// evaluated across the frame boundary, so it keeps the state unknown.
let reported = false;
let legacy = false;
function setNavigationState(state: NavigationState) {
  reported = true;
  if (legacy) return;
  post({
    type: "guard.state",
    payload: { state: state.dirty ? "dirty" : "clean", ...(state.message ? { message: state.message } : {}) },
  });
}
function registerNavigationGuard() {
  legacy = true;
  post({ type: "guard.state", payload: { state: "unknown" } });
  return () => undefined;
}

// Platform shortcuts (Ctrl/⌘+K) typed in the frame: keys never reach the portal
// document, so forward them. Captured first, so an app cannot keep the platform
// shortcut for itself; nothing else is forwarded.
document.addEventListener(
  "keydown",
  (event) => {
    const name = platformShortcut(event);
    if (!name || event.repeat) return;
    event.preventDefault();
    event.stopPropagation();
    post({ type: "shortcut", payload: { name } });
  },
  { capture: true },
);

// Links: app paths go through the portal router, other links open outside the frame.
document.addEventListener("click", (event) => {
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  const anchor = (event.target as Element | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
  if (!anchor || anchor.hasAttribute("download") || (anchor.target && anchor.target !== "_self")) return;
  const url = new URL(anchor.href, window.location.href);
  event.preventDefault();
  if (url.origin === window.location.origin) {
    const base = boot.basePath;
    if (url.pathname === base || url.pathname.startsWith(base + "/")) {
      const path = (url.pathname.slice(base.length) || "/") + url.search + url.hash;
      post({ type: "route.navigate", payload: { path, replace: false } });
    }
    return;
  }
  if (url.protocol === "https:" || url.protocol === "http:") window.open(url.href, "_blank", "noopener,noreferrer");
});

// The runtime's own status lines are platform text: they follow the person's
// language even for apps built before display settings.
const MESSAGES = {
  "ko-KR": {
    renderFailed: "앱 화면을 표시할 수 없습니다. 앱을 다시 열거나 관리자에게 문의하세요.",
    loading: "앱 화면을 불러오는 중…",
    unsupported: "지원하지 않는 앱 화면입니다.",
  },
  "en-US": {
    renderFailed: "The app screen cannot be shown. Open the app again or contact an administrator.",
    loading: "Loading the app screen…",
    unsupported: "This app screen is not supported.",
  },
} as const;
function message(key: keyof (typeof MESSAGES)["ko-KR"]): string {
  return (MESSAGES[boot.settings?.locale] ?? MESSAGES["ko-KR"])[key];
}

class Boundary extends Component<{ children: ReactNode }, { error: boolean }> {
  state = { error: false };
  static getDerivedStateFromError() {
    return { error: true };
  }
  componentDidCatch() {
    post({ type: "frame.error", payload: { code: "app_render_failed" } });
  }
  render() {
    return this.state.error ? (
      <p className="miy-frame-status" role="alert">
        {message("renderFailed")}
      </p>
    ) : (
      this.props.children
    );
  }
}

function remote(): ComponentType<AppProps> {
  if (boot.contractVersion !== UI_CONTRACT) throw new Error("unsupported_ui_contract");
  const entry = new URL(boot.assetBase + boot.moduleEntry, window.location.origin);
  if (entry.origin !== window.location.origin || !entry.pathname.startsWith(boot.assetBase))
    throw new Error("untrusted_ui_origin");
  const key = "miy_" + boot.releaseId.replaceAll("-", "");
  registerRemotes([{ name: key, entry: entry.href }]);
  const module = boot.moduleExport.replace(/^\.\//, "");
  return lazy(async () => {
    const loaded = await loadRemote<{ default: ComponentType<AppProps> }>(`${key}/${module}`);
    if (!loaded?.default) throw new Error("missing_AppRoot");
    return loaded;
  });
}

// The bridge updates route and display state without replacing the frame's
// API clients. These clients read the current token when making each request.
const frameSession = {
  activity,
  installationId: boot.installationId,
  appId: boot.appId,
  releaseId: boot.releaseId,
  basePath: boot.basePath,
  navigate: (path: string, options?: { replace?: boolean }) => {
    if (!relativePath(path)) throw new Error("Use an app-relative path");
    post({ type: "route.navigate", payload: { path, replace: !!options?.replace } });
  },
  // The full portal address of an app path, for links people share.
  portalUrl: (path: string) => new URL(appTarget(boot.basePath, path), boot.parentOrigin).href,
  api: <T,>(path: string, init?: RequestInit) => json<T>("/_miy/api" + apiPath(path), init),
  apiBlob: (path: string, init?: RequestInit) => blob("/_miy/api" + apiPath(path), init),
  fileApi: async (path: string, init?: FileInit) =>
    checkedResponse(await call("/_miy/files" + apiPath(path), init), init),
  download: (url) => startNativeDownload(downloadPath(url)),
  downloadAndWait,
  platformApi: async <T,>(resource: "directory", query?: DirectoryQuery, options?: DirectoryRequestOptions) => {
    if (resource !== "directory") throw new Error("unknown_platform_resource");
    return completeDirectoryPage(await json<T>("/_miy/platform/directory" + directoryQuery(query), options), query);
  },
  directory: (query, options) => json("/_miy/platform/directory" + directoryQuery(query), options),
  appApi: <T,>(target: string, path: string, init?: RequestInit) => {
    if (!/^[a-z][a-z0-9-]{0,47}$/.test(target)) throw new Error("invalid_api_path");
    return json<T>(`/_miy/connections/${target}/api${apiPath(path)}`, init);
  },
  live,
  liveBinary: (handlers: BinaryHandlers) => connectBinary(handlers, async () => {
    if (Date.now() > tokenExpiresAt - 5_000) await renew();
    const url = new URL("/_miy/live", window.location.href);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    return { url: url.href, auth: JSON.stringify({ type: "auth", token }) };
  }, () => !!stopped, (code) => {
    if (code === 4401) renewalError("frame_session_ended");
    if (code === 4403) renewalError("frame_access_denied");
    if (code === 4409) renewalError("release_changed_reload_required");
  }),
  setNavigationState,
  // Apps built with the 0.8 SDK still call this; see setNavigationState.
  registerNavigationGuard,
} satisfies Omit<AppRuntime, "location" | "theme" | "settings"> & {
  registerNavigationGuard: typeof registerNavigationGuard;
};

function FrameApp({ View }: { View: ComponentType<AppProps> }) {
  const [location, setLocation] = useState<FrameLocation>(currentLocation);
  const [settings, setSettings] = useState<FrameSettings>(currentSettings);
  const [systemDark, setSystemDark] = useState(DARK.matches);
  useEffect(() => {
    settingsListeners.add(setSettings);
    setSettings(currentSettings);
    const changed = () => setSystemDark(DARK.matches);
    DARK.addEventListener("change", changed);
    return () => {
      settingsListeners.delete(setSettings);
      DARK.removeEventListener("change", changed);
    };
  }, []);
  const display = useMemo(() => appSettings(settings, systemDark), [settings, systemDark]);
  useEffect(() => applyDocumentSettings(display), [display]);
  useEffect(() => {
    routeListeners.add(setLocation);
    setLocation(currentLocation);
    // Apps on the current SDK report their own state; otherwise start clean.
    const initial = setTimeout(() => {
      if (!reported && !legacy) post({ type: "guard.state", payload: { state: "clean" } });
    }, 0);
    return () => {
      clearTimeout(initial);
      routeListeners.delete(setLocation);
    };
  }, []);
  const runtime = useMemo<AppRuntime>(
    () => ({ ...frameSession, location, theme: display.theme, settings: display }),
    [location, display],
  );
  return <View runtime={runtime} />;
}

const root = createRoot(document.getElementById("miy-frame-root")!);
try {
  const View = remote();
  root.render(
    <Boundary>
      <Suspense fallback={<p className="miy-frame-status">{message("loading")}</p>}>
        <FrameApp View={View} />
      </Suspense>
    </Boundary>,
  );
} catch (error) {
  post({ type: "frame.error", payload: { code: error instanceof Error ? error.message.slice(0, 64) : "frame_failed" } });
  root.render(
    <p className="miy-frame-status" role="alert">
      {message("unsupported")}
    </p>,
  );
}
