import { createBrowserHistory } from "@tanstack/history";
import React, { useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import AppRoot from "../../web/src/AppRoot";
import {
  appTarget,
  request,
  createNavigationGuards,
  type AppRuntime,
} from "../browser";
const history = createBrowserHistory();
const guards = createNavigationGuards();
history.block({
  blockerFn: () => !guards.confirmNavigation(),
  enableBeforeUnload: () => !!guards.message(),
});
function Preview() {
  const [context, setContext] = useState<{
    csrf: string;
    appId: string;
    installationId: string;
    releaseId: string;
    user: { name: string };
  }>();
  const [error, setError] = useState("");
  const [location, setLocation] = useState(history.location.href);
  useEffect(() => {
    void request<typeof context>("/__miy/context")
      .then(setContext)
      .catch((e) => setError(String(e)));
    const update = () => setLocation(history.location.href);
    return history.subscribe(update);
  }, []);
  if (error) return <p role="alert">{error}</p>;
  if (!context) return <p>Connecting to local preview…</p>;
  const uri = new URL(location, window.location.origin);
  const basePath = "/apps/" + context.appId;
  const runtime: AppRuntime = {
    appId: context.appId,
    installationId: context.installationId,
    releaseId: context.releaseId,
    basePath,
    location: {
      pathname: uri.pathname.slice(basePath.length) || "/",
      search: uri.search,
      hash: uri.hash,
    },
    navigate: (path, options) => {
      const to = appTarget(basePath, path);
      if (options?.replace) history.replace(to);
      else history.push(to);
    },
    api: (path, init) => request("/__miy/api" + safe(path), context.csrf, init),
    platformApi: (resource) =>
      request("/__miy/platform/" + resource, context.csrf),
    appApi: (app, path, init) =>
      request(
        "/__miy/connections/" + app + "/api" + safe(path),
        context.csrf,
        init,
      ),
    registerNavigationGuard: guards.register,
    theme: "light",
  };
  return (
    <>
      <header style={{ padding: 16, background: "#edf2f7" }}>
        miy · LOCAL PREVIEW · {context.user.name} · development database /
        server DEV connections
      </header>
      <main style={{ padding: 24 }}>
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
