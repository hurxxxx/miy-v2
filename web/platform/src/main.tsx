import React, {
  Component,
  lazy,
  Suspense,
  useEffect,
  useMemo,
  useState,
} from "react";
import { createRoot } from "react-dom/client";
import {
  createRouter,
  createRootRoute,
  createRoute,
  RouterProvider,
  Outlet,
  useLocation,
  Link,
} from "@tanstack/react-router";
import {
  registerRemotes,
  loadRemote,
} from "@module-federation/enhanced/runtime";
import {
  appTarget,
  request,
  UI_CONTRACT,
  type AppRuntime,
  type AppProps,
  type Installation,
} from "@miy/sdk";
import { Button, Panel } from "@miy/ui";
import "./style.css";

type User = { id: string; name: string; admin: boolean; csrf: string };
type LoginOptions = {
  developmentLogin: boolean;
  accounts: { account: string; label: string; description: string }[];
  oauthUrl: string;
};
function Login() {
  const [options, setOptions] = useState<LoginOptions>();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState("");
  useEffect(() => {
    request<LoginOptions>("/auth/options")
      .then(setOptions)
      .catch(() => setError("로그인 정보를 불러올 수 없습니다."));
  }, []);
  async function enter(account: string) {
    setBusy(account);
    setError("");
    try {
      const result = await request<{ redirect: string }>(
        "/auth/development",
        undefined,
        { method: "POST", body: JSON.stringify({ account }) },
      );
      window.location.assign(result.redirect);
    } catch {
      setError("로그인하지 못했습니다. 다시 시도해 주세요.");
      setBusy(undefined);
    }
  }
  return (
    <main className="login">
      <h1>miy</h1>
      <p>
        {options?.developmentLogin
          ? "개발 계정을 선택하면 바로 시작합니다."
          : "업무 앱과 데이터를 한곳에서"}
      </p>
      {options?.developmentLogin && (
        <div className="quick-accounts">
          {options.accounts.map((account, index) => (
            <button
              type="button"
              key={account.account}
              className={`account-button ${index === 0 ? "primary" : ""}`}
              disabled={!!busy}
              onClick={() => enter(account.account)}
            >
              <strong>
                {busy === account.account ? "접속 중…" : account.label}
              </strong>
              <span>{account.description}</span>
            </button>
          ))}
        </div>
      )}
      {error && <p role="alert">{error}</p>}
      <a
        className={options?.developmentLogin ? "oauth-link" : ""}
        href={options?.oauthUrl || "/auth/login"}
      >
        {options?.developmentLogin ? "OAuth로 로그인" : "로그인"}
      </a>
    </main>
  );
}
function useSession() {
  const [user, setUser] = useState<User | null | undefined>(undefined);
  useEffect(() => {
    request<User>("/api/v1/me")
      .then(setUser)
      .catch(() => setUser(null));
  }, []);
  return user;
}
function Shell() {
  const user = useSession();
  const [apps, setApps] = useState<Installation[]>([]);
  const location = useLocation();
  useEffect(() => {
    if (user)
      request<Installation[]>("/api/v1/installations")
        .then(setApps)
        .catch(() => setApps([]));
  }, [user, location.pathname]);
  if (user === undefined) return <main>불러오는 중…</main>;
  if (!user) return <Login />;
  async function logout() {
    const data = await request<{ redirect: string }>(
      "/auth/logout",
      user!.csrf,
      { method: "POST" },
    );
    window.location.assign(data.redirect);
  }
  return (
    <div className="shell">
      <aside>
        <Link to="/" className="brand">
          miy
        </Link>
        <p className="label">WORKSPACE · DEV</p>
        <nav>
          <Link to="/">홈</Link>
          {apps
            .filter((a) => a.environment === "dev")
            .map((a) => (
              <Link key={a.id} to={("/apps/" + a.appId) as "/"}>
                {a.name}
              </Link>
            ))}
        </nav>
      </aside>
      <div className="workspace">
        <header>
          <span>업무 공간</span>
          <span>
            {user.name} <Button onClick={logout}>로그아웃</Button>
          </span>
        </header>
        <main>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
function Home() {
  const [apps, setApps] = useState<Installation[]>([]);
  const user = useSession();
  function refresh() {
    void request<Installation[]>("/api/v1/installations").then(setApps);
  }
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, []);
  return (
    <>
      <h1>앱</h1>
      <p>접근할 수 있는 업무 앱을 선택하세요.</p>
      <div className="grid">
        {apps
          .filter((a) => a.environment === "dev")
          .map((a) => (
            <Panel key={a.id} title={a.name}>
              <p>데이터베이스: {a.dbStatus}</p>
              <p>실행 환경: {a.environment}</p>
              {a.kind === "docker_app" && <p>Docker: {a.runtimeStatus}</p>}
              {(a.kind !== "docker_app" || a.runtimeStatus === "running") && (
                <Link to={("/apps/" + a.appId) as "/"}>앱 열기 →</Link>
              )}
              {a.kind === "docker_app" && a.canManage && user && (
                <RuntimeControls app={a} csrf={user.csrf} refresh={refresh} />
              )}
            </Panel>
          ))}
      </div>
    </>
  );
}
function RuntimeControls({
  app,
  csrf,
  refresh,
}: {
  app: Installation;
  csrf: string;
  refresh: () => void;
}) {
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  async function control(action: string) {
    setBusy(action);
    setError("");
    try {
      const job = await request<{ id: string }>(
        `/api/v1/apps/${app.appId}/runtime`,
        csrf,
        {
          method: "POST",
          body: JSON.stringify({
            action,
            environment: app.environment,
            idempotencyKey: crypto.randomUUID(),
          }),
        },
      );
      for (let i = 0; i < 240; i++) {
        const state = await request<{
          status: string;
          step: string;
          errorCode?: string;
        }>(`/api/v1/runtime/jobs/${job.id}`);
        if (state.status === "failed")
          throw new Error(state.errorCode || "작업 실패");
        if (state.status === "succeeded") {
          refresh();
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      throw new Error(
        "작업이 지연되고 있습니다. 새로고침해 상태를 확인하세요.",
      );
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "작업을 완료하지 못했습니다.",
      );
    } finally {
      setBusy("");
      refresh();
    }
  }
  if (app.runtimeStatus === "removed")
    return (
      <p>
        앱 실행 환경이 제거됐습니다. DB는 보존됩니다. CLI에서 다시 배포할 수
        있습니다.
      </p>
    );
  return (
    <div className="runtime-controls">
      <p>{app.imageId?.slice(0, 22)}…</p>
      <div>
        {(app.runtimeStatus === "running"
          ? ["stop", "restart", "remove"]
          : ["start", "remove"]
        ).map((action) => (
          <Button
            key={action}
            disabled={!!busy}
            onClick={() => control(action)}
          >
            {action === "start"
              ? "시작"
              : action === "stop"
                ? "중지"
                : action === "restart"
                  ? "재시작"
                  : "앱 제거(DB 보존)"}
          </Button>
        ))}
      </div>
      {busy && <p role="status">Docker 작업을 처리하고 있습니다…</p>}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
class Boundary extends Component<
  { children: React.ReactNode },
  { error: boolean }
> {
  state = { error: false };
  static getDerivedStateFromError() {
    return { error: true };
  }
  render() {
    return this.state.error ? (
      <Panel title="앱을 열 수 없습니다">
        <p>앱을 다시 열거나 관리자에게 문의하세요.</p>
      </Panel>
    ) : (
      this.props.children
    );
  }
}
const modules = new Map<
  string,
  React.LazyExoticComponent<React.ComponentType<AppProps>>
>();
function remote(app: Installation) {
  if (app.ui.contractVersion !== UI_CONTRACT)
    throw new Error("unsupported_ui_contract");
  const entry = new URL(app.ui.entry, window.location.origin);
  if (
    entry.origin !== window.location.origin ||
    !entry.pathname.startsWith("/artifacts/")
  )
    throw new Error("untrusted_ui_origin");
  const key = "miy_" + app.releaseId.replaceAll("-", "");
  if (!modules.has(key)) {
    registerRemotes([{ name: key, entry: entry.href }]);
    modules.set(
      key,
      lazy(async () => {
        const module = await loadRemote<{
          default: React.ComponentType<AppProps>;
        }>(key + "/AppRoot");
        if (!module?.default) throw new Error("missing_AppRoot");
        return module;
      }),
    );
  }
  return modules.get(key)!;
}
function AppView() {
  const user = useSession();
  const location = useLocation();
  const appId = location.pathname.split("/")[2];
  const [app, setApp] = useState<Installation | null>();
  const [updated, setUpdated] = useState(false);
  useEffect(() => {
    setApp(undefined);
    setUpdated(false);
    let active = true;
    request<Installation[]>("/api/v1/installations")
      .then((items) => {
        if (active)
          setApp(
            items.find((a) => a.appId === appId && a.environment === "dev") ??
              null,
          );
      })
      .catch(() => {
        if (active) setApp(null);
      });
    return () => {
      active = false;
    };
  }, [appId]);
  useEffect(() => {
    if (!app) return;
    const id = setInterval(() => {
      request<Installation[]>("/api/v1/installations")
        .then((items) => {
          const current = items.find((a) => a.id === app.id);
          if (
            !current ||
            current.releaseId !== app.releaseId ||
            current.runtimeStatus !== app.runtimeStatus
          )
            setUpdated(true);
        })
        .catch(() => setUpdated(true));
    }, 3000);
    return () => clearInterval(id);
  }, [app]);
  const runtime = useMemo<AppRuntime | null>(
    () =>
      app
        ? {
            installationId: app.id,
            appId: app.appId,
            releaseId: app.releaseId,
            basePath: "/apps/" + app.appId,
            location: {
              pathname:
                location.pathname.slice(("/apps/" + app.appId).length) || "/",
              search: location.searchStr,
              hash: location.hash,
            },
            navigate: (to, options) => {
              const href = appTarget("/apps/" + app.appId, to);
              void router.navigate({
                to: href as "/",
                replace: options?.replace,
              });
            },
            api: async <T,>(path: string, init?: RequestInit) => {
              if (updated) throw new Error("release_changed_reload_required");
              if (app.runtimeStatus === "not_deployed")
                throw new Error("app_runtime_not_deployed");
              if (app.runtimeStatus !== "running")
                throw new Error("app_runtime_not_running");
              if (
                !path.startsWith("/") ||
                path.startsWith("//") ||
                path.includes("..")
              )
                throw new Error("invalid_api_path");
              return request<T>(
                "/api/v1/installations/" + app.id + "/api" + path,
                user?.csrf,
                {
                  ...init,
                  headers: { ...init?.headers, "x-miy-release": app.releaseId },
                },
              );
            },
            theme: "light",
          }
        : null,
    [
      app,
      location.pathname,
      location.searchStr,
      location.hash,
      updated,
      user?.csrf,
    ],
  );
  if (app === undefined) return <p>앱을 불러오는 중…</p>;
  if (!app || !runtime)
    return (
      <Panel title="앱에 접근할 수 없습니다">
        <p>앱이 없거나 접근 권한이 없습니다.</p>
      </Panel>
    );
  if (app.kind === "docker_app" && app.runtimeStatus !== "running")
    return (
      <Panel title="앱이 실행 중이 아닙니다">
        <p>
          현재 상태: {app.runtimeStatus}. 홈의 앱 관리에서 시작할 수 있습니다.
        </p>
        <Link to="/">홈으로</Link>
      </Panel>
    );
  let View: ReturnType<typeof remote>;
  try {
    View = remote(app);
  } catch {
    return (
      <Panel title="지원하지 않는 앱 화면">
        <p>앱 계약을 확인하세요.</p>
      </Panel>
    );
  }
  if (updated)
    return (
      <Panel title="앱 상태가 변경되었습니다">
        <p>최신 상태로 다시 열어주세요.</p>
        <Button onClick={() => window.location.reload()}>새로고침</Button>
      </Panel>
    );
  return (
    <Boundary key={app.releaseId}>
      <Suspense fallback={<p>앱 화면을 불러오는 중…</p>}>
        <View key={app.id} runtime={runtime} />
      </Suspense>
    </Boundary>
  );
}
const root = createRootRoute({ component: Shell });
const home = createRoute({
  getParentRoute: () => root,
  path: "/",
  component: Home,
});
const app = createRoute({
  getParentRoute: () => root,
  path: "/apps/$appId/$",
  component: AppView,
});
const appIndex = createRoute({
  getParentRoute: () => root,
  path: "/apps/$appId",
  component: AppView,
});
const router = createRouter({
  routeTree: root.addChildren([home, app, appIndex]),
});
createRoot(document.getElementById("root")!).render(
  <RouterProvider router={router} />,
);
