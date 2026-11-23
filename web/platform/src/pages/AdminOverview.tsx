import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { type AdminOverviewResponse, type AuditPage } from "@miy/sdk";
import { useTranslation } from "react-i18next";
import { LoadError } from "../components/LoadState";
import { useAdminAppRead } from "../data/admin-apps";
import { formatDateTime } from "../platform/time/time-utils";
import { Badge, Button, Loading, Panel } from "../ui";
import { describeAuditAction } from "./AdminAudit";

function useOverviewPages() {
  const [pages, setPages] = useState<string[]>([]);
  return {
    cursor: pages.at(-1),
    previous: pages.length > 0 ? () => setPages((current) => current.slice(0, -1)) : undefined,
    next: (cursor: string) => setPages((current) => current.at(-1) === cursor ? current : [...current, cursor]),
  };
}

function OverviewPagination({ previous, next }: { previous?: () => void; next?: () => void }) {
  const { t } = useTranslation("console");
  if (!previous && !next) return null;
  return (
    <div className="mt-3 flex gap-2">
      <Button size="sm" disabled={!previous} onClick={previous}>{t("overview.previousPage")}</Button>
      <Button size="sm" disabled={!next} onClick={next}>{t("overview.nextPage")}</Button>
    </div>
  );
}

export function AdminOverview() {
  const { t, i18n } = useTranslation("console");
  const approvals = useOverviewPages();
  const runtime = useOverviewPages();
  const query = new URLSearchParams();
  if (approvals.cursor) query.set("approvalCursor", approvals.cursor);
  if (runtime.cursor) query.set("runtimeCursor", runtime.cursor);
  const overview = useAdminAppRead<AdminOverviewResponse>(`/api/v1/admin/overview${query.size ? `?${query}` : ""}`);
  const summary = overview.data;
  const audit = useAdminAppRead<AuditPage>("/api/v1/audit?limit=5");
  const approvalNext = summary?.approvalNextCursor;
  const runtimeNext = summary?.runtimeNextCursor;
  const number = new Intl.NumberFormat(i18n.language);
  return (
    <div className="admin-overview">
      {overview.error && <LoadError message={t("overview.loadFailed")} onRetry={overview.refresh} />}
      {!overview.error && !summary && <Loading />}
      {(summary || approvals.previous || runtime.previous) && (
          <div className="admin-overview-grid">
            <Panel title={t("overview.registrationQueue")}>
              {summary && <p className="admin-metric">{number.format(summary.pendingRegistrations)}</p>}
              <Link to="/admin/apps" hash="registration-requests">{t("overview.openRegistrations")}</Link>
            </Panel>
            <Panel title={t("overview.permissionQueue")}>
              {summary && <p className="admin-metric">{number.format(summary.pendingApprovals)}</p>}
              {!!summary?.approvalExamples.length && <ul className="admin-overview-list">
                {summary.approvalExamples.map((app) => (
                  <li key={`${app.appId}:${app.environment}`}>
                    <Link to="/admin/apps/$appId/$tab" params={{ appId: app.appId, tab: "installations" }}>{app.name}</Link>
                    <Badge>{t(`environments.${app.environment}`)}</Badge>
                  </li>
                ))}
              </ul>}
              <OverviewPagination previous={approvals.previous} next={approvalNext ? () => approvals.next(approvalNext) : undefined} />
            </Panel>
            <Panel title={t("overview.runtimeIssues")}>
              {summary && <p className="admin-metric">{number.format(summary.runtimeIssues)}</p>}
              {!!summary?.runtimeExamples.length && <ul className="admin-overview-list">
                {summary.runtimeExamples.map((app) => (
                  <li key={`${app.appId}:${app.environment}`}>
                    <Link to="/admin/apps/$appId/$tab" params={{ appId: app.appId, tab: "runtime" }}>{app.name}</Link>
                    <Badge>{t(`environments.${app.environment}`)}</Badge>
                    <Badge tone="warning">{t(`overview.issues.${app.issue}`)}</Badge>
                  </li>
                ))}
              </ul>}
              <OverviewPagination previous={runtime.previous} next={runtimeNext ? () => runtime.next(runtimeNext) : undefined} />
            </Panel>
          </div>
        )}
      <Panel title={t("overview.recentAudit")} actions={<Link to="/admin/audit">{t("overview.openAudit")}</Link>}>
        {audit.error ? <LoadError message={t("audit.loadFailed")} onRetry={audit.refresh} />
          : !audit.data ? <Loading /> : audit.data.entries.length === 0 ? (
            <p className="ui-empty-inline">{t("audit.empty")}</p>
          ) : (
            <ul className="admin-overview-list">
              {audit.data.entries.map((entry) => (
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
