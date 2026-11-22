import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { request } from "@miy/sdk";
import { useInstallations, useManagedInstallations } from "../data/installations";
import { useUser, useSurface } from "../data/session";
import { environmentPath, surfaceUrl } from "../lib/paths";
import { NewTabLink } from "../components/NewTabLink";
import { LoadError, StaleNotice } from "../components/LoadState";
import { RuntimeHealth } from "../components/RuntimeHealth";
import { RuntimeControls } from "../components/RuntimeControls";
import { PermissionApproval } from "../components/PermissionApproval";
import { Badge, Button, EmptyState, Field, Loading, Panel } from "../ui";
import { AppAccessAdmin, GroupAdmin, useGroups } from "./AdminAccess";
import { AuditLog } from "./AdminAudit";
import { SupportReports } from "./AdminReports";
import { UserAdmin, type Account } from "./AdminUsers";

type Registration = { id: string; appId: string; name: string; status: string; requester: string };

export function Admin() {
  const { t } = useTranslation("console");
  const user = useUser();
  if (!user.admin)
    return (
      <Panel title={t("admin.adminRequired")}>
        <p>{t("admin.signInAsAdmin")}</p>
      </Panel>
    );
  return <AdminConsole csrf={user.csrf} selfId={user.id} />;
}

function AdminConsole({ csrf, selfId }: { csrf: string; selfId: string }) {
  const { t } = useTranslation("console");
  const surface = useSurface();
  // Installations whose release asks for platform permissions not yet approved.
  const [pendingApprovals, setPendingApprovals] = useState<Record<string, number>>({});
  const pendingApproval = useCallback(
    (id: string, count: number) => setPendingApprovals((current) => (current[id] === count ? current : { ...current, [id]: count })),
    [],
  );
  // The runtime panel uses the management list; the shared list feeds the rail.
  const installations = useInstallations();
  const managed = useManagedInstallations();
  const [requests, setRequests] = useState<Registration[] | null>(null);
  const [users, setUsers] = useState<Account[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [message, setMessage] = useState("");
  const groups = useGroups(setMessage);
  const loadDirectory = useCallback(async () => {
    try {
      const [nextRequests, nextUsers] = await Promise.all([
        request<Registration[]>("/api/v1/app-registrations"),
        request<Account[]>("/api/v1/users"),
      ]);
      setRequests(nextRequests);
      setUsers(nextUsers);
      setLoadError(false);
    } catch {
      setLoadError(true);
    }
  }, []);
  const refresh = useCallback(() => {
    void loadDirectory();
    void installations.refresh();
    void managed.refresh();
  }, [loadDirectory, installations.refresh, managed.refresh]);
  useEffect(() => {
    void loadDirectory();
    const timer = setInterval(() => void loadDirectory(), 3000);
    return () => clearInterval(timer);
  }, [loadDirectory]);
  async function decide(id: string, approve: boolean) {
    try {
      await request(`/api/v1/app-registrations/${id}/decision`, csrf, {
        method: "POST",
        body: JSON.stringify({ approve }),
      });
      setMessage(t(approve ? "admin.registrationApproved" : "admin.registrationRejected"));
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  }
  async function grant(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    try {
      await request("/api/v1/grants", csrf, {
        method: "PUT",
        body: JSON.stringify({
          userId: form.get("user"),
          appId: form.get("app"),
          environment: form.get("environment"),
          permission: form.get("permission"),
          enabled: form.get("enabled") === "true",
        }),
      });
      setMessage(t("admin.grantChanged"));
    } catch (e) {
      setMessage(String(e));
    }
  }
  const pending = requests?.filter((r) => r.status === "pending") ?? [];
  return (
    <>
      <div className="page-header">
        <div>
          <h1>{t("admin.title")}</h1>
          <p>{t("admin.description")}</p>
        </div>
      </div>
      {loadError && <LoadError message={t("admin.directoryLoadFailed")} onRetry={refresh} />}
      {Object.values(pendingApprovals).some((count) => count > 0) && (
        <p role="status" className="status-message">
          {t("admin.approvalsWaiting", { count: Object.values(pendingApprovals).filter((count) => count > 0).length })}
        </p>
      )}
      {message && (
        <p role="status" className="status-message">
          {message}
        </p>
      )}
      <div className="admin-grid">
        <Panel title={t("admin.registrations")} actions={requests && <Badge tone={pending.length ? "info" : "neutral"}>{t("admin.count", { count: pending.length })}</Badge>}>
          {requests === null ? (
            !loadError && <Loading />
          ) : pending.length === 0 ? (
            <p className="ui-empty-inline">{t("admin.noPendingRegistrations")}</p>
          ) : (
            pending.map((r) => (
              <div key={r.id} className="registration-row">
                <p>
                  <strong>{r.name}</strong> · <code>{r.appId}</code>
                </p>
                <div className="registration-actions">
                  <Button size="sm" variant="primary" onClick={() => decide(r.id, true)}>
                    {t("admin.approve")}
                  </Button>
                  <Button size="sm" onClick={() => decide(r.id, false)}>
                    {t("admin.reject")}
                  </Button>
                </div>
              </div>
            ))
          )}
        </Panel>
        <Panel title={t("admin.grants")}>
          <form className="grant-form" onSubmit={grant}>
            <Field label={t("admin.user")}>
              <select name="user" disabled={users === null}>
                {(users ?? []).map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t("admin.appId")}>
              <input name="app" required pattern="[a-z][a-z0-9-]*" autoComplete="off" />
            </Field>
            <Field label={t("admin.environment")}>
              <select name="environment">
                <option value="dev">{t("environments.dev")}</option>
                <option value="prod">{t("environments.prod")}</option>
              </select>
            </Field>
            <Field label={t("admin.permission")}>
              <select name="permission">
                <option value="use">{t("admin.permissions.use")}</option>
                <option value="develop">{t("admin.permissions.develop")}</option>
                <option value="deploy">{t("admin.permissions.deploy")}</option>
              </select>
            </Field>
            <Field label={t("admin.grantOrRevoke")}>
              <select name="enabled">
                <option value="true">{t("admin.grant")}</option>
                <option value="false">{t("admin.revoke")}</option>
              </select>
            </Field>
            <div className="grant-form-actions">
              <Button type="submit" variant="primary" disabled={users === null}>
                {t("admin.applyGrant")}
              </Button>
            </div>
          </form>
        </Panel>
      </div>
      <h2 className="page-section-title">{t("admin.accounts")}</h2>
      <div className="admin-grid">
        <UserAdmin csrf={csrf} selfId={selfId} users={users} report={setMessage} refresh={refresh} />
      </div>
      <h2 className="page-section-title">{t("admin.access")}</h2>
      <div className="admin-grid">
        <GroupAdmin csrf={csrf} users={users} report={setMessage} directory={groups} />
        <AppAccessAdmin csrf={csrf} users={users} report={setMessage} directory={groups} />
      </div>
      <SupportReports csrf={csrf} />
      <AuditLog users={users} report={setMessage} />
      <h2 className="page-section-title">{t("admin.runtime")}</h2>
      {managed.items === null ? (
        managed.status === "error" ? (
          <LoadError message={t("admin.appsLoadFailed")} onRetry={refresh} />
        ) : (
          <Loading>{t("admin.appsLoading")}</Loading>
        )
      ) : (
        <>
          {managed.status === "error" && <StaleNotice updatedAt={managed.updatedAt} onRetry={refresh} />}
          {managed.items.filter((a) => a.kind === "docker_app").length === 0 ? (
            <EmptyState title={t("admin.noDeployedApps")} />
          ) : (
            <div className="card-grid">
              {managed.items
                .filter((a) => a.kind === "docker_app")
                .map((a) => (
                  <Panel
                    key={a.id}
                    title={`${a.name} · ${a.environment}`}
                    actions={a.appDisabled && <Badge tone="warning">{t("admin.companyDisabled")}</Badge>}
                  >
                    <p>
                      {a.version} · DB {a.dbStatus}
                    </p>
                    {a.canOpen ? (
                      <NewTabLink href={surfaceUrl(a.environment === "prod" ? surface.portalOrigin : surface.devPortalOrigin, environmentPath(a.environment, a.appId))}>{t("admin.openInPortal")}</NewTabLink>
                    ) : (
                      <p className="ui-empty-inline">
                        {t(a.appDisabled ? "admin.disabledCannotOpen" : "admin.cannotOpen")}
                      </p>
                    )}
                    <RuntimeHealth app={a} />
                    {a.canManage && <RuntimeControls app={a} csrf={csrf} refresh={refresh} />}
                    <PermissionApproval installationId={a.id} releaseId={a.releaseId} csrf={csrf} onChange={(count) => pendingApproval(a.id, count)} />
                  </Panel>
                ))}
            </div>
          )}
        </>
      )}
    </>
  );
}
