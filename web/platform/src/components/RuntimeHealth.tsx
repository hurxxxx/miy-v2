import { useEffect, useState } from "react";
import type { Installation } from "@miy/sdk";

export function RuntimeHealth({ app }: { app: Installation }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 5000);
    return () => clearInterval(timer);
  }, []);
  if (app.kind !== "docker_app") return null;
  const health = app.health;
  const status = app.runtimeStatus === "running" ? health?.status || "unknown" : "not_applicable";
  const recoveryRequired = app.runtimeStatus === "recovery_required";
  const last = app.canManage ? app.lastOperation : undefined;
  const stale = app.runtimeStatus === "running" && !!health?.checkedAt && now - Date.parse(health.checkedAt) > 30000;
  const labels: Record<string, string> = {
    healthy: "정상",
    degraded: `응답 확인 중 (${health?.consecutiveFailures || 0}/3회 실패)`,
    unhealthy: "응답 비정상",
    unknown: "확인 전 또는 확인 불가",
    not_applicable: "검사 대상 아님",
  };
  return (
    <div
      role="status"
      aria-label="앱 실행 및 서비스 상태"
      className={`runtime-health ${status === "unhealthy" ? "is-unhealthy" : ""}`.trim()}
    >
      <p>
        컨테이너: {recoveryRequired ? "복구 필요" : app.runtimeStatus} · 서비스: {stale ? "확인 지연" : labels[status]}
      </p>
      {recoveryRequired && <p>앱이 중지됐습니다. 관리자가 DB 상태를 확인하고 호환되는 버전을 배포해야 합니다.</p>}
      {last?.status === "failed" && last.releaseId && ["deploy", "promote"].includes(last.action) && (
        <p>배포 실패 · 시도한 릴리스 <code>{last.releaseId}</code></p>
      )}
      {status === "unhealthy" && <p>앱 응답이 정상적이지 않습니다. 관리자가 로그와 상태를 확인해야 합니다.</p>}
      {health?.checkedAt && <p>마지막 서비스 검사: {new Date(health.checkedAt).toLocaleString()}</p>}
      {app.canManage && health?.errorCode && <p>검사 결과: {health.errorCode}</p>}
    </div>
  );
}
