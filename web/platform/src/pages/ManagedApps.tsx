import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { useManagedInstallations } from "../data/installations";
import { useSurface, useUser } from "../data/session";
import { currentEnvironment, environmentPath, surfaceUrl } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { RuntimeControls } from "../components/RuntimeControls";
import { RuntimeHealth } from "../components/RuntimeHealth";
import {
  AppMark,
  Badge,
  Button,
  EmptyState,
  Loading,
  Panel,
  monograms,
} from "../ui";

type AppPage = {
  apps: { id: string; name: string }[];
  nextCursor?: string | null;
};

async function readApps(path: string) {
  const value = await request<AppPage>(path);
  if (!Array.isArray(value.apps)) throw new Error("invalid_app_list");
  return value;
}

export function ManagedApps() {
  const user = useUser();
  const { portalOrigin } = useSurface();
  const managed = useManagedInstallations();
  const environment = currentEnvironment(useLocation().pathname);
  const [catalog, setCatalog] = useState<AppPage>();
  const [catalogError, setCatalogError] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const loadCatalog = useCallback(async () => {
    try {
      setCatalog(await readApps("/api/v1/apps?limit=200"));
      setCatalogError(false);
    } catch {
      setCatalogError(true);
    }
  }, []);
  useEffect(() => {
    void loadCatalog();
  }, [loadCatalog]);
  async function moreApps() {
    if (!catalog?.nextCursor || loadingMore) return;
    setLoadingMore(true);
    try {
      const next = await readApps(
        `/api/v1/apps?limit=200&cursor=${encodeURIComponent(catalog.nextCursor)}`,
      );
      setCatalog({ ...next, apps: [...catalog.apps, ...next.apps] });
      setCatalogError(false);
    } catch {
      setCatalogError(true);
    } finally {
      setLoadingMore(false);
    }
  }
  const apps = (managed.items ?? []).filter(
    (app) => app.environment === environment,
  );
  const marks = useMemo(
    () => monograms(apps.map((app) => ({ key: app.id, name: app.name }))),
    [apps],
  );
  const refresh = () => {
    void managed.refresh();
    void loadCatalog();
  };
  return (
    <>
      <div className="page-header">
        <div>
          <h1>내가 관리하는 앱</h1>
          <p>
            {environment === "prod" ? "운영" : "테스트"} 환경의 실행 상태를
            확인하고 허용된 작업을 수행합니다.
          </p>
        </div>
      </div>
      {managed.items === null ? (
        managed.status === "error" ? (
          <LoadError
            message="관리할 앱 목록을 불러오지 못했습니다."
            onRetry={refresh}
          />
        ) : (
          <Loading>앱 목록을 불러오는 중…</Loading>
        )
      ) : (
        <>
          {managed.status === "error" && (
            <StaleNotice updatedAt={managed.updatedAt} onRetry={refresh} />
          )}
          {apps.length === 0 ? (
            <EmptyState title="이 환경에서 관리할 설치가 없습니다.">
              앱·환경 권한이 필요하면 관리자에게 요청하세요.
            </EmptyState>
          ) : (
            <div className="grid">
              {apps.map((app) => (
                <Panel
                  key={app.id}
                  className="app-card"
                  title={
                    <span className="app-card-title">
                      <AppMark
                        mark={marks.get(app.id) ?? "?"}
                        colorKey={app.appId}
                      />
                      <span>{app.name}</span>
                    </span>
                  }
                  actions={
                    app.appDisabled && (
                      <Badge tone="warning">회사 사용 중지</Badge>
                    )
                  }
                >
                  <div className="app-card-meta">
                    <Badge>{app.runtimeStatus}</Badge>
                    <Badge>DB {app.dbStatus}</Badge>
                    <Badge>v{app.version || "—"}</Badge>
                    <code>{app.appId}</code>
                  </div>
                  <RuntimeHealth app={app} />
                  {app.canOpen ? (
                    <a
                      className="app-card-open"
                      href={surfaceUrl(
                        portalOrigin,
                        environmentPath(environment, app.appId),
                      )}
                    >
                      포털에서 앱 열기 →
                    </a>
                  ) : (
                    <p className="ui-empty-inline">
                      {app.appDisabled
                        ? "회사 사용 중지 중입니다. 허용된 관리 작업은 계속할 수 있습니다."
                        : "이 앱의 사용 권한이 없거나 현재 열 수 없습니다."}
                    </p>
                  )}
                  {app.kind === "docker_app" && app.canManage && (
                    <RuntimeControls
                      app={app}
                      csrf={user.csrf}
                      refresh={refresh}
                    />
                  )}
                </Panel>
              ))}
            </div>
          )}
        </>
      )}
      <h2 className="page-section-title">등록된 담당 앱</h2>
      <p>
        설치 전 앱을 포함한 담당 앱 목록입니다. 설치와 배포는 기존 CLI 흐름을
        사용하세요.
      </p>
      {catalogError && (
        <LoadError
          message="등록된 담당 앱을 불러오지 못했습니다."
          onRetry={() => void loadCatalog()}
        />
      )}
      {!catalog && !catalogError && <Loading />}
      {catalog &&
        (catalog.apps.length ? (
          <ul>
            {catalog.apps.map((app) => (
              <li key={app.id}>
                {app.name} · <code>{app.id}</code>
              </li>
            ))}
          </ul>
        ) : (
          <p className="ui-empty-inline">등록된 담당 앱이 없습니다.</p>
        ))}
      {catalog?.nextCursor && (
        <Button disabled={loadingMore} onClick={() => void moreApps()}>
          담당 앱 더 보기
        </Button>
      )}
    </>
  );
}
