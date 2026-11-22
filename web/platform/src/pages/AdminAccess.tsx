import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { request, type Group } from "@miy/sdk";
import { Badge, Button, Field, Loading, Panel, useConfirm } from "../ui";

// Users and groups receive app permissions here; apps keep their internal data access.
// Group permissions are computed on every request, so nothing is copied to users.

export type DirectoryUser = { id: string; name: string };
type Member = { id: string; name: string; enabled: boolean };
type Environment = "dev" | "prod";
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


type Report = (message: string) => void;

// Bind each response to the selection that requested it. Refreshing after a
// mutation reloads the current selection, even if the user switched meanwhile.
function useSelectedResource<T>(id: string | null, path: string | null, report: Report, errorMessage: string) {
  const [result, setResult] = useState<{ id: string; data: T } | null>(null);
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    setResult((previous) => (previous?.id === id ? previous : null));
    if (!id || !path) return;
    let current = true;
    request<T>(path)
      .then((data) => {
        if (current) setResult({ id, data });
      })
      .catch((error) => {
        if (current) report(`${errorMessage}: ${String(error)}`);
      });
    return () => {
      current = false;
    };
  }, [id, path, revision, report, errorMessage]);
  return { data: result?.id === id ? result.data : null, refresh };
}

export function useGroups(report: Report) {
  const { t } = useTranslation("console");
  const { data: groups, refresh } = useSelectedResource<Group[]>("groups", "/api/v1/groups", report, t("access.groupsLoadFailed"));
  return { groups, refresh };
}

async function send(csrf: string, path: string, method: "POST" | "PUT" | "PATCH", body: unknown) {
  await request(path, csrf, { method, body: JSON.stringify(body) });
}

type AdminAccessProps = {
  csrf: string;
  users: DirectoryUser[] | null;
  report: Report;
  directory: ReturnType<typeof useGroups>;
};

export function GroupAdmin({ csrf, users, report, directory }: AdminAccessProps) {
  const { t } = useTranslation("console");
  const { groups, refresh } = directory;
  const [selected, setSelected] = useState<string | null>(null);
  const { data: members, refresh: refreshMembers } = useSelectedResource<Member[]>(
    selected,
    selected ? `/api/v1/groups/${selected}/members` : null,
    report,
    t("access.membersLoadFailed"),
  );
  const [source, setSource] = useState<"hr" | "local">("local");
  const group = groups?.find((g) => g.id === selected) ?? null;
  const hrGroups = (groups ?? []).filter((g) => g.source === "hr" && g.active);
  async function apply(path: string, method: "POST" | "PUT" | "PATCH", body: unknown, done: string) {
    try {
      await send(csrf, path, method, body);
      report(done);
      refresh();
      refreshMembers();
    } catch (error) {
      report(String(error));
    }
  }
  function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const parentId = source === "hr" ? String(form.get("parent") ?? "") : "";
    void apply("/api/v1/groups", "POST", { name: form.get("name"), source, parentId: parentId || null }, t("access.groupCreated"));
    event.currentTarget.reset();
  }
  function addMember(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!group) return;
    const userId = String(new FormData(event.currentTarget).get("user") ?? "");
    if (group.source === "local")
      void apply(`/api/v1/groups/${group.id}/members`, "PUT", { userId, member: true }, t("access.memberAdded"));
    else void apply(`/api/v1/users/${encodeURIComponent(userId)}/primary-group`, "PUT", { groupId: group.id }, t("access.primarySet"));
  }
  function moveParent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!group) return;
    const parentId = String(new FormData(event.currentTarget).get("parent") ?? "");
    void apply(`/api/v1/groups/${group.id}/parent`, "PUT", { parentId: parentId || null }, t("access.parentChanged"));
  }

  return (
    <Panel title={t("access.groups")} actions={groups && <Badge>{t("access.groupCount", { count: groups.length })}</Badge>}>
      <form className="access-toolbar" onSubmit={create} aria-label={t("access.createGroup")}>
        <Field label={t("access.name")}>
          <input name="name" required maxLength={120} autoComplete="off" />
        </Field>
        <Field label={t("access.type")}>
          <select name="source" value={source} onChange={(e) => setSource(e.target.value as "hr" | "local")}>
            <option value="local">{t("access.sources.local")}</option>
            <option value="hr">{t("access.sources.hr")}</option>
          </select>
        </Field>
        <Field label={t("access.parent")}>
          <select name="parent" disabled={source !== "hr"}>
            <option value="">{t("access.none")}</option>
            {hrGroups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </Field>
        <Button type="submit" variant="primary">
          {t("access.create")}
        </Button>
      </form>
      <p className="ui-empty-inline access-hint">
        {t("access.groupHint")}
      </p>
      {groups === null ? (
        <Loading />
      ) : groups.length === 0 ? (
        <p className="ui-empty-inline">{t("access.noGroups")}</p>
      ) : (
        <div className="access-table-scroll" role="region" aria-label={t("access.groupList")} tabIndex={0}>
          <table className="access-table">
            <thead>
              <tr>
                <th scope="col">{t("access.name")}</th>
                <th scope="col">{t("access.type")}</th>
                <th scope="col">{t("access.parent")}</th>
                <th scope="col">{t("access.members")}</th>
                <th scope="col">{t("access.state")}</th>
                <th scope="col">
                  <span className="visually-hidden">{t("access.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {groups.map((g) => (
                <tr key={g.id} aria-current={g.id === selected ? "true" : undefined}>
                  <td>{g.name}</td>
                  <td>{t(`access.sources.${g.source}`)}</td>
                  <td>{groups.find((p) => p.id === g.parentId)?.name ?? "—"}</td>
                  <td>{g.members}</td>
                  <td>{g.active ? t("access.active") : <Badge tone="warning">{t("access.inactive")}</Badge>}</td>
                  <td className="access-actions">
                    <Button size="sm" onClick={() => setSelected(g.id === selected ? null : g.id)} aria-expanded={g.id === selected}>
                      {t("access.members")}
                    </Button>
                    <Button
                      size="sm"
                      onClick={() =>
                        void apply(`/api/v1/groups/${g.id}`, "PATCH", { active: !g.active }, t(g.active ? "access.groupDeactivated" : "access.groupActivated"))
                      }
                    >
                      {t(g.active ? "access.deactivate" : "access.activate")}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {group && (
        <section className="access-detail" aria-label={t("access.membersOf", { name: group.name })}>
          <h3>{t("access.membersOf", { name: group.name })}</h3>
          <form className="access-toolbar" onSubmit={addMember}>
            <Field label={t(group.source === "local" ? "access.userToAdd" : "access.userToMakePrimary")}>
              <select name="user" disabled={users === null}>
                {(users ?? []).map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.name}
                  </option>
                ))}
              </select>
            </Field>
            <Button type="submit">{t(group.source === "local" ? "access.add" : "access.setPrimary")}</Button>
          </form>
          {group.source === "hr" && (
            <form key={`${group.id}:${group.parentId ?? ""}`} className="access-toolbar" onSubmit={moveParent}>
              <Field label={t("access.parent")}>
                <select name="parent" defaultValue={group.parentId ?? ""}>
                  <option value="">{t("access.none")}</option>
                  {hrGroups
                    .filter((g) => g.id !== group.id)
                    .map((g) => (
                      <option key={g.id} value={g.id}>
                        {g.name}
                      </option>
                    ))}
                </select>
              </Field>
              <Button type="submit">{t("access.changeParent")}</Button>
            </form>
          )}
          {members === null ? (
            <Loading />
          ) : members.length === 0 ? (
            <p className="ui-empty-inline">{t("access.noMembers")}</p>
          ) : (
            <ul className="access-members">
              {members.map((m) => (
                <li key={m.id}>
                  <span>
                    {m.name} {!m.enabled && <Badge tone="warning">{t("access.userDisabled")}</Badge>}
                  </span>
                  <Button
                    size="sm"
                    onClick={() =>
                      group.source === "local"
                        ? void apply(`/api/v1/groups/${group.id}/members`, "PUT", { userId: m.id, member: false }, t("access.memberRemoved"))
                        : void apply(`/api/v1/users/${encodeURIComponent(m.id)}/primary-group`, "PUT", { groupId: null }, t("access.primaryCleared"))
                    }
                  >
                    {t(group.source === "local" ? "access.remove" : "access.clearPrimary")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </Panel>
  );
}

export function AppAccessAdmin({ csrf, users, report, directory }: AdminAccessProps) {
  const { t } = useTranslation("console");
  const { groups } = directory;
  const [apps, setApps] = useState<{ id: string; name: string }[] | null>(null);
  const [appId, setAppId] = useState("");
  const { data: loadedAccess, refresh } = useSelectedResource<Access>(
    appId,
    appId ? `/api/v1/apps/${encodeURIComponent(appId)}/access` : null,
    report,
    t("access.appAccessLoadFailed"),
  );
  const access = loadedAccess?.appId === appId ? loadedAccess : null;
  const { confirm, dialog } = useConfirm();
  useEffect(() => {
    request<{ apps: { id: string; name: string }[] }>("/api/v1/apps?limit=200")
      .then((page) => setApps(page.apps))
      .catch((error) => report(t("access.appsLoadFailedWith", { error: String(error) })));
  }, [report, t]);
  async function apply(path: string, body: unknown, done: string) {
    try {
      await send(csrf, path, "PUT", body);
      report(done);
      refresh();
    } catch (error) {
      report(String(error));
    }
  }
  const base = `/api/v1/apps/${encodeURIComponent(appId)}`;
  async function toggleCompanySwitch() {
    if (!access) return;
    if (!access.disabled) {
      const ok = await confirm({
        title: t("access.disableAppTitle"),
        body: <p>{t("access.disableAppBody")}</p>,
        confirmLabel: t("access.disableApp"),
        tone: "danger",
      });
      if (!ok) return;
    }
    await apply(`${base}/control`, { disabled: !access.disabled }, t(access.disabled ? "access.appEnabled" : "access.appDisabled"));
  }
  function addGroupGrant(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    void apply(
      "/api/v1/group-grants",
      {
        groupId: form.get("group"),
        appId,
        environment: form.get("environment"),
        permission: form.get("permission"),
        enabled: true,
      },
      t("access.groupGranted"),
    );
  }
  const groupName = (id: string) => groups?.find((g) => g.id === id)?.name ?? id;
  const userName = (id: string) => users?.find((u) => u.id === id)?.name ?? id;

  return (
    <Panel title={t("access.appAccess")}>
      {dialog}
      <Field label={t("access.app")}>
        <select value={appId} onChange={(e) => setAppId(e.target.value)} disabled={apps === null}>
          <option value="">{t("access.chooseApp")}</option>
          {(apps ?? []).map((a) => (
            <option key={a.id} value={a.id}>
              {a.name} ({a.id})
            </option>
          ))}
        </select>
      </Field>
      {appId && access === null && <Loading />}
      {access && (
        <div className="access-detail">
          <div className="access-row">
            <span>
              {t("access.companyUse")} {access.disabled ? <Badge tone="danger">{t("access.stopped")}</Badge> : <Badge tone="success">{t("access.inUse")}</Badge>}
            </span>
            <Button size="sm" variant={access.disabled ? "primary" : "danger"} onClick={() => void toggleCompanySwitch()}>
              {t(access.disabled ? "access.enableApp" : "access.disableApp")}
            </Button>
          </div>
          {(["dev", "prod"] as const).map((environment) => (
            <div key={environment} className="access-row">
              <span>{t("access.admission", { environment: t(`environments.${environment}`) })}</span>
              <select
                aria-label={t("access.admission", { environment: t(`environments.${environment}`) })}
                value={access.admission[environment]}
                onChange={(e) =>
                  void apply(`${base}/admission`, { environment, mode: e.target.value }, t("access.admissionChanged", { environment: t(`environments.${environment}`) }))
                }
              >
                <option value="selected">{t("access.admissionSelected")}</option>
                <option value="all">{t("access.admissionAll")}</option>
              </select>
            </div>
          ))}
          <h3>{t("access.groupGrants")}</h3>
          <form className="access-toolbar" onSubmit={addGroupGrant}>
            <Field label={t("access.group")}>
              <select name="group" disabled={!groups?.length}>
                {(groups ?? [])
                  .filter((g) => g.active)
                  .map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.name}
                    </option>
                  ))}
              </select>
            </Field>
            <Field label={t("admin.environment")}>
              <select name="environment">
                <option value="dev">{t("environments.dev")}</option>
                <option value="prod">{t("environments.prod")}</option>
              </select>
            </Field>
            <Field label={t("admin.permission")} hint={t("access.deployPerPerson")}>
              <select name="permission">
                <option value="use">{t("access.permissions.use")}</option>
                <option value="develop">{t("access.permissions.develop")}</option>
              </select>
            </Field>
            <Button type="submit" disabled={!groups?.length}>
              {t("admin.grant")}
            </Button>
          </form>
          {access.groupGrants.length === 0 ? (
            <p className="ui-empty-inline">{t("access.noGroupGrants")}</p>
          ) : (
            <ul className="access-members">
              {access.groupGrants.map((g) => (
                <li key={`${g.groupId}-${g.environment}-${g.permission}`}>
                  <span>
                    {groupName(g.groupId)} · {t(`environments.${g.environment}`)} · {t(`access.permissions.${g.permission}`)}
                  </span>
                  <Button size="sm" onClick={() => void apply("/api/v1/group-grants", { ...g, appId, enabled: false }, t("access.groupRevoked"))}>
                    {t("admin.revoke")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <h3>{t("access.userGrants")}</h3>
          {access.userGrants.length === 0 ? (
            <p className="ui-empty-inline">{t("access.noUserGrants")}</p>
          ) : (
            <ul className="access-members">
              {access.userGrants.map((g) => (
                <li key={`${g.userId}-${g.environment}-${g.permission}`}>
                  <span>
                    {userName(g.userId)} · {t(`environments.${g.environment}`)} · {t(`access.permissions.${g.permission}`)}
                  </span>
                  <Button
                    size="sm"
                    onClick={() =>
                      void apply(
                        "/api/v1/grants",
                        {
                          userId: g.userId,
                          appId,
                          environment: g.environment,
                          permission: g.permission,
                          enabled: false,
                        },
                        t("access.userRevoked"),
                      )
                    }
                  >
                    {t("admin.revoke")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </Panel>
  );
}
