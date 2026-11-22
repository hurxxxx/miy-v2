import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Installation } from "@miy/sdk";

export function RuntimeHealth({ app }: { app: Installation }) {
  const { t, i18n } = useTranslation("portal");
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
    healthy: t("runtimeHealth.statuses.healthy"),
    degraded: t("runtimeHealth.statuses.degraded", { failures: health?.consecutiveFailures || 0 }),
    unhealthy: t("runtimeHealth.statuses.unhealthy"),
    unknown: t("runtimeHealth.statuses.unknown"),
    not_applicable: t("runtimeHealth.statuses.not_applicable"),
  };
  return (
    <div
      role="status"
      aria-label={t("runtimeHealth.label")}
      className={`runtime-health ${status === "unhealthy" ? "is-unhealthy" : ""}`.trim()}
    >
      <p>
        {t("runtimeHealth.summary", {
          container: recoveryRequired ? t("runtimeHealth.recoveryRequired") : app.runtimeStatus,
          service: stale ? t("runtimeHealth.delayed") : labels[status],
        })}
      </p>
      {recoveryRequired && <p>{t("runtimeHealth.stopped")}</p>}
      {last?.status === "failed" && last.releaseId && ["deploy", "promote"].includes(last.action) && (
        <p>{t("runtimeHealth.deployFailed")} <code>{last.releaseId}</code></p>
      )}
      {status === "unhealthy" && <p>{t("runtimeHealth.unhealthyNotice")}</p>}
      {health?.checkedAt && <p>{t("runtimeHealth.lastCheck", { time: new Date(health.checkedAt).toLocaleString(i18n.language) })}</p>}
      {app.canManage && health?.errorCode && <p>{t("runtimeHealth.result", { code: health.errorCode })}</p>}
    </div>
  );
}
