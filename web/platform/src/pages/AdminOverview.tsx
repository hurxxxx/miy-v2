import { useEffect, useState } from "react";
import { Link } from "@tanstack/react-router";
import { request, type AuditPage } from "@miy/sdk";
import { useTranslation } from "react-i18next";
import { LoadError } from "../components/LoadState";
import { formatDateTime } from "../platform/time/time-utils";
import { Badge, Loading, Panel } from "../ui";
import { describeAuditAction } from "./AdminAudit";

type Installation = { appId: string; name: string; environment: "dev" | "prod" };
type Issue = "failed" | "recovery_required" | "unreachable" | "unexpected_stop" | "degraded" | "unhealthy";
type Overview = {
  pendingRegistrations: number;
  pendingApprovals: number;
  runtimeIssues: number;
  approvalExamples: Installation[];
  runtimeExamples: (Installation & { issue: Issue })[];
};

export function AdminOverview() {
  const { t, i18n } = useTranslation("console");
  const [attempt, reload] = useState(0);
  const [summary, setSummary] = useState<Overview | null | false>(null);
  const [audit, setAudit] = useState<AuditPage | null | false>(null);

  useEffect(() => {
    const controller = new AbortController();
    const { signal } = controller;
    setSummary(null);
    setAudit(null);
    void request<Overview>("/api/v1/admin/overview", undefined, { signal }).then(setSummary, () => {
      if (!signal.aborted) setSummary(false);
    });
    void request<AuditPage>("/api/v1/audit?limit=5", undefined, { signal }).then(setAudit, () => {
      if (!signal.aborted) setAudit(false);
    });
    return () => controller.abort();
  }, [attempt]);

  const retry = () => reload((value) => value + 1);
  const number = new Intl.NumberFormat(i18n.language);
  return (
    <div className="admin-overview">
      {summary === false ? <LoadError message={t("overview.loadFailed")} onRetry={retry} />
        : summary === null ? <Loading /> : (
          <div className="admin-overview-grid">
            <Panel title={t("overview.registrationQueue")}>
              <p className="admin-metric">{number.format(summary.pendingRegistrations)}</p>
              <Link to="/admin/apps" hash="registration-requests">{t("overview.openRegistrations")}</Link>
            </Panel>
            <Panel title={t("overview.permissionQueue")}>
              <p className="admin-metric">{number.format(summary.pendingApprovals)}</p>
              {summary.approvalExamples.length > 0 && <ul className="admin-overview-list">
                {summary.approvalExamples.map((app) => (
                  <li key={`${app.appId}:${app.environment}`}>
                    <Link to="/admin/apps/$appId/$tab" params={{ appId: app.appId, tab: "installations" }}>{app.name}</Link>
                    <Badge>{t(`environments.${app.environment}`)}</Badge>
                  </li>
                ))}
              </ul>}
            </Panel>
            <Panel title={t("overview.runtimeIssues")}>
              <p className="admin-metric">{number.format(summary.runtimeIssues)}</p>
              {summary.runtimeExamples.length > 0 && <ul className="admin-overview-list">
                {summary.runtimeExamples.map((app) => (
                  <li key={`${app.appId}:${app.environment}`}>
                    <Link to="/admin/apps/$appId/$tab" params={{ appId: app.appId, tab: "runtime" }}>{app.name}</Link>
                    <Badge>{t(`environments.${app.environment}`)}</Badge>
                    <Badge tone="warning">{t(`overview.issues.${app.issue}`)}</Badge>
                  </li>
                ))}
              </ul>}
            </Panel>
          </div>
        )}
      <Panel title={t("overview.recentAudit")} actions={<Link to="/admin/audit">{t("overview.openAudit")}</Link>}>
        {audit === false ? <LoadError message={t("audit.loadFailed")} onRetry={retry} />
          : audit === null ? <Loading /> : audit.entries.length === 0 ? (
            <p className="ui-empty-inline">{t("audit.empty")}</p>
          ) : (
            <ul className="admin-overview-list">
              {audit.entries.map((entry) => (
                <li key={entry.id}>
                  <span>{describeAuditAction(t, entry.action)}</span>
                  <span>{entry.actorName ?? entry.actor ?? t("audit.system")}</span>
                  <time dateTime={entry.at}>{formatDateTime(entry.at, { dateStyle: "short", timeStyle: "short" })}</time>
                </li>
              ))}
            </ul>
          )}
      </Panel>
    </div>
  );
}
