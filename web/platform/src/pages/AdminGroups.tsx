import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Link, useLocation, useNavigate, useRouter } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { isApiError, request, type Group } from "@miy/sdk";
import { DirectoryPicker, type DirectoryOption, type DirectoryProvider } from "@miy/ui";
import { Badge, Button, EmptyState, Field, Loading, Notice, Panel, useConfirm } from "../ui";
import { AdminOrganizationTree } from "./AdminOrganizationTree";
import { fetchAdminUsers } from "./admin-users-directory";

type Props = { csrf: string; report: (message: string) => void };
type Member = { id: string; name: string; enabled: boolean };
type Grant = { groupId: string; appId: string; environment: "dev" | "prod"; permission: "use" | "develop" };
type Resource<T> = { path: string; data: T; error?: never } | { path: string; error: true; data?: never };

function useResource<T>(path: string | null, revision: number) {
  const [resource, setResource] = useState<Resource<T>>();
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!path) return;
    const controller = new AbortController();
    void request<T>(path, undefined, { signal: controller.signal }).then(
      (data) => {
        if (!controller.signal.aborted) setResource({ path, data });
      },
      () => {
        if (!controller.signal.aborted) setResource({ path, error: true });
      },
    );
    return () => controller.abort();
  }, [path, revision, attempt]);
  return {
    resource: resource?.path === path ? resource : undefined,
    retry: () => {
      setResource(undefined);
      setAttempt((value) => value + 1);
    },
  };
}

function write<T = unknown>(path: string, csrf: string, method: "POST" | "PUT" | "PATCH", body: unknown) {
  return request<T>(path, csrf, { method, body: JSON.stringify(body) });
}

function excludedParents(groups: Group[], id: string) {
  const children = new Map<string, string[]>();
  for (const group of groups) {
    if (group.parentId) {
      const siblings = children.get(group.parentId) ?? [];
      siblings.push(group.id);
      children.set(group.parentId, siblings);
    }
  }
  const seen = new Set<string>();
  const pending = [id];
  while (pending.length) {
    const current = pending.pop()!;
    if (seen.has(current)) continue;
    seen.add(current);
    pending.push(...(children.get(current) ?? []));
  }
  return seen;
}

export function OrganizationAdmin(props: Props) {
  return <GroupDirectory {...props} source="hr" />;
}

export function PermissionGroupsAdmin(props: Props) {
  return <GroupDirectory {...props} source="local" />;
}

type Mutate = <T>(action: () => Promise<T>) => Promise<T | undefined>;

function GroupDirectory({ csrf, report, source }: Props & { source: Group["source"] }) {
  const { t } = useTranslation("console");
  const { search } = useLocation();
  const navigate = useNavigate();
  const router = useRouter();
  const isOrg = source === "hr";
  const path = isOrg ? "/admin/organization" : "/admin/groups";
  const selected = typeof search.group === "string" ? search.group : null;
  const [creating, setCreating] = useState(false);
  const [groupSearch, setGroupSearch] = useState("");
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const { resource, retry } = useResource<Group[]>("/api/v1/groups", revision);
  const groups = resource?.data?.filter((group) => group.source === source) ?? [];
  const group = groups.find((item) => item.id === selected);
  const visible = groups.filter((item) => item.name.toLocaleLowerCase().includes(groupSearch.trim().toLocaleLowerCase()));

  const mutate: Mutate = async (action) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      return await action();
    } catch (error) {
      report(isApiError(error) && error.code === "primary_group_changed" ? t("groups.primaryChanged") : String(error));
      return undefined;
    } finally {
      pending.current = false;
      setBusy(false);
      setRevision((value) => value + 1);
    }
  };

  function select(id: string) {
    void navigate({ to: path, search: { group: id } as never });
  }

  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const created = await mutate(() => write<{ id: string }>("/api/v1/groups", csrf, "POST", {
      name: form.get("name"),
      source,
      parentId: form.get("parent") || null,
    }));
    if (!created || router.state.location.pathname !== path) return;
    setCreating(false);
    setGroupSearch("");
    report(t(isOrg ? "groups.organizationCreated" : "groups.created"));
    select(created.id);
  }

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[minmax(14rem,1fr)_minmax(0,2fr)]">
      <Panel title={t(isOrg ? "groups.tree" : "groups.title")} actions={
        <Button size="sm" variant="primary" disabled={!resource?.data || busy} onClick={() => setCreating(!creating)}>
          {t(creating ? "common:actions.cancel" : isOrg ? "groups.createOrganization" : "groups.createGroup")}
        </Button>
      }>
        {creating && (
          <form className="access-toolbar" onSubmit={create}>
            <Field label={t("groups.name")}><input name="name" required maxLength={120} autoComplete="off" /></Field>
            {isOrg && (
              <Field label={t("groups.parent")}>
                <select name="parent" defaultValue={group?.active ? group.id : ""}>
                  <option value="">{t("groups.noParent")}</option>
                  {groups.filter((item) => item.active).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
                </select>
              </Field>
            )}
            <Button type="submit" variant="primary" disabled={busy}>{t("groups.create")}</Button>
          </form>
        )}
        {!resource && <Loading />}
        {resource?.error && <ReadError label={t("groups.loadFailed")} onRetry={retry} />}
        {resource?.data && (groups.length === 0 ? (
          <EmptyState title={t(isOrg ? "groups.noOrganizations" : "groups.noGroups")} />
        ) : isOrg ? (
          <AdminOrganizationTree groups={groups} selected={selected} onSelect={select} />
        ) : (
          <>
            <Field label={t("groups.search")}><input type="search" value={groupSearch} onChange={(event) => setGroupSearch(event.target.value)} /></Field>
            {visible.length === 0 ? <EmptyState title={t("common:empty.noResults")} /> : (
              <ul className="access-members">
                {visible.map((item) => (
                  <li key={item.id}>
                    <Button variant="ghost" size="sm" aria-current={item.id === selected ? "true" : undefined} onClick={() => select(item.id)}>
                      {item.name}{!item.active && <Badge tone="warning">{t("groups.inactive")}</Badge>}
                    </Button>
                    <span className="text-sm text-ui-ink-muted">{t("groups.memberCount", { count: item.members })}</span>
                  </li>
                ))}
              </ul>
            )}
          </>
        ))}
      </Panel>
      {group ? (
        <GroupDetails key={group.id} {...{ csrf, report, group, groups, revision, busy, mutate }} />
      ) : resource?.data && (
        <Panel title={t("groups.details")}><EmptyState title={t(selected ? "groups.notFound" : isOrg ? "groups.selectOrganization" : "groups.selectGroup")} /></Panel>
      )}
    </div>
  );
}

function ReadError({ label, onRetry }: { label: string; onRetry: () => void }) {
  const { t } = useTranslation("console");
  return <Notice role="alert" tone="danger" actions={<Button size="sm" onClick={onRetry}>{t("common:actions.retry")}</Button>}>{label}</Notice>;
}

function GroupDetails({ csrf, report, group, groups, revision, busy, mutate }: Props & {
  group: Group; groups: Group[]; revision: number; busy: boolean; mutate: Mutate;
}) {
  const { t } = useTranslation("console");
  const { confirm, dialog } = useConfirm();
  const isOrg = group.source === "hr";
  const path = `/api/v1/groups/${encodeURIComponent(group.id)}`;
  const { resource: members, retry: retryMembers } = useResource<Member[]>(`${path}/members`, revision);
  const { resource: grants, retry: retryGrants } = useResource<Grant[]>("/api/v1/group-grants", revision);
  const [picks, setPicks] = useState<DirectoryOption[]>([]);
  const [failed, setFailed] = useState<DirectoryOption[]>([]);
  const [formState, setFormState] = useState<"name" | "parent" | null>(null);
  const memberIds = useMemo(() => new Set(members?.data?.map((item) => item.id)), [members?.data]);
  const provider = useMemo<DirectoryProvider>(() => ({
    async load({ query, limit, cursor, signal }) {
      const page = await fetchAdminUsers({ q: query }, { limit, cursor, signal });
      return {
        items: page.items.filter((user) => !memberIds.has(user.id)).map((user) => ({
          kind: "user",
          id: user.id,
          name: user.name,
          description: user.primaryGroupName ?? undefined,
        })),
        nextCursor: page.nextCursor,
      };
    },
  }), [memberIds]);
  const blockedParents = isOrg ? excludedParents(groups, group.id) : new Set<string>();
  const parent = groups.find((item) => item.id === group.parentId);
  const groupGrants = grants?.data?.filter((grant) => grant.groupId === group.id);

  async function rename(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = new FormData(event.currentTarget).get("name");
    if (await mutate(() => write(path, csrf, "PATCH", { name }))) {
      setFormState(null);
      report(t("groups.renamed"));
    }
  }

  async function move(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const parentId = new FormData(event.currentTarget).get("parent") || null;
    if (await mutate(() => write(`${path}/parent`, csrf, "PUT", { parentId }))) {
      setFormState(null);
      report(t("groups.parentChanged"));
    }
  }

  async function toggleActive() {
    if (group.active && !await confirm({
      title: t("groups.deactivateTitle", { name: group.name }),
      body: t("groups.deactivateBody"),
      confirmLabel: t("groups.deactivate"),
      tone: "danger",
    })) return;
    if (await mutate(() => write(path, csrf, "PATCH", { active: !group.active }))) {
      report(t(group.active ? "groups.deactivated" : "groups.activated"));
    }
  }

  async function addMembers(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const additions = picks.filter((user) => !memberIds.has(user.id));
    if (!additions.length) return;
    if (isOrg && !await confirm({
      title: t("groups.assignTitle", { name: group.name }),
      body: t("groups.assignBody"),
      confirmLabel: t("groups.setPrimary"),
    })) return;
    const failures = await mutate(async () => {
      const rejected: DirectoryOption[] = [];
      for (const user of additions) {
        try {
          if (isOrg) await write(`/api/v1/users/${encodeURIComponent(user.id)}/primary-group`, csrf, "PUT", { groupId: group.id });
          else await write(`${path}/members`, csrf, "PUT", { userId: user.id, member: true });
        } catch {
          rejected.push(user);
        }
      }
      return rejected;
    });
    if (!failures) return;
    setPicks(failures);
    setFailed(failures);
    const added = additions.length - failures.length;
    if (added) report(t(isOrg ? "groups.primarySet" : "groups.memberAdded", { count: added }));
  }

  async function removeMember(member: Member) {
    if (!await confirm({
      title: t(isOrg ? "groups.clearTitle" : "groups.removeTitle", { name: member.name }),
      body: t(isOrg ? "groups.clearBody" : "groups.removeBody"),
      confirmLabel: t(isOrg ? "groups.clearPrimary" : "groups.remove"),
      tone: "danger",
    })) return;
    const result = await mutate(() => isOrg
      ? write(`/api/v1/users/${encodeURIComponent(member.id)}/primary-group`, csrf, "PUT", {
        groupId: null,
        expectedGroupId: group.id,
      })
      : write(`${path}/members`, csrf, "PUT", { userId: member.id, member: false }));
    if (result) report(t(isOrg ? "groups.primaryCleared" : "groups.memberRemoved"));
  }

  return (
    <div className="min-w-0">
      <Panel title={group.name} actions={<Badge tone={group.active ? "success" : "warning"}>{t(group.active ? "groups.active" : "groups.inactive")}</Badge>}>
        {isOrg && <p>{t("groups.parent")}: {parent?.name ?? t("groups.noParent")}</p>}
        <div className="access-toolbar">
          <Button size="sm" disabled={busy} onClick={() => setFormState("name")}>{t("common:actions.rename")}</Button>
          {isOrg && <Button size="sm" disabled={busy} onClick={() => setFormState("parent")}>{t("groups.changeParent")}</Button>}
          <Button size="sm" disabled={busy} onClick={() => void toggleActive()}>{t(group.active ? "groups.deactivate" : "groups.activate")}</Button>
          {isOrg && <Link to="/admin/users" search={{ organization: group.id } as never}>{t("groups.browseUsers")}</Link>}
        </div>
        {formState === "name" && (
          <form className="access-toolbar" onSubmit={rename}>
            <Field label={t("groups.name")}><input name="name" defaultValue={group.name} required maxLength={120} /></Field>
            <Button type="submit" variant="primary" disabled={busy}>{t("common:actions.save")}</Button>
            <Button onClick={() => setFormState(null)}>{t("common:actions.cancel")}</Button>
          </form>
        )}
        {formState === "parent" && (
          <form className="access-toolbar" onSubmit={move}>
            <Field label={t("groups.parent")}>
              <select name="parent" defaultValue={group.parentId ?? ""}>
                <option value="">{t("groups.noParent")}</option>
                {groups.filter((item) => !blockedParents.has(item.id) && (item.active || item.id === group.parentId)).map((item) => (
                  <option key={item.id} value={item.id}>{item.name}</option>
                ))}
              </select>
            </Field>
            <Button type="submit" variant="primary" disabled={busy}>{t("common:actions.save")}</Button>
            <Button onClick={() => setFormState(null)}>{t("common:actions.cancel")}</Button>
          </form>
        )}
      </Panel>
      <Panel title={t("groups.membersOf", { name: group.name })}>
        <section aria-label={t("groups.membersOf", { name: group.name })}>
          {isOrg && <p className="ui-empty-inline access-hint">{t("groups.organizationHint")}</p>}
          {!members && <Loading />}
          {members?.error && <ReadError label={t("groups.membersLoadFailed")} onRetry={retryMembers} />}
          {members?.data && (
            <>
              {group.active && (
                <form className="access-toolbar" onSubmit={addMembers}>
                  <DirectoryPicker
                    className="min-w-0 flex-1"
                    label={t(isOrg ? "groups.userToMakePrimary" : "groups.userToAdd")}
                    provider={provider}
                    value={picks}
                    onChange={(value) => { setPicks(value); setFailed([]); }}
                    kinds={["user"]}
                    disabled={busy}
                    labels={{
                      users: t("groups.users"),
                      searchPlaceholder: t("groups.searchUsers"),
                      loading: t("groups.loadingUsers"),
                      noMatches: t("common:empty.noResults"),
                      unavailable: t("groups.usersLoadFailed"),
                      retry: t("common:actions.retry"),
                      loadMore: t("groups.moreUsers"),
                      remove: (name) => t("groups.undoSelection", { name }),
                    }}
                  />
                  <Button type="submit" variant="primary" disabled={busy || !picks.some((user) => !memberIds.has(user.id))}>
                    {t(isOrg ? "groups.setPrimary" : "common:actions.add")}
                  </Button>
                </form>
              )}
              {failed.length > 0 && <Notice role="alert" tone="danger">{t("groups.addFailed", { names: failed.map((user) => user.name).join(", ") })}</Notice>}
              {members.data.length === 0 ? <EmptyState title={t("groups.noMembers")} /> : (
                <ul className="access-members">
                  {members.data.map((member) => (
                    <li key={member.id}>
                      <span>{member.name} {!member.enabled && <Badge tone="warning">{t("groups.userDisabled")}</Badge>}</span>
                      <Button size="sm" disabled={busy} onClick={() => void removeMember(member)}>{t(isOrg ? "groups.clearPrimary" : "groups.remove")}</Button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </section>
      </Panel>
      <Panel title={t("groups.appGrants")}>
        {!grants && <Loading />}
        {grants?.error && <ReadError label={t("groups.grantsLoadFailed")} onRetry={retryGrants} />}
        {groupGrants && (groupGrants.length === 0 ? <EmptyState title={t("groups.noGrants")} /> : (
          <ul className="access-members">
            {groupGrants.map((grant) => (
              <li key={`${grant.appId}:${grant.environment}:${grant.permission}`}>
                <Link to={`/admin/apps/${encodeURIComponent(grant.appId)}/access` as "/"}>{grant.appId}</Link>
                <span>{t(`groups.environments.${grant.environment}`)} · {t(`access.permissions.${grant.permission}`)}</span>
              </li>
            ))}
          </ul>
        ))}
      </Panel>
      {dialog}
    </div>
  );
}
