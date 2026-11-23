import { createBrowserHistory } from "@tanstack/history";
import React, { useState, useEffect, useMemo } from "react";
import { createRoot } from "react-dom/client";
import "./preview.css";
import AppRoot from "../../web/src/AppRoot";
import {
  appTarget,
  request,
  requestBlob,
  requestResponse,
  completeDirectoryPage,
  directoryQuery,
  createNavigationGuards,
  type AppRuntime,
  type AppSettings,
  type LiveConnection,
  type LiveHandlers,
  type NavigationState,
} from "../browser";
// Preview has no portal settings: it follows this browser. A deployed app
// receives the person's portal choice the same way, through runtime.settings.
const DARK = window.matchMedia("(prefers-color-scheme: dark)");

/**
 * The preview counterpart of the frame runtime's live connection: the local
 * preview server relays `/__miy/live` to this backend's `/api/live` with the
 * developer's identity and sends the same `{"type":"miy.ready"}` first.
 */
function previewLive(handlers: LiveHandlers): LiveConnection {
  let socket: WebSocket | null = null;
  let closed = false;
  let attempt = 0;
  let retry: ReturnType<typeof setTimeout> | undefined;
  const open = () => {
    if (closed) return;
    const url = new URL("/__miy/live", window.location.href);
    url.protocol = "ws:";
    const current = new WebSocket(url.href);
    socket = current;
    let ready = false;
    current.onmessage = (event) => {
      if (typeof event.data !== "string") return;
      if (!ready) {
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
      handlers.onClose?.({ code: event.code, retrying: !closed });
      if (closed) return;
      attempt += 1;
      retry = setTimeout(open, Math.min(30_000, 1_000 * 2 ** Math.min(attempt, 5)));
    };
  };
  open();
  return {
    send(data) {
      if (!socket || socket.readyState !== WebSocket.OPEN) throw new Error("live_not_open");
      socket.send(data);
    },
    close() {
      closed = true;
      clearTimeout(retry);
      socket?.close(1000);
    },
  };
}

function browserSettings(): AppSettings {
  const theme = DARK.matches ? "dark" : "light";
  return {
    theme,
    themePreference: "system",
    locale: "ko-KR",
    timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Seoul",
    dateFormat: "korean",
  };
}
function applyDocumentSettings(settings: AppSettings) {
  const root = document.documentElement;
  root.classList.toggle("dark", settings.theme === "dark");
  root.dataset.miyTheme = settings.theme;
  root.lang = settings.locale;
}
const history = createBrowserHistory();
// The platform reads the app's reported input state; preview does the same.
let navigationState: NavigationState = { dirty: false };
const guards = createNavigationGuards();
guards.register(() =>
  navigationState.dirty
    ? navigationState.message || "저장하지 않은 내용이 있습니다. 이동하시겠습니까?"
    : undefined,
);
// As in the portal: the app's own runtime.navigate does not ask (it knows its
// input state); browser history still asks.
let appNavigation = false;
history.block({
  blockerFn: () => !appNavigation && !guards.confirmNavigation(),
  enableBeforeUnload: () => !!guards.message(),
});
type PreviewContext = {
  csrf: string;
  appId: string;
  installationId: string;
  releaseId: string;
  user: { name: string };
};

function previewSession(context: PreviewContext) {
  const basePath = "/apps/" + context.appId;
  return {
    appId: context.appId,
    installationId: context.installationId,
    releaseId: context.releaseId,
    basePath,
    navigate: (path, options) => {
      const to = appTarget(basePath, path);
      appNavigation = true;
      try {
        if (options?.replace) history.replace(to);
        else history.push(to);
      } finally {
        // Blockers run while the history call is in progress.
        setTimeout(() => {
          appNavigation = false;
        }, 0);
      }
    },
    api: (path, init) => request("/__miy/api" + safe(path), context.csrf, init),
    apiBlob: (path, init) => requestBlob("/__miy/api" + safe(path), context.csrf, init),
    fileApi: (path, init) => requestResponse("/__miy/files" + safe(path), context.csrf, init),
    download: () => {
      throw new Error("download_links_unavailable_in_preview");
    },
    portalUrl: (path) => new URL(appTarget(basePath, path), window.location.origin).href,
    platformApi: async (resource, query, options) => {
      if (resource !== "directory") throw new Error("unknown_platform_resource");
      return completeDirectoryPage(
        await request("/__miy/platform/directory" + directoryQuery(query), context.csrf, options),
        query,
      );
    },
    directory: (query, options) =>
      request("/__miy/platform/directory" + directoryQuery(query), context.csrf, options),
    appApi: (app, path, init) =>
      request(
        "/__miy/connections/" + app + "/api" + safe(path),
        context.csrf,
        init,
      ),
    live: previewLive,
    setNavigationState: (state) => {
      navigationState = state;
    },
  } satisfies Omit<AppRuntime, "location" | "theme" | "settings">;
}

function Preview() {
  const [context, setContext] = useState<PreviewContext>();
  const [error, setError] = useState("");
  const [location, setLocation] = useState(history.location.href);
  const [settings, setSettings] = useState(browserSettings);
  useEffect(() => {
    const changed = () => setSettings(browserSettings());
    DARK.addEventListener("change", changed);
    return () => DARK.removeEventListener("change", changed);
  }, []);
  useEffect(() => applyDocumentSettings(settings), [settings]);
  useEffect(() => {
    void request<typeof context>("/__miy/context")
      .then(setContext)
      .catch((e) => setError(String(e)));
    const update = () => setLocation(history.location.href);
    return history.subscribe(update);
  }, []);
  const session = useMemo(() => context && previewSession(context), [context]);
  if (error) return <p role="alert">{error}</p>;
  if (!context || !session) return <p>Connecting to local preview…</p>;
  const uri = new URL(location, window.location.origin);
  const runtime: AppRuntime = {
    ...session,
    location: {
      pathname: uri.pathname.slice(session.basePath.length) || "/",
      search: uri.search,
      hash: uri.hash,
    },
    theme: settings.theme,
    settings,
  };
  return (
    <>
      <header className="miy-preview-header">
        miy · LOCAL PREVIEW · {context.user.name} · development database /
        server DEV connections
      </header>
      <main className="miy-preview-main">
        <AppRoot runtime={runtime} />
      </main>
    </>
  );
}
function safe(path: string) {
  if (
    !path.startsWith("/") ||
    path.startsWith("//") ||
    path.includes("..") ||
    path.includes("\\")
  )
    throw Error("invalid_api_path");
  return path;
}
createRoot(document.getElementById("root")!).render(<Preview />);
