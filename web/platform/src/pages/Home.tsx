import { Link, useLocation } from "@tanstack/react-router";
import { Panel } from "@miy/ui";
import { installationsIn, useInstallationPolling, useInstallations } from "../data/installations";
import { useUser } from "../data/session";
import { currentEnvironment, environmentPath } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { RuntimeHealth } from "../components/RuntimeHealth";
import { RuntimeControls } from "../components/RuntimeControls";

export function Home() {
  const user = useUser();
  const installations = useInstallations();
  useInstallationPolling();
  const location = useLocation();
  const environment = currentEnvironment(location.pathname);
  const refresh = () => void installations.refresh();
  const apps = installationsIn(installations.items, environment);
  return (
    <>
      <h1>{environment === "prod" ? "운영 앱" : "테스트 앱"}</h1>
      <p>접근할 수 있는 업무 앱을 선택하세요.</p>
      {installations.items === null ? (
        installations.status === "error" ? (
          <LoadError message="앱 목록을 불러오지 못했습니다. 연결 상태를 확인해 주세요." onRetry={refresh} />
        ) : (
          <p className="load-pending">앱 목록을 불러오는 중…</p>
        )
      ) : (
        <>
          {installations.status === "error" && (
            <StaleNotice updatedAt={installations.updatedAt} onRetry={refresh} />
          )}
          {apps.length === 0 ? (
            <p className="empty-state">
              이 실행 환경에서 사용할 수 있는 앱이 없습니다. 접근 권한이 필요하면 관리자에게 요청하세요.
            </p>
          ) : (
            <div className="grid">
              {apps.map((a) => (
                <Panel key={a.id} title={a.name}>
                  <p>데이터베이스: {a.dbStatus}</p>
                  <p>
                    실행 환경: {a.environment} · 버전: {a.version || "—"}
                  </p>
                  <RuntimeHealth app={a} />
                  {(a.kind !== "docker_app" || a.runtimeStatus === "running") && (
                    <Link to={environmentPath(environment, a.appId) as "/"}>앱 열기 →</Link>
                  )}
                  {a.kind === "docker_app" && a.canManage && (
                    <RuntimeControls app={a} csrf={user.csrf} refresh={refresh} />
                  )}
                </Panel>
              ))}
            </div>
          )}
        </>
      )}
    </>
  );
}
