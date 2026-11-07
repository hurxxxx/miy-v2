import React, { Component, Suspense, useEffect, useMemo, useState } from "react";
import { Link, useLocation, useRouter } from "@tanstack/react-router";
import { appTarget, createNavigationGuards, request, type AppRuntime, type Installation } from "@miy/sdk";
import { Button, Loading, Notice, Panel } from "../ui";
import { useUser } from "../data/session";
import { currentEnvironment, environmentPath } from "../lib/paths";
import { activeGuards } from "../lib/navigation-guards";
import { remote } from "./remote";

class Boundary extends Component<{ children: React.ReactNode }, { error: boolean }> {
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

function AppAvailability({ health }: { health: Installation["health"] }) {
  if (health?.status !== "degraded" && health?.status !== "unhealthy") return null;
  return <Notice role="status" aria-label="앱 이용 안내" tone="warning">
    <p>{health.status === "unhealthy"
      ? "앱 응답이 원활하지 않습니다. 문제가 계속되면 앱 담당자에게 문의하세요."
      : "앱의 응답을 확인하고 있습니다. 잠시 후 다시 시도해 주세요."}</p>
  </Notice>;
}

export function AppView() {
  const location = useLocation();
  const parts = location.pathname.split("/");
  const environment = currentEnvironment(location.pathname);
  const appId = parts[1] === "environments" ? parts[4]! : parts[2]!;
  const basePath = parts[1] === "environments" ? environmentPath(environment, appId) : "/apps/" + appId;
  return (
    <AppSession key={`${environment}:${appId}`} appId={appId} environment={environment} basePath={basePath} />
  );
}

// A route change creates a new session; observations never replace its pinned release.
function AppSession({ appId, environment, basePath }: { appId: string; environment: string; basePath: string }) {
  const user = useUser();
  const router = useRouter();
  const location = useLocation();
  const [app, setApp] = useState<Installation | null>();
  const [loadFailed, setLoadFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [health, setHealth] = useState<Installation["health"]>();
  const [updated, setUpdated] = useState(false);
  const guards = useMemo(() => createNavigationGuards(), [app?.id, app?.releaseId]);
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
  }, [guards, router]);
  useEffect(() => {
    setApp(undefined);
    setLoadFailed(false);
    setUpdated(false);
    let active = true;
    const controller = new AbortController();
    request<Installation[]>("/api/v1/installations", undefined, { signal: controller.signal })
      .then((items) => {
        if (active) {
          const found = items.find((a) => a.appId === appId && a.environment === environment) ?? null;
          setApp(found);
          setHealth(found?.health);
        }
      })
      .catch(() => {
        if (active) setLoadFailed(true);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [appId, environment, attempt]);
  useEffect(() => {
    if (!app) return;
    const session = app;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const controller = new AbortController();
    async function poll() {
      try {
        const items = await request<Installation[]>("/api/v1/installations", undefined, {
          signal: controller.signal,
        });
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
              void router.navigate({ to: href as "/", replace: options?.replace });
            },
            api: async <T,>(path: string, init?: RequestInit) => {
              if (updated) throw new Error("release_changed_reload_required");
              if (app.runtimeStatus === "not_deployed") throw new Error("app_runtime_not_deployed");
              if (app.runtimeStatus !== "running") throw new Error("app_runtime_not_running");
              if (!path.startsWith("/") || path.startsWith("//") || path.includes(".."))
                throw new Error("invalid_api_path");
              return request<T>("/api/v1/installations/" + app.id + "/api" + path, user.csrf, {
                ...init,
                headers: { ...init?.headers, "x-miy-release": app.releaseId },
              });
            },
            platformApi: <T,>(resource: "directory") => {
              if (updated) throw new Error("release_changed_reload_required");
              return request<T>(`/api/v1/installations/${app.id}/platform/${resource}`, user.csrf);
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
              return request<T>(`/api/v1/installations/${app.id}/connections/${target}/api${path}`, user.csrf, {
                ...init,
                headers: { ...init?.headers, "x-miy-release": app.releaseId },
              });
            },
            registerNavigationGuard: guards.register,
            theme: "light",
          }
        : null,
    [app, basePath, guards, location.pathname, location.searchStr, location.hash, updated, user.csrf, router],
  );
  if (loadFailed)
    return (
      <Panel title="앱 정보를 불러오지 못했습니다">
        <Notice
          role="alert"
          tone="danger"
          actions={
            <Button size="sm" onClick={() => setAttempt((value) => value + 1)}>
              다시 시도
            </Button>
          }
        >
          <p>연결 상태를 확인한 뒤 다시 시도하세요. 앱 접근 권한은 바뀌지 않았습니다.</p>
        </Notice>
      </Panel>
    );
  if (app === undefined) return <Loading>앱을 불러오는 중…</Loading>;
  if (!app || !runtime)
    return (
      <Panel title="앱에 접근할 수 없습니다">
        <p>앱이 없거나 접근 권한이 없습니다.</p>
      </Panel>
    );
  if (app.kind === "docker_app" && app.runtimeStatus !== "running")
    return (
      <Panel title="앱이 실행 중이 아닙니다">
        <p>지금은 앱을 사용할 수 없습니다. 앱 담당자에게 문의하세요.</p>
        <Link to={environmentPath(environment) as "/"}>홈으로</Link>
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
      <div className="app-session-bar">
        {updated && (
          <Notice
            tone="warning"
            actions={
              <Button size="sm" variant="primary" onClick={() => window.location.reload()}>
                새로고침
              </Button>
            }
          >
            <p>
              <strong>앱 상태가 변경되었습니다</strong>
            </p>
            <p>작성 중인 내용을 확인한 뒤 최신 상태로 다시 열어주세요. 현재 화면의 API 호출은 중지됩니다.</p>
          </Notice>
        )}
        <AppAvailability health={health ?? app.health} />
      </div>
      {/* Platform fallbacks stay outside the container excluded from shell CSS. */}
      <Boundary key={app.releaseId}>
        <Suspense fallback={<p>앱 화면을 불러오는 중…</p>}>
          <div className="miy-app-root" data-miy-app={app.appId}>
            <View key={app.id} runtime={runtime} />
          </div>
        </Suspense>
      </Boundary>
    </>
  );
}
