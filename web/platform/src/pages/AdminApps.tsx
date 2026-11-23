import { useState } from "react";
import { Link, useParams } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { isApiError, request, type App, type AppDetail, type Installation } from "@miy/sdk";
import { FileQuota } from "../components/FileQuota";
import { LoadError, StaleNotice } from "../components/LoadState";
import { NewTabLink } from "../components/NewTabLink";
import { PermissionApproval } from "../components/PermissionApproval";
import { RuntimeControls } from "../components/RuntimeControls";
import { RuntimeHealth } from "../components/RuntimeHealth";
import { useAdminAppRead } from "../data/admin-apps";
import { distributionLabelKeys } from "../data/installations";
import { useSurface } from "../data/session";
import { environmentOrigin, environmentPath, surfaceUrl } from "../lib/paths";
import { Badge, Button, EmptyState, Field, Loading, Panel } from "../ui";
import { AppAccessAdmin, PreRegistrationGrant } from "./AdminAccess";

type Props = { csrf: string; report: (message: string) => void };
type Registration = { id: string; appId: string; name: string; requestedBy: string; status: string };
type AppPage = { apps: App[]; nextCursor: string | null };
const tabs = ["access", "installations", "runtime"] as const;
const catalogPath = "/admin/apps";
const refreshInterval = 3000;

export function AdminApps(props: Props) {
  const { appId, tab } = useParams({ strict: false }) as { appId?: string; tab?: string };
  return appId
    ? <AdminAppDetail key={appId} appId={appId} tab={tab ?? "access"} {...props} />
    : <AdminAppsCatalog {...props} />;
}

function AdminAppsCatalog({ csrf, report }: Props) {
  const { t, i18n } = useTranslation(["console", "portal"]);
  const [cursors, setCursors] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const cursor = cursors.at(-1);
  const catalog = useAdminAppRead<AppPage>(
    `/api/v1/apps?limit=50${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`,
  );
  const normalized = search.trim().toLocaleLowerCase(i18n.language);
  const filtered = catalog.data?.apps.filter((app) =>
    `${app.name} ${app.id}`.toLocaleLowerCase(i18n.language).includes(normalized),
  );
  function showNewApp() {
    if (cursors.length) setCursors([]);
    else void catalog.refresh();
  }

  return (
    <>
      <Panel title={t("apps.catalog")}>
        <div className="access-toolbar">
          <Field label={t("apps.filterPage")}>
            <input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("apps.filterPlaceholder")} />
          </Field>
        </div>
        {catalog.error && <LoadError message={t("admin.appsLoadFailed")} onRetry={catalog.refresh} />}
        {!catalog.data && !catalog.error && <Loading />}
        {filtered && (filtered.length ? (
          <div className="access-table-scroll">
            <table className="access-table">
              <thead>
                <tr>
                  <th scope="col">{t("apps.name")}</th>
                  <th scope="col">{t("apps.distribution")}</th>
                  <th scope="col">{t("apps.companyUse")}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((app) => (
                  <tr key={app.id}>
                    <td>
                      <Link to="/admin/apps/$appId" params={{ appId: app.id }} className="font-semibold">{app.name}</Link>
                      <div><code>{app.id}</code></div>
                    </td>
                    <td>{t(distributionLabelKeys[app.distribution ?? "organization"], { ns: "portal" })}</td>
                    <td><Badge tone={app.disabled ? "warning" : "neutral"}>{t(app.disabled ? "admin.companyDisabled" : "apps.allowed")}</Badge></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState title={t(normalized ? "apps.noMatches" : "apps.noCatalogApps")} />
        ))}
        {(cursor || catalog.data?.nextCursor) && (
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button size="sm" disabled={!cursor} onClick={() => setCursors((current) => current.slice(0, -1))}>
              {t("apps.previousPage")}
            </Button>
            <span>{t("apps.page", { page: cursors.length + 1 })}</span>
            <Button
              size="sm"
              disabled={!catalog.data?.nextCursor}
              onClick={() => {
                const next = catalog.data?.nextCursor;
                if (next) setCursors((current) => [...current, next]);
              }}
            >
              {t("apps.nextPage")}
            </Button>
          </div>
        )}
      </Panel>
      <div id="registration-requests" className="mt-4">
        <RegistrationRequests csrf={csrf} report={report} onApproved={showNewApp} />
      </div>
    </>
  );
}

function RegistrationRequests({ csrf, report, onApproved }: Props & { onApproved: () => void }) {
  const { t } = useTranslation("console");
  const registrations = useAdminAppRead<Registration[]>("/api/v1/app-registrations", refreshInterval);
  const [saving, setSaving] = useState<string>();
  const [preassignOpen, setPreassignOpen] = useState(false);
  const pending = registrations.data?.filter((entry) => entry.status === "pending");
  async function decide(id: string, approve: boolean) {
    if (saving) return;
    setSaving(id);
    try {
      await request(`/api/v1/app-registrations/${encodeURIComponent(id)}/decision`, csrf, {
        method: "POST",
        body: JSON.stringify({ approve }),
      });
      report(t(approve ? "admin.registrationApproved" : "admin.registrationRejected"));
      if (approve) onApproved();
    } catch (error) {
      report(String(error));
    } finally {
      await registrations.refresh();
      setSaving(undefined);
    }
  }

  return (
    <Panel title={t("admin.registrations")} actions={pending && <Badge tone={pending.length ? "info" : "neutral"}>{pending.length}</Badge>}>
      {registrations.error && <LoadError message={t("apps.registrationsLoadFailed")} onRetry={registrations.refresh} />}
      {!pending && !registrations.error && <Loading />}
      {pending && (pending.length ? (
        pending.map((entry) => (
          <div key={entry.id} className="registration-row">
            <div>
              <p><strong>{entry.name}</strong> · <code>{entry.appId}</code></p>
              <p className="text-sm text-app-ink-muted">{t("apps.requestedBy")} <code>{entry.requestedBy}</code></p>
            </div>
            <div className="registration-actions">
              <Button size="sm" variant="primary" disabled={!!saving} onClick={() => void decide(entry.id, true)}>
                {t("admin.approve")}
              </Button>
              <Button size="sm" disabled={!!saving} onClick={() => void decide(entry.id, false)}>
                {t("admin.reject")}
              </Button>
            </div>
          </div>
        ))
      ) : (
        <p className="ui-empty-inline">{t("admin.noPendingRegistrations")}</p>
      ))}
      <details className="mt-4" onToggle={(event) => setPreassignOpen(event.currentTarget.open)}>
        <summary>{t("access.preRegistration")}</summary>
        {preassignOpen && <PreRegistrationGrant csrf={csrf} report={report} />}
      </details>
    </Panel>
  );
}

function AdminAppDetail({ appId, tab, csrf, report }: Props & { appId: string; tab: string }) {
  const { t } = useTranslation(["console", "portal"]);
  const detail = useAdminAppRead<AppDetail>(`/api/v1/apps/${encodeURIComponent(appId)}`, tab === "access" ? undefined : refreshInterval);
  const app = detail.data?.app.id === appId ? detail.data.app : undefined;
  const installations = detail.data?.installations.filter((installation) =>
    installation.kind === "docker_app" && installation.appId === appId,
  ) ?? [];
  const missing = isApiError(detail.error) && detail.error.status === 404;
  const failed = detail.error || (detail.data && !app);

  return (
    <>
      <div className="mb-4"><Link to={catalogPath}>← {t("apps.backToCatalog")}</Link></div>
      {missing ? <EmptyState title={t("apps.notFound")} /> : failed && (
        app ? <StaleNotice updatedAt={detail.updatedAt} onRetry={detail.refresh} /> : (
          <LoadError message={t("apps.detailLoadFailed")} onRetry={detail.refresh} />
        )
      )}
      {!app && !failed && <Loading />}
      {app && !missing && (
        <>
          <div className="page-header">
            <div>
              <h2>{app.name}</h2>
              <p><code>{app.id}</code> · {t(distributionLabelKeys[app.distribution ?? "organization"], { ns: "portal" })}</p>
            </div>
            {app.disabled && <Badge tone="warning">{t("admin.companyDisabled")}</Badge>}
          </div>
          <nav aria-label={t("apps.sections")} className="mb-4 flex flex-wrap gap-2">
            {tabs.map((section) => (
              <Link
                key={section}
                to="/admin/apps/$appId/$tab"
                params={{ appId, tab: section }}
                aria-current={tab === section ? "page" : undefined}
                className={`ui-button ui-button-sm ${tab === section ? "ui-button-primary" : "ui-button-secondary"}`}
              >
                {t(`apps.tabs.${section}`)}
              </Link>
            ))}
          </nav>
          {tab === "access" && <AppAccessAdmin appId={appId} csrf={csrf} report={report} refreshApp={detail.refresh} />}
          {tab === "installations" && (
            <InstallationView installations={installations} csrf={csrf} mode="installations" refresh={detail.refresh} />
          )}
          {tab === "runtime" && (
            <InstallationView installations={installations} csrf={csrf} mode="runtime" refresh={detail.refresh} />
          )}
          {!tabs.some((section) => tab === section) && <EmptyState title={t("apps.unknownSection")} />}
        </>
      )}
    </>
  );
}

function InstallationView({
  installations,
  csrf,
  mode,
  refresh,
}: {
  installations: Installation[];
  csrf: string;
  mode: "installations" | "runtime";
  refresh: () => void;
}) {
  const { t } = useTranslation(["console", "portal"]);
  const surface = useSurface();
  if (!installations.length) return <EmptyState title={t("admin.noDeployedApps")} />;

  return (
    <div className="card-grid">
      {installations.map((app) => (
        <Panel
          key={app.id}
          title={t(`environments.${app.environment}`)}
          actions={app.appDisabled && mode === "runtime" && <Badge tone="warning">{t("admin.companyDisabled")}</Badge>}
        >
          <p className="app-card-meta">
            <Badge>{t("apps.version", { version: app.version || "—" })}</Badge>
            <Badge>DB {app.dbStatus}</Badge>
          </p>
          {mode === "installations" ? (
            <>
              <p>{t("apps.release")} <code>{app.releaseId}</code></p>
              <PermissionApproval key={app.releaseId} installationId={app.id} releaseId={app.releaseId} csrf={csrf} />
            </>
          ) : (
            <>
              {app.canOpen ? (
                <NewTabLink href={surfaceUrl(environmentOrigin(surface, app.environment), environmentPath(app.environment, app.appId))}>
                  {t("admin.openInPortal")}
                </NewTabLink>
              ) : (
                <p className="ui-empty-inline">{t(app.appDisabled ? "admin.disabledCannotOpen" : "admin.cannotOpen")}</p>
              )}
              <RuntimeHealth app={app} />
              {app.lastOperation && (
                <p>
                  {t("apps.lastOperation")}{" "}
                  {t(`runtimeControls.actions.${app.lastOperation.action}`, { ns: "portal", defaultValue: app.lastOperation.action })}
                  {" · "}<code>{app.lastOperation.status}/{app.lastOperation.step}</code>
                </p>
              )}
              {app.canManage && <RuntimeControls app={app} csrf={csrf} refresh={refresh} />}
              <FileQuota installationId={app.id} csrf={csrf} />
            </>
          )}
        </Panel>
      ))}
    </div>
  );
}
