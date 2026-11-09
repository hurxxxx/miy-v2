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
  type AppFrameToken,
  type AppProps,
  type AppRuntime,
  type FrameBootstrap,
  type NavigationState,
} from "@miy/sdk";
import {
  PROTOCOL,
  envelope,
  readParentMessage,
  relativePath,
  type FrameLocation,
  type FrameMessage,
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
let released = false;
let renewal: Promise<void> | null = null;
let pending: { resolve: (code: string) => void; reject: (error: Error) => void } | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

/** One renewal at a time: ask the portal for a code, exchange it on this origin. */
function renew(): Promise<void> {
  if (renewal) return renewal;
  if (Date.now() >= frameExpiresAt) return Promise.reject(new Error("frame_session_ended"));
  renewal = new Promise<string>((resolve, reject) => {
    pending = { resolve, reject };
    post({ type: "token.renew", payload: {} });
    setTimeout(() => reject(new Error("token_renewal_timeout")), 15_000);
  })
    .then(async (code) => {
      const response = await fetch("/_miy/renew", {
        method: "POST",
        mode: "same-origin",
        credentials: "omit",
        referrerPolicy: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code }),
      });
      if (!response.ok) throw new Error("token_renewal_refused");
      const value = (await response.json()) as AppFrameToken;
      token = value.token;
      tokenExpiresAt = Date.parse(value.tokenExpiresAt);
      schedule();
    })
    .finally(() => {
      renewal = null;
      pending = null;
    });
  return renewal;
}
function schedule() {
  clearTimeout(timer);
  if (tokenExpiresAt >= frameExpiresAt) return;
  const wait = Math.max(5_000, tokenExpiresAt - Date.now() - 60_000);
  timer = setTimeout(() => void renew().catch(() => undefined), wait);
}
schedule();

function apiPath(path: string) {
  if (!relativePath(path) || path.split("/").some((part) => part === ".." || part === "."))
    throw new Error("invalid_api_path");
  return path;
}

/** A request to this frame origin only: no cookies, the frame token, its release. */
async function call(path: string, init: RequestInit = {}): Promise<Response> {
  if (released) throw new Error("release_changed_reload_required");
  if (Date.now() > tokenExpiresAt - 5_000) await renew();
  const send = () => {
    const headers = new Headers(init.headers);
    if (typeof init.body === "string" && !headers.has("Content-Type"))
      headers.set("Content-Type", "application/json");
    headers.set("Authorization", `Bearer ${token}`);
    headers.set("x-miy-release", boot.releaseId);
    return fetch(path, {
      ...init,
      headers,
      mode: "same-origin",
      credentials: "omit",
      referrerPolicy: "same-origin",
    });
  };
  let response = await send();
  if (response.status === 401 && Date.now() < frameExpiresAt) {
    await renew();
    response = await send();
  }
  if (response.status === 409) {
    const data = await response
      .clone()
      .json()
      .catch(() => null);
    if (data?.error?.code === "release_changed_reload_required" && !released) {
      released = true;
      post({ type: "release.changed", payload: {} });
    }
  }
  if (response.status === 401) post({ type: "frame.error", payload: { code: "frame_session_ended" } });
  return response;
}
async function json<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await call(path, init);
  const data = await response.json().catch(() => null);
  if (!response.ok) throw new Error(data?.error?.code || `HTTP ${response.status}`);
  return data as T;
}
async function blob(path: string, init?: RequestInit): Promise<Blob> {
  const response = await call(path, init);
  if (!response.ok) {
    const data = await response.json().catch(() => null);
    throw new Error(data?.error?.code || `HTTP ${response.status}`);
  }
  return response.blob();
}

// The bridge listener: only the parent window at the expected origin, only
// validated messages. Route changes reach the app through a subscription.
const routeListeners = new Set<(location: FrameLocation) => void>();
let currentLocation: FrameLocation = boot.location;
window.addEventListener("message", (event) => {
  if (event.source !== window.parent || event.origin !== boot.parentOrigin) return;
  const message = readParentMessage(event.data);
  if (!message) return;
  if (message.type === "route.changed") {
    currentLocation = message.payload;
    for (const listener of routeListeners) listener(message.payload);
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
        앱 화면을 표시할 수 없습니다. 앱을 다시 열거나 관리자에게 문의하세요.
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

function FrameApp({ View }: { View: ComponentType<AppProps> }) {
  const [location, setLocation] = useState<FrameLocation>(currentLocation);
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
  const runtime = useMemo(
    () =>
      ({
        installationId: boot.installationId,
        appId: boot.appId,
        releaseId: boot.releaseId,
        basePath: boot.basePath,
        location,
        navigate: (path: string, options?: { replace?: boolean }) => {
          if (!relativePath(path)) throw new Error("Use an app-relative path");
          post({ type: "route.navigate", payload: { path, replace: !!options?.replace } });
        },
        api: <T,>(path: string, init?: RequestInit) => json<T>("/_miy/api" + apiPath(path), init),
        apiBlob: (path: string, init?: RequestInit) => blob("/_miy/api" + apiPath(path), init),
        platformApi: <T,>(resource: "directory") => {
          if (resource !== "directory") throw new Error("unknown_platform_resource");
          return json<T>("/_miy/platform/directory");
        },
        appApi: <T,>(target: string, path: string, init?: RequestInit) => {
          if (!/^[a-z][a-z0-9-]{0,47}$/.test(target)) throw new Error("invalid_api_path");
          return json<T>(`/_miy/connections/${target}/api${apiPath(path)}`, init);
        },
        setNavigationState,
        // Apps built with the 0.8 SDK still call this; see setNavigationState.
        registerNavigationGuard,
        theme: "light",
      }) satisfies AppRuntime & { registerNavigationGuard: typeof registerNavigationGuard },
    [location],
  );
  return <View runtime={runtime} />;
}

const root = createRoot(document.getElementById("miy-frame-root")!);
try {
  const View = remote();
  root.render(
    <Boundary>
      <Suspense fallback={<p className="miy-frame-status">앱 화면을 불러오는 중…</p>}>
        <FrameApp View={View} />
      </Suspense>
    </Boundary>,
  );
} catch (error) {
  post({ type: "frame.error", payload: { code: error instanceof Error ? error.message.slice(0, 64) : "frame_failed" } });
  root.render(
    <p className="miy-frame-status" role="alert">
      지원하지 않는 앱 화면입니다.
    </p>,
  );
}
