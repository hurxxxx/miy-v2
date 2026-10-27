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
  createNavigationGuards,
  appTarget,
  request,
  UI_CONTRACT,
  type AppRuntime,
  type AppProps,
  type Installation,
} from "@miy/sdk";
import { Button, Panel } from "@miy/ui";
import "./style.css";
import { RuntimeHealth } from "./RuntimeHealth";

const activeGuards = new Set<ReturnType<typeof createNavigationGuards>>();

type User = { id: string; name: string; admin: boolean; csrf: string };
type LoginOptions = {
  developmentLogin: boolean;
  developmentOrigin: string;
  accounts: { account: string; label: string; description: string }[];
  oauthUrl: string;
};
function environmentPath(environment: string, app?: string) {
  return `/environments/${environment}` + (app ? `/apps/${app}` : "");
}
function currentEnvironment(path: string) {
  const parts = path.split("/");
  if (parts[1] !== "environments") return "dev";
  return parts[2] === "dev" || parts[2] === "prod" ? parts[2] : "invalid";
}
function Login() {
  const [options, setOptions] = useState<LoginOptions>();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState("");
  useEffect(() => {
    request<LoginOptions>("/auth/options")
      .then((options) =>
        setOptions(
          options.developmentLogin &&
            options.developmentOrigin !== window.location.origin
            ? { ...options, developmentLogin: false, accounts: [] }
            : options,
        ),
      )
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
      window.location.assign(
        (window.location.pathname.startsWith("/cli/authorize/") || window.location.pathname.startsWith("/cli/device/"))
          ? window.location.href
          : result.redirect,
      );
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
        onClick={() => {
          if ((window.location.pathname.startsWith("/cli/authorize/") || window.location.pathname.startsWith("/cli/device/")))
            sessionStorage.setItem("miy.returnTo", window.location.pathname);
        }}
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
  const environment = currentEnvironment(location.pathname);
  useEffect(() => {
    if (user && location.pathname === "/") {
      const target = sessionStorage.getItem("miy.returnTo");
      if ((target?.startsWith("/cli/authorize/") || target?.startsWith("/cli/device/"))) {
        sessionStorage.removeItem("miy.returnTo");
        window.location.assign(target);
      }
    }
  }, [user, location.pathname]);
  useEffect(() => {
    if (user)
      request<Installation[]>("/api/v1/installations")
        .then(setApps)
        .catch(() => setApps([]));
  }, [user, location.pathname]);
  if (user === undefined) return <main>불러오는 중…</main>;
  if (!user) return <Login />;
  if (environment === "invalid")
    return (
      <main>
        <Panel title="존재하지 않는 실행 환경입니다">
          <p>테스트 또는 운영 영역의 주소를 사용하세요.</p>
          <Link to={environmentPath("dev") as "/"}>테스트 영역으로</Link>
        </Panel>
      </main>
    );
  async function logout() {
    if ([...activeGuards].some((guards) => !guards.confirmNavigation())) return;
    const data = await request<{ redirect: string }>(
      "/auth/logout",
      user!.csrf,
      { method: "POST" },
    );
    for (const guards of activeGuards) guards.clear();
    window.location.assign(data.redirect);
  }
  return (
    <div className="shell">
      <aside>
        <Link to="/" className="brand">
          miy
        </Link>
        <p className="label">
          {environment === "prod" ? "운영 · PROD" : "테스트 · DEV"}
        </p>
        <nav aria-label="실행 환경">
          <Link to={environmentPath("dev") as "/"}>테스트 영역</Link>
          <Link to={environmentPath("prod") as "/"}>운영 영역</Link>
          {user.admin && <Link to={"/admin" as "/"}>플랫폼 관리</Link>}
        </nav>
        <nav>
          <Link to={environmentPath(environment) as "/"}>홈</Link>
          {apps
            .filter((a) => a.environment === environment)
            .map((a) => (
              <Link
                key={a.id}
                to={environmentPath(environment, a.appId) as "/"}
              >
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
  const location = useLocation();
  const environment = currentEnvironment(location.pathname);
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
      <h1>{environment === "prod" ? "운영 앱" : "테스트 앱"}</h1>
      <p>접근할 수 있는 업무 앱을 선택하세요.</p>
      <div className="grid">
        {apps
          .filter((a) => a.environment === environment)
          .map((a) => (
            <Panel key={a.id} title={a.name}>
              <p>데이터베이스: {a.dbStatus}</p>
              <p>
                실행 환경: {a.environment} · 버전: {a.version || "—"}
              </p>
              <RuntimeHealth app={a} />
              {(a.kind !== "docker_app" || a.runtimeStatus === "running") && (
                <Link to={environmentPath(environment, a.appId) as "/"}>
                  앱 열기 →
                </Link>
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
      const plan = await request<{canSubmit: boolean; expectedState: unknown; changes: unknown}>(
        `/api/v1/apps/${app.appId}/runtime/preflight`, csrf,
        {method: "POST", body: JSON.stringify({target: {appId: app.appId, environment: app.environment}, action})},
      );
      if (!plan.canSubmit) throw new Error("현재 상태에서 실행할 수 없습니다. 새로고침 후 확인하세요.");
      if ((app.environment === "prod" || action === "remove") &&
          !window.confirm(`${app.name} (${app.appId}) / ${app.environment}: ${action} 실행
실행 중인 앱에 영향을 줍니다. 데이터베이스와 업무 데이터는 보존됩니다. 계속할까요?`)) return;
      const job = await request<{ id: string }>(
        `/api/v1/apps/${app.appId}/runtime`,
        csrf,
        {
          method: "POST",
          body: JSON.stringify({
            action,
            environment: app.environment,
            expectedState: plan.expectedState,
            confirmed: app.environment === "prod" || action === "remove",
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
  const location = useLocation();
  const parts = location.pathname.split("/");
  const environment = currentEnvironment(location.pathname);
  const appId = parts[1] === "environments" ? parts[4] : parts[2];
  const basePath =
    parts[1] === "environments"
      ? environmentPath(environment, appId)
      : "/apps/" + appId;
  return (
    <AppSession
      key={`${environment}:${appId}`}
      appId={appId}
      environment={environment}
      basePath={basePath}
    />
  );
}
// A route change creates a new session; observations never replace its pinned release.
function AppSession({
  appId,
  environment,
  basePath,
}: {
  appId: string;
  environment: string;
  basePath: string;
}) {
  const user = useSession();
  const location = useLocation();
  const [app, setApp] = useState<Installation | null>();
  const [health, setHealth] = useState<Installation["health"]>();
  const [updated, setUpdated] = useState(false);
  const guards = useMemo(
    () => createNavigationGuards(),
    [app?.id, app?.releaseId],
  );
  useEffect(() => {
    activeGuards.add(guards);
    const unblock = router.history.block({
      blockerFn: () => !guards.confirmNavigation(),
      enableBeforeUnload: () => !!guards.message(),
    });
    return () => {
      unblock();
      activeGuards.delete(guards);
    };
  }, [guards]);
  useEffect(() => {
    setApp(undefined);
    setUpdated(false);
    let active = true;
    const controller = new AbortController();
    request<Installation[]>("/api/v1/installations", undefined, {
      signal: controller.signal,
    })
      .then((items) => {
        if (active) {
          const found =
            items.find(
              (a) => a.appId === appId && a.environment === environment,
            ) ?? null;
          setApp(found);
          setHealth(found?.health);
        }
      })
      .catch(() => {
        if (active) setApp(null);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [appId, environment]);
  useEffect(() => {
    if (!app) return;
    const session = app;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const controller = new AbortController();
    async function poll() {
      try {
        const items = await request<Installation[]>(
          "/api/v1/installations",
          undefined,
          { signal: controller.signal },
        );
        if (!active) return;
        const current = items.find((a) => a.id === session.id);
        if (
          !current ||
          current.releaseId !== session.releaseId ||
          current.runtimeStatus !== session.runtimeStatus
        )
          setUpdated(true);
        else setHealth(current.health);
      } catch {
        if (active) setUpdated(true);
      } finally {
        if (active) timer = setTimeout(poll, 3000);
      }
    }
    timer = setTimeout(poll, 3000);
    return () => {
      active = false;
      clearTimeout(timer);
      controller.abort();
    };
  }, [app]);
  const runtime = useMemo<AppRuntime | null>(
    () =>
      app
        ? {
            installationId: app.id,
            appId: app.appId,
            releaseId: app.releaseId,
            basePath,
            location: {
              pathname: location.pathname.slice(basePath.length) || "/",
              search: location.searchStr,
              hash: location.hash,
            },
            navigate: (to, options) => {
              const href = appTarget(basePath, to);
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
            platformApi: <T,>(resource: "directory") => {
              if (updated) throw new Error("release_changed_reload_required");
              return request<T>(
                `/api/v1/installations/${app.id}/platform/${resource}`,
                user?.csrf,
              );
            },
            appApi: <T,>(target: string, path: string, init?: RequestInit) => {
              if (updated) throw new Error("release_changed_reload_required");
              if (
                !/^[a-z][a-z0-9-]{0,47}$/.test(target) ||
                !path.startsWith("/") ||
                path.startsWith("//") ||
                path.includes("..") ||
                path.includes("\\")
              )
                throw new Error("invalid_api_path");
              return request<T>(
                `/api/v1/installations/${app.id}/connections/${target}/api${path}`,
                user?.csrf,
                {
                  ...init,
                  headers: { ...init?.headers, "x-miy-release": app.releaseId },
                },
              );
            },
            registerNavigationGuard: guards.register,
            theme: "light",
          }
        : null,
    [
      app,
      basePath,
      guards,
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
  return (
    <>
      {updated && (
        <Panel title="앱 상태가 변경되었습니다">
          <p>
            작성 중인 내용을 확인한 뒤 최신 상태로 다시 열어주세요. 현재 화면의
            API 호출은 중지됩니다.
          </p>
          <Button onClick={() => window.location.reload()}>새로고침</Button>
        </Panel>
      )}
      <RuntimeHealth app={{ ...app, health }} />
      <Boundary key={app.releaseId}>
        <Suspense fallback={<p>앱 화면을 불러오는 중…</p>}>
          <View key={app.id} runtime={runtime} />
        </Suspense>
      </Boundary>
    </>
  );
}
function CliAuthorize() {
  const user = useSession();
  const location = useLocation();
  const id = location.pathname.split("/").at(-1);
  const [result, setResult] = useState("");
  const [busy, setBusy] = useState(false);
  async function approve() {
    if (!user) return;
    setBusy(true);
    try {
      await request(`/api/v1/cli-authorizations/${id}/approve`, user.csrf, {
        method: "POST",
      });
      setResult("CLI 연결을 승인했습니다. 터미널로 돌아가세요.");
    } catch (e) {
      setResult(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Panel title="CLI 로그인 승인">
      <p>
        {user?.name} 계정으로 CLI가 플랫폼 API를 사용하도록 연결합니다. 권한은
        이 계정의 앱·환경 권한을 따릅니다.
      </p>
      <p>
        요청 ID: <code>{id}</code>
      </p>
      <p>
        터미널의 요청 ID와 같은지 확인하세요. 연결은 8시간 뒤 만료되며 app
        logout으로 해제할 수 있습니다.
      </p>
      {result ? (
        <p role="status">{result}</p>
      ) : (
        <Button disabled={busy} onClick={approve}>
          이 CLI 연결 승인
        </Button>
      )}
    </Panel>
  );
}
function CliDeviceApprove() {
  const user = useSession();
  const location = useLocation();
  const id = location.pathname.split("/").at(-1);
  const [code, setCode] = useState("");
  const [result, setResult] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function decide(decision: "approve" | "deny") {
    if (!user || busy) return;
    setBusy(true); setError("");
    try {
      await request(`/api/v1/cli/devices/${id}/decision`, user.csrf, {
        method: "POST", body: JSON.stringify({ userCode: code, decision }),
      });
      setResult(decision === "approve" ? "CLI 연결을 승인했습니다. 터미널로 돌아가세요." : "CLI 연결을 거절했습니다.");
    } catch {
      setError("처리하지 못했습니다. 터미널의 코드와 요청 유효 시간을 확인해 주세요.");
    } finally { setBusy(false); }
  }
  return <Panel title="CLI 로그인 승인">
    <p>서버: <strong>{window.location.origin}</strong></p>
    <p><strong>{user?.name}</strong> 계정의 권한으로 CLI를 연결합니다.</p>
    <p>직접 시작한 요청인지 확인하고 터미널에 표시된 코드를 입력하세요. 요청은 5분 동안 유효합니다.</p>
    <p>요청 ID: <code>{id}</code></p>
    {result ? <p role="status">{result}</p> : <form onSubmit={event => { event.preventDefault(); void decide("approve"); }}>
      <label>인증 코드 <input name="userCode" value={code} onChange={event => setCode(event.target.value)} autoComplete="off" spellCheck={false} maxLength={32} required /></label>
      <Button type="submit" disabled={busy || !code.trim()}>이 계정으로 CLI 연결 승인</Button>
      <Button type="button" disabled={busy || !code.trim()} onClick={() => void decide("deny")}>거절</Button>
      {error && <p role="alert">{error}</p>}
      <p>연결 후에는 CLI의 로그인 세션 관리에서 철회할 수 있습니다.</p>
    </form>}
  </Panel>;
}
function Admin() {
  const user = useSession();
  const [requests, setRequests] = useState<
    {
      id: string;
      appId: string;
      name: string;
      status: string;
      requester: string;
    }[]
  >([]);
  const [users, setUsers] = useState<{ id: string; name: string }[]>([]);
  const [apps, setApps] = useState<Installation[]>([]);
  const [message, setMessage] = useState("");
  function refresh() {
    void request<typeof requests>("/api/v1/app-registrations").then(
      setRequests,
    );
    void request<typeof users>("/api/v1/users").then(setUsers);
    void request<Installation[]>("/api/v1/installations").then(setApps);
  }
  useEffect(() => {
    if (!user?.admin) return;
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [user?.admin]);
  if (!user?.admin)
    return (
      <Panel title="관리자 권한이 필요합니다">
        <p>관리자로 로그인하세요.</p>
      </Panel>
    );
  async function decide(id: string, approve: boolean) {
    try {
      await request(`/api/v1/app-registrations/${id}/decision`, user!.csrf, {
        method: "POST",
        body: JSON.stringify({ approve }),
      });
      setMessage(approve ? "앱 등록을 승인했습니다." : "등록을 거절했습니다.");
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  }
  async function grant(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    try {
      await request("/api/v1/grants", user!.csrf, {
        method: "PUT",
        body: JSON.stringify({
          userId: form.get("user"),
          appId: form.get("app"),
          environment: form.get("environment"),
          permission: form.get("permission"),
          enabled: form.get("enabled") === "true",
        }),
      });
      setMessage("권한을 변경했습니다.");
    } catch (e) {
      setMessage(String(e));
    }
  }
  return (
    <>
      <h1>플랫폼 관리</h1>
      <Panel title="앱 등록 신청">
        {requests
          .filter((r) => r.status === "pending")
          .map((r) => (
            <div key={r.id}>
              <p>
                {r.name} · {r.appId}
              </p>
              <Button onClick={() => decide(r.id, true)}>승인</Button>{" "}
              <Button onClick={() => decide(r.id, false)}>거절</Button>
            </div>
          ))}
      </Panel>
      <Panel title="앱·환경 권한">
        <form onSubmit={grant}>
          <label>
            사용자{" "}
            <select name="user">
              {users.map((u) => (
                <option key={u.id} value={u.id}>
                  {u.name}
                </option>
              ))}
            </select>
          </label>{" "}
          <label>
            앱 ID <input name="app" required pattern="[a-z][a-z0-9-]*" />
          </label>{" "}
          <label>
            환경{" "}
            <select name="environment">
              <option value="dev">테스트</option>
              <option value="prod">운영</option>
            </select>
          </label>{" "}
          <label>
            권한{" "}
            <select name="permission">
              <option value="use">사용</option>
              <option value="develop">개발 (운영 배포 불가)</option>
              <option value="deploy">배포 (운영 승인·제어)</option>
            </select>
          </label>{" "}
          <select aria-label="부여 또는 회수" name="enabled">
            <option value="true">부여</option>
            <option value="false">회수</option>
          </select>{" "}
          <Button type="submit">권한 적용</Button>
        </form>
      </Panel>
      {message && <p role="status">{message}</p>}
      <div className="grid">
        {apps
          .filter((a) => a.kind === "docker_app")
          .map((a) => (
            <Panel key={a.id} title={`${a.name} · ${a.environment}`}>
              <p>
                {a.version} · DB {a.dbStatus}
              </p>
              <Link to={environmentPath(a.environment, a.appId) as "/"}>
                앱 열기
              </Link>
              <RuntimeHealth app={a} />
              <RuntimeControls app={a} csrf={user.csrf} refresh={refresh} />
            </Panel>
          ))}
      </div>
    </>
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
  routeTree: root.addChildren([
    home,
    app,
    appIndex,
    createRoute({
      getParentRoute: () => root,
      path: "/environments/$environment",
      component: Home,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/environments/$environment/apps/$appId",
      component: AppView,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/environments/$environment/apps/$appId/$",
      component: AppView,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/cli/authorize/$id",
      component: CliAuthorize,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/cli/device/$id",
      component: CliDeviceApprove,
    }),
    createRoute({
      getParentRoute: () => root,
      path: "/admin",
      component: Admin,
    }),
  ]),
});
createRoot(document.getElementById("root")!).render(
  <RouterProvider router={router} />,
);
