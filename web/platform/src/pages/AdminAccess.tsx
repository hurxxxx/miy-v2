import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { request, type Environment, type Group } from "@miy/sdk";
import { LoadError } from "../components/LoadState";
import { useAdminAppRead } from "../data/admin-apps";
import { Badge, Button, Field, Loading, Panel, useConfirm } from "../ui";

export type DirectoryUser = { id: string; name: string };
type Account = DirectoryUser & { enabled: boolean };
type GroupGrant = {
  groupId: string;
  environment: Environment;
  permission: "use" | "develop";
};
type UserGrant = {
  userId: string;
  environment: Environment;
  permission: "use" | "develop" | "deploy";
};
type Access = {
  appId: string;
  disabled: boolean;
  admission: Record<Environment, "selected" | "all">;
  userGrants: UserGrant[];
  groupGrants: GroupGrant[];
};

type AdminAccessProps = {
  csrf: string;
  report: (message: string) => void;
};

const environments = ["dev", "prod"] as const;
const groupPermissions = ["use", "develop"] as const;
const individualPermissions = [...groupPermissions, "deploy"] as const;

function GrantScope({ individual = false }: { individual?: boolean }) {
  const { t } = useTranslation("console");
  return (
    <>
      <Field label={t("admin.environment")}>
        <select name="environment">
          {environments.map((environment) => (
            <option value={environment} key={environment}>{t(`environments.${environment}`)}</option>
          ))}
        </select>
      </Field>
      <Field label={t("admin.permission")} hint={individual ? undefined : t("access.deployPerPerson")}>
        <select name="permission">
          {(individual ? individualPermissions : groupPermissions).map((permission) => (
            <option value={permission} key={permission}>
              {t(`${individual ? "admin" : "access"}.permissions.${permission}`)}
            </option>
          ))}
        </select>
      </Field>
    </>
  );
}

function AccountSelect({ accounts }: { accounts: Account[] | undefined }) {
  const { t, i18n } = useTranslation("console");
  return (
    <Field label={t("admin.user")}>
      <select name="user" required defaultValue="" disabled={!accounts?.length}>
        <option value="" disabled>{t("access.chooseUser")}</option>
        {[...(accounts ?? [])].sort((a, b) => a.name.localeCompare(b.name, i18n.language)).map((user) => (
          <option key={user.id} value={user.id}>
            {user.name}{user.enabled ? "" : ` (${t("access.userDisabled")})`}
          </option>
        ))}
      </select>
    </Field>
  );
}

export function PreRegistrationGrant({ csrf, report }: AdminAccessProps) {
  const { t } = useTranslation("console");
  const accounts = useAdminAppRead<Account[]>("/api/v1/users");
  const [saving, setSaving] = useState(false);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    const form = new FormData(event.currentTarget);
    setSaving(true);
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
      report(t("admin.grantChanged"));
    } catch (error) {
      report(String(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="access-detail">
      <p className="access-hint">{t("access.preRegistrationHint")}</p>
      {accounts.error && <LoadError message={t("access.usersLoadFailed")} onRetry={accounts.refresh} />}
      {!accounts.data && !accounts.error && <Loading />}
      {accounts.data && (
        <form className="access-toolbar" onSubmit={submit}>
          <Field label={t("admin.appId")}>
            <input name="app" required maxLength={48} pattern="[a-z](?:[a-z0-9]|-)*" autoComplete="off" />
          </Field>
          <AccountSelect accounts={accounts.data} />
          <GrantScope individual />
          <Field label={t("admin.grantOrRevoke")}>
            <select name="enabled">
              <option value="true">{t("admin.grant")}</option>
              <option value="false">{t("admin.revoke")}</option>
            </select>
          </Field>
          <Button type="submit" disabled={!!accounts.error || !accounts.data.length || saving}>{t("admin.applyGrant")}</Button>
        </form>
      )}
    </div>
  );
}

export function AppAccessAdmin({
  appId,
  csrf,
  report,
  refreshApp,
}: AdminAccessProps & { appId: string; refreshApp: () => Promise<void> }) {
  const { t } = useTranslation("console");
  const base = `/api/v1/apps/${encodeURIComponent(appId)}`;
  const { data, error, refresh } = useAdminAppRead<Access>(`${base}/access`);
  const users = useAdminAppRead<Account[]>("/api/v1/users");
  const groups = useAdminAppRead<Group[]>("/api/v1/groups");
  const access = data?.appId === appId ? data : undefined;
  const activeGroups = groups.data?.filter((group) => group.active) ?? [];
  const [saving, setSaving] = useState(false);
  const mounted = useRef(false);
  const { confirm, dialog } = useConfirm();
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  async function apply(path: string, body: unknown, done: string, changesApp = false) {
    if (saving) return;
    setSaving(true);
    try {
      await request(path, csrf, { method: "PUT", body: JSON.stringify(body) });
      if (mounted.current) report(done);
    } catch (failure) {
      if (mounted.current) report(String(failure));
    } finally {
      if (mounted.current) {
        await refresh();
        if (changesApp) await refreshApp();
        setSaving(false);
      }
    }
  }

  async function toggleCompanySwitch() {
    if (!access) return;
    if (!access.disabled && !(await confirm({
      title: t("access.disableAppTitle"),
      body: <p>{t("access.disableAppBody")}</p>,
      confirmLabel: t("access.disableApp"),
      tone: "danger",
    }))) return;
    await apply(`${base}/control`, { disabled: !access.disabled }, t(access.disabled ? "access.appEnabled" : "access.appDisabled"), true);
  }

  function addGroupGrant(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    void apply("/api/v1/group-grants", {
      groupId: form.get("group"),
      appId,
      environment: form.get("environment"),
      permission: form.get("permission"),
      enabled: true,
    }, t("access.groupGranted"));
  }

  function addUserGrant(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    void apply("/api/v1/grants", {
      userId: form.get("user"),
      appId,
      environment: form.get("environment"),
      permission: form.get("permission"),
      enabled: true,
    }, t("admin.grantChanged"));
  }

  if (error || (data && !access)) return <LoadError message={t("access.appAccessLoadFailed")} onRetry={refresh} />;
  if (!access) return <Loading />;

  return (
    <Panel title={t("access.appAccess")}>
      {dialog}
      <div className="access-row">
        <span>
          {t("access.companyUse")} {access.disabled ? <Badge tone="danger">{t("access.stopped")}</Badge> : <Badge tone="success">{t("access.inUse")}</Badge>}
        </span>
        <Button size="sm" variant={access.disabled ? "primary" : "danger"} disabled={saving} onClick={() => void toggleCompanySwitch()}>
          {t(access.disabled ? "access.enableApp" : "access.disableApp")}
        </Button>
      </div>
      {environments.map((environment) => (
        <div key={environment} className="access-row">
          <span>{t("access.admission", { environment: t(`environments.${environment}`) })}</span>
          <select
            aria-label={t("access.admission", { environment: t(`environments.${environment}`) })}
            value={access.admission[environment]}
            disabled={saving}
            onChange={(event) =>
              void apply(`${base}/admission`, { environment, mode: event.target.value }, t("access.admissionChanged", { environment: t(`environments.${environment}`) }))
            }
          >
            <option value="selected">{t("access.admissionSelected")}</option>
            <option value="all">{t("access.admissionAll")}</option>
          </select>
        </div>
      ))}
      <div className="access-detail">
        <h3>{t("access.groupGrants")}</h3>
        {groups.error && <LoadError message={t("access.groupsLoadFailed")} onRetry={groups.refresh} />}
        {!groups.data && !groups.error && <Loading />}
        <form className="access-toolbar" onSubmit={addGroupGrant}>
          <Field label={t("access.group")}>
            <select name="group" required defaultValue="" disabled={!activeGroups.length}>
              <option value="" disabled>{t("access.chooseGroup")}</option>
              {activeGroups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}
            </select>
          </Field>
          <GrantScope />
          <Button type="submit" disabled={!!groups.error || !activeGroups.length || saving}>{t("admin.grant")}</Button>
        </form>
        {access.groupGrants.length === 0 ? (
          <p className="ui-empty-inline">{t("access.noGroupGrants")}</p>
        ) : (
          <ul className="access-members">
            {access.groupGrants.map((grant) => (
              <li key={`${grant.groupId}-${grant.environment}-${grant.permission}`}>
                <span>
                  {groups.data?.find((group) => group.id === grant.groupId)?.name ?? grant.groupId} · {t(`environments.${grant.environment}`)} · {t(`access.permissions.${grant.permission}`)}
                </span>
                <Button
                  size="sm"
                  disabled={saving}
                  onClick={() => void apply("/api/v1/group-grants", { ...grant, appId, enabled: false }, t("access.groupRevoked"))}
                >
                  {t("admin.revoke")}
                </Button>
              </li>
            ))}
          </ul>
        )}
        <h3>{t("access.userGrants")}</h3>
        {users.error && <LoadError message={t("access.usersLoadFailed")} onRetry={users.refresh} />}
        {!users.data && !users.error && <Loading />}
        <form className="access-toolbar" onSubmit={addUserGrant}>
          <AccountSelect accounts={users.data} />
          <GrantScope individual />
          <Button type="submit" disabled={!!users.error || !users.data?.length || saving}>{t("admin.grant")}</Button>
        </form>
        {access.userGrants.length === 0 ? (
          <p className="ui-empty-inline">{t("access.noUserGrants")}</p>
        ) : (
          <ul className="access-members">
            {access.userGrants.map((grant) => (
              <li key={`${grant.userId}-${grant.environment}-${grant.permission}`}>
                <span>
                  {users.data?.find((user) => user.id === grant.userId)?.name ?? grant.userId} · {t(`environments.${grant.environment}`)} · {t(`access.permissions.${grant.permission}`)}
                </span>
                <Button
                  size="sm"
                  disabled={saving}
                  onClick={() => void apply("/api/v1/grants", { ...grant, appId, enabled: false }, t("access.userRevoked"))}
                >
                  {t("admin.revoke")}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </Panel>
  );
}
