import { useMemo } from "react";
import { Link, useLocation } from "@tanstack/react-router";
import type { Installation } from "@miy/sdk";
import { installationsIn, useInstallationPolling, useInstallations } from "../data/installations";
import { currentEnvironment, environmentPath } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { AppMark, EmptyState, Loading, Panel, monograms } from "../ui";

export function Home() {
  const installations = useInstallations();
  useInstallationPolling();
  const location = useLocation();
  const environment = currentEnvironment(location.pathname);
  const refresh = () => void installations.refresh();
  const apps = installationsIn(installations.items, environment);
  const marks = useMemo(() => monograms(apps.map((a) => ({ key: a.id, name: a.name }))), [apps]);
  return (
    <>
      <div className="page-header">
        <div>
          <h1>{environment === "prod" ? "운영 앱" : "테스트 앱"}</h1>
          <p>접근할 수 있는 업무 앱을 선택하세요.</p>
        </div>
      </div>
      {installations.items === null ? (
        installations.status === "error" ? (
          <LoadError message="앱 목록을 불러오지 못했습니다. 연결 상태를 확인해 주세요." onRetry={refresh} />
        ) : (
          <Loading>앱 목록을 불러오는 중…</Loading>
        )
      ) : (
        <>
          {installations.status === "error" && <StaleNotice updatedAt={installations.updatedAt} onRetry={refresh} />}
          {apps.length === 0 ? (
            <EmptyState title="이 실행 환경에서 사용할 수 있는 앱이 없습니다.">
              접근 권한이 필요하면 관리자에게 요청하세요.
            </EmptyState>
          ) : (
            <div className="grid">
              {apps.map((a) => (
                <AppCard
                  key={a.id}
                  app={a}
                  mark={marks.get(a.id) ?? "?"}
                  environment={environment}
                />
              ))}
            </div>
          )}
        </>
      )}
    </>
  );
}

function AppCard({
  app: a,
  mark,
  environment,
}: {
  app: Installation;
  mark: string;
  environment: string;
}) {
  const openable = a.kind !== "docker_app" || a.runtimeStatus === "running";
  return (
    <Panel
      className="app-card"
      title={
        <span className="app-card-title">
          <AppMark mark={mark} colorKey={a.appId} />
          <span>{a.name}</span>
        </span>
      }
    >
      {openable && (
        <Link className="app-card-open" to={environmentPath(environment, a.appId) as "/"}>
          앱 열기 →
        </Link>
      )}
      {!openable && <p>지금은 앱을 사용할 수 없습니다. 담당자에게 문의하세요.</p>}
    </Panel>
  );
}
