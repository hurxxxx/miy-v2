import { useMemo } from "react";
import { Link, useLocation } from "@tanstack/react-router";
import type { Installation } from "@miy/sdk";
import { installationsIn, useInstallationPolling, useInstallations } from "../data/installations";
import { useUser } from "../data/session";
import { currentEnvironment, environmentPath } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { RuntimeHealth } from "../components/RuntimeHealth";
import { RuntimeControls } from "../components/RuntimeControls";
import { AppMark, Badge, EmptyState, Loading, Panel, monograms, type Tone } from "../ui";

const runtimeTone: Record<string, Tone> = { running: "success", stopped: "neutral", failed: "danger", removed: "neutral" };

export function Home() {
  const user = useUser();
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
                  csrf={user.csrf}
                  refresh={refresh}
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
  csrf,
  refresh,
}: {
  app: Installation;
  mark: string;
  environment: string;
  csrf: string;
  refresh: () => void;
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
      <div className="app-card-meta">
        <Badge tone={runtimeTone[a.runtimeStatus] ?? "neutral"}>{a.runtimeStatus}</Badge>
        <Badge>DB {a.dbStatus}</Badge>
        <Badge>v{a.version || "—"}</Badge>
        <code>{a.appId}</code>
      </div>
      <RuntimeHealth app={a} />
      {openable && (
        <Link className="app-card-open" to={environmentPath(environment, a.appId) as "/"}>
          앱 열기 →
        </Link>
      )}
      {a.kind === "docker_app" && a.canManage && <RuntimeControls app={a} csrf={csrf} refresh={refresh} />}
    </Panel>
  );
}
