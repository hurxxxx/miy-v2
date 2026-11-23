import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { useLocation, useNavigate } from "@tanstack/react-router";
import { request, type Group } from "@miy/sdk";
import { usePrompt } from "@miy/ui";
import { formatDateTime } from "../platform/time/time-utils";
import { Badge, Button, Field, Loading, Notice, Panel, useConfirm, type Tone } from "../ui";
import { AdminOrganizationTree } from "./AdminOrganizationTree";
import { fetchAdminUsers, type AdminUser, type AdminUserFilters } from "./admin-users-directory";

type Props = {
  csrf: string;
  selfId: string;
  report: (message: string) => void;
};

type LinkKind = "invitation" | "password_reset";
type LinkState = "pending" | "completed" | "revoked" | "expired" | "inactive" | "resetting";
type AccountLink = {
  id: string;
  kind: LinkKind;
  name: string | null;
  canDevelop: boolean;
  userId: string | null;
  state: LinkState;
  createdByName: string;
  createdAt: string;
  expiresAt: string;
  completedAt: string | null;
};
type Issued = { kind: LinkKind; name: string | null; url: string; expiresAt: string };

const SUBJECT = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
const states: Record<LinkState, Tone> = {
  pending: "info",
  completed: "success",
  revoked: "neutral",
  expired: "neutral",
  inactive: "warning",
  resetting: "info",
};

const revocable = (link: AccountLink) => link.state === "pending" || link.state === "inactive";

const FAILURES = [
  "user_exists",
  "invalid_user_id",
  "invalid_user_name",
  "last_enabled_administrator",
  "account_links_unavailable",
  "account_link_limit",
  "account_link_not_pending",
  "password_reset_in_progress",
  "user_disabled",
  "invalid_account_link",
  "cannot_switch_to_self",
] as const;

function failure(t: TFunction<"console">, error: unknown) {
  const text = String(error);
  const code = FAILURES.find((value) => text.includes(value));
  return code ? t(`users.failures.${code}`) : text;
}

function when(value: string) {
  return formatDateTime(value, { dateStyle: "short", timeStyle: "short" });
}

type DirectoryPage = Awaited<ReturnType<typeof fetchAdminUsers>>;
type DirectoryResult = { key: string; page?: DirectoryPage; failed?: true };
const FIRST_PAGE = [null];

function readFilters(search: string): AdminUserFilters {
  const params = new URLSearchParams(search);
  const role = params.get("role");
  const status = params.get("status");
  const organization = params.get("organization") || undefined;
  return {
    q: params.get("q")?.trim().slice(0, 120) || undefined,
    role: role === "admin" || role === "developer" || role === "member" ? role : undefined,
    status: status === "enabled" || status === "disabled" ? status : undefined,
    organization,
    descendants: organization && organization !== "unassigned" && params.get("descendants") === "true" ? true : undefined,
  };
}

/** A spreadsheet-safe CSV cell: quoted, and never read as a formula. */
function cell(value: string) {
  const text = /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
  return `"${text.replaceAll('"', '""')}"`;
}

function downloadCsv(t: TFunction<"console">, users: AdminUser[]) {
  const header = ["id", "name", "admin", "canDevelop", "enabled", "primaryGroupId", "primaryGroupName"].map(cell).join(",");
  const rows = users.map((user) =>
    [user.id, user.name, String(user.admin), String(user.canDevelop), String(user.enabled), user.primaryGroupId ?? "", user.primaryGroupName ?? ""].map(cell).join(","),
  );
  // The byte order mark lets spreadsheet programs read Korean names as UTF-8.
  const blob = new Blob(["\ufeff" + [header, ...rows].join("\r\n") + "\r\n"], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = t("users.csvFileName");
  link.click();
  URL.revokeObjectURL(url);
}

export function UserAdmin({ csrf, selfId, report }: Props) {
  const { t } = useTranslation("console");
  const { confirm, dialog } = useConfirm();
  const { prompt, promptDialog } = usePrompt();
  const search = useLocation({ select: (location) => location.searchStr });
  const navigate = useNavigate();
  const filters = useMemo(() => readFilters(search), [search]);
  const [query, setQuery] = useState(filters.q ?? "");
  const [paging, setPaging] = useState<{ search: string; cursors: (string | null)[] } | null>(null);
  const [result, setResult] = useState<DirectoryResult | null>(null);
  const [revision, setRevision] = useState(0);
  const [organizations, setOrganizations] = useState<Group[] | null>(null);
  const [organizationsFailed, setOrganizationsFailed] = useState(false);
  const [exporting, setExporting] = useState(false);
  const exportRequest = useRef<AbortController | null>(null);
  const cursors = paging?.search === search ? paging.cursors : FIRST_PAGE;
  const cursor = cursors.at(-1) ?? null;
  const key = JSON.stringify([search, cursor, revision]);
  const visible = result?.key === key ? result : null;
  const page = visible?.page;
  const shown = page?.items ?? [];
  const refresh = () => setRevision((value) => value + 1);
  const [busy, setBusy] = useState(false);
  const [links, setLinks] = useState<AccountLink[] | null>(null);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [issued, setIssued] = useState<Issued | null>(null);
  const [copied, setCopied] = useState("");
  const issuedBox = useRef<HTMLDivElement>(null);

  useEffect(() => setQuery(filters.q ?? ""), [filters.q]);
  useEffect(() => () => exportRequest.current?.abort(), []);
  useEffect(() => {
    const controller = new AbortController();
    fetchAdminUsers(filters, { cursor, signal: controller.signal })
      .then((page) => { if (!controller.signal.aborted) setResult({ key, page }); })
      .catch(() => { if (!controller.signal.aborted) setResult({ key, failed: true }); });
    return () => controller.abort();
  }, [cursor, filters, key]);
  useEffect(() => {
    const controller = new AbortController();
    request<Group[]>("/api/v1/groups", undefined, { signal: controller.signal })
      .then((groups) => { if (!controller.signal.aborted) setOrganizations(groups); })
      .catch(() => { if (!controller.signal.aborted) setOrganizationsFailed(true); });
    return () => controller.abort();
  }, []);

  function updateFilters(change: Partial<AdminUserFilters>) {
    const next = { ...filters, ...change };
    setPaging(null);
    void navigate({ to: "/admin/users", search: next });
  }

  function selectOrganization(organization: string | undefined) {
    updateFilters({ organization, descendants: organization && organization !== "unassigned" ? filters.descendants : undefined });
  }

  async function exportDirectory() {
    if (exportRequest.current) return;
    const controller = new AbortController();
    exportRequest.current = controller;
    setExporting(true);
    try {
      const users = new Map<string, AdminUser>();
      let nextCursor: string | null = null;
      do {
        const result = await fetchAdminUsers(filters, { cursor: nextCursor, limit: 200, signal: controller.signal });
        for (const user of result.items) users.set(user.id, user);
        nextCursor = result.nextCursor;
      } while (nextCursor);
      if (!controller.signal.aborted) downloadCsv(t, [...users.values()]);
    } catch {
      if (!controller.signal.aborted) report(t("users.exportFailed"));
    } finally {
      exportRequest.current = null;
      if (!controller.signal.aborted) setExporting(false);
    }
  }

  const loadLinks = useCallback(async () => {
    try {
      const data = await request<{ available: boolean; links: AccountLink[] }>("/api/v1/account-links");
      setLinks(data.links);
      setAvailable(data.available);
    } catch {
      // An older server has no account links; the direct registration below still works.
      setLinks([]);
      setAvailable(false);
    }
  }, []);
  useEffect(() => {
    void loadLinks();
    const timer = setInterval(() => void loadLinks(), 15000);
    return () => clearInterval(timer);
  }, [loadLinks]);
  useEffect(() => {
    if (issued) issuedBox.current?.focus();
  }, [issued]);

  async function send(path: string, method: "POST" | "PATCH", body: unknown, done: string) {
    setBusy(true);
    try {
      await request(path, csrf, { method, body: JSON.stringify(body) });
      report(done);
      refresh();
      return true;
    } catch (error) {
      report(failure(t, error));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function issue(body: Record<string, unknown>, name: string | null, done: string) {
    setBusy(true);
    try {
      const link = await request<Issued>("/api/v1/account-links", csrf, { method: "POST", body: JSON.stringify(body) });
      setIssued({ kind: link.kind, name: link.name ?? name, url: link.url, expiresAt: link.expiresAt });
      setCopied("");
      report(done);
      void loadLinks();
      return true;
    } catch (error) {
      report(failure(t, error));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function invite(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const data = new FormData(form);
    const ok = await issue(
      {
        kind: "invitation",
        name: String(data.get("name")).trim(),
        canDevelop: data.get("canDevelop") === "true",
        expiresInHours: Number(data.get("expiresInHours")),
      },
      null,
      t("users.invitationIssued"),
    );
    if (ok) form.reset();
  }

  async function resetPassword(user: AdminUser) {
    const ok = await confirm({
      title: t("users.resetConfirmTitle"),
      body: (
        <p>
          <strong>{user.name}</strong> {t("users.resetConfirmBody")}
        </p>
      ),
      confirmLabel: t("users.createLink"),
      tone: "primary",
    });
    if (!ok) return;
    await issue(
      { kind: "password_reset", userId: user.id },
      user.name,
      t("users.resetIssued"),
    );
  }

  async function revoke(link: AccountLink) {
    setBusy(true);
    try {
      await request(`/api/v1/account-links/${encodeURIComponent(link.id)}/revoke`, csrf, { method: "POST" });
      report(t("users.linkRevoked"));
    } catch (error) {
      report(failure(t, error));
    } finally {
      setBusy(false);
      void loadLinks();
    }
  }

  async function copy() {
    if (!issued) return;
    try {
      await navigator.clipboard.writeText(issued.url);
      setCopied(t("users.copied"));
    } catch {
      setCopied(t("users.copyManually"));
    }
  }

  async function register(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const data = new FormData(form);
    const ok = await send(
      "/api/v1/users",
      "POST",
      {
        id: String(data.get("id")).trim(),
        name: String(data.get("name")).trim(),
        canDevelop: data.get("canDevelop") === "true",
      },
      t("users.registered"),
    );
    if (ok) form.reset();
  }

  async function rename(user: AdminUser) {
    const name = await prompt({
      title: t("users.renameTitle"),
      description: t("users.renameDescription"),
      placeholder: t("users.displayName"),
      defaultValue: user.name,
      submitLabel: t("common:actions.save"),
      cancelLabel: t("common:actions.cancel"),
    });
    if (name && name !== user.name) {
      await send(`/api/v1/users/${encodeURIComponent(user.id)}`, "PATCH", { name }, t("users.renamed"));
    }
  }

  async function setAdmin(user: AdminUser) {
    const next = !user.admin;
    const ok = await confirm({
      title: t(next ? "users.makeAdminTitle" : "users.removeAdminTitle"),
      body: next ? (
        <p>
          <strong>{user.name}</strong> {t("users.makeAdminBody")}
        </p>
      ) : (
        <p>
          <strong>{user.name}</strong> {t("users.removeAdminBody")}
          {user.id === selfId && ` ${t("users.thisIsYou")}`}
        </p>
      ),
      confirmLabel: t(next ? "users.makeAdmin" : "users.removeAdminConfirm"),
      tone: next ? "primary" : "danger",
    });
    if (!ok) return;
    await send(`/api/v1/users/${encodeURIComponent(user.id)}`, "PATCH", { admin: next }, t(next ? "users.madeAdmin" : "users.removedAdmin"));
  }

  async function setEnabled(user: AdminUser) {
    const next = !user.enabled;
    if (!next) {
      const ok = await confirm({
        title: t("users.disableTitle"),
        body: (
          <p>
            <strong>{user.name}</strong> {t("users.disableBody")}
          </p>
        ),
        confirmLabel: t("users.disable"),
        tone: "danger",
      });
      if (!ok) return;
    }
    await send(`/api/v1/users/${encodeURIComponent(user.id)}/state`, "POST", { enabled: next }, t(next ? "users.enabled" : "users.disabled"));
  }

  // User switch (docs/CORE-PERMISSIONS.md "사용자 전환"): a new tab signs in
  // to the chosen portal as this person with a one-time link.
  async function signInAs(user: AdminUser) {
    const choice = { environment: "prod" as "prod" | "dev" };
    const ok = await confirm({
      title: t("users.signInAsTitle", { name: user.name }),
      body: (
        <>
          <p>{t("users.signInAsBody")}</p>
          <fieldset className="switch-environment">
            <legend>{t("users.signInAsWhere")}</legend>
            {(["prod", "dev"] as const).map((environment) => (
              <label key={environment}>
                <input
                  type="radio"
                  name="switch-environment"
                  value={environment}
                  defaultChecked={environment === "prod"}
                  onChange={() => {
                    choice.environment = environment;
                  }}
                />{" "}
                {t(environment === "prod" ? "users.signInAsPortal" : "users.signInAsDevPortal")}
              </label>
            ))}
          </fieldset>
        </>
      ),
      confirmLabel: t("users.signInAsConfirm"),
      tone: "danger",
    });
    if (!ok) return;
    // Open the tab while the click still counts as the person's action; a
    // window opened after the request would be blocked as a popup.
    const tab = window.open("", "_blank");
    if (!tab) {
      report(t("users.popupBlocked"));
      return;
    }
    tab.opener = null;
    setBusy(true);
    try {
      const started = await request<{ url: string }>(
        `/api/v1/users/${encodeURIComponent(user.id)}/sign-in-as`,
        csrf,
        { method: "POST", body: JSON.stringify(choice) },
      );
      tab.location.replace(started.url);
      report(t("users.signInAsStarted", { name: user.name }));
    } catch (error) {
      tab.close();
      report(failure(t, error));
    } finally {
      setBusy(false);
    }
  }

  const active = (link: AccountLink) => revocable(link) || link.state === "resetting";
  const recent = (links ?? []).filter(active).concat((links ?? []).filter((link) => !active(link)));
  return (
    <>
      <div className="admin-users-directory">
        <Panel title={t("users.organizationTree")} className="admin-users-organizations">
          <div className="admin-org-scope" aria-label={t("users.organizationFilter")} role="group">
            <button className="admin-org-choice" type="button" aria-current={filters.organization === undefined || undefined} onClick={() => selectOrganization(undefined)}>
              {t("users.allOrganizations")}
            </button>
            <button className="admin-org-choice" type="button" aria-current={filters.organization === "unassigned" || undefined} onClick={() => selectOrganization("unassigned")}>
              {t("users.unassigned")}
            </button>
          </div>
          {organizations && (
            <AdminOrganizationTree groups={organizations} selected={filters.organization ?? null} onSelect={selectOrganization} />
          )}
          {!organizations && (organizationsFailed ? (
            <Notice role="alert" tone="warning">{t("users.organizationsLoadFailed")}</Notice>
          ) : <Loading />)}
          {filters.organization && filters.organization !== "unassigned" && (
            <label className="admin-org-descendants">
              <input type="checkbox" checked={filters.descendants ?? false} onChange={(event) => updateFilters({ descendants: event.target.checked || undefined })} />
              {t("users.includeDescendants")}
            </label>
          )}
        </Panel>
        <Panel
          title={t("users.accounts")}
          actions={
            <div className="admin-users-actions">
              <Button size="sm" onClick={() => void exportDirectory()} disabled={exporting || !page || (shown.length === 0 && cursors.length === 1)}>
                {t(exporting ? "users.exporting" : "users.exportCsv")}
              </Button>
              <a href="#invite-users" className="ui-button ui-button-primary ui-button-sm">{t("users.invite")}</a>
            </div>
          }
        >
          <form className="user-filters" role="search" onSubmit={(event) => {
            event.preventDefault();
            updateFilters({ q: query.trim() || undefined });
          }}>
            <input
              type="search"
              aria-label={t("users.search")}
              placeholder={t("users.searchPlaceholder")}
              value={query}
              maxLength={120}
              onChange={(event) => setQuery(event.target.value)}
            />
            <Button type="submit" size="sm">{t("users.submitSearch")}</Button>
            <select aria-label={t("users.roleFilter")} value={filters.role ?? ""} onChange={(event) => updateFilters({ role: (event.target.value || undefined) as AdminUserFilters["role"] })}>
              <option value="">{t("users.allRoles")}</option>
              <option value="admin">{t("users.filters.admin")}</option>
              <option value="developer">{t("users.filters.developer")}</option>
              <option value="member">{t("users.memberRole")}</option>
            </select>
            <select aria-label={t("users.statusFilter")} value={filters.status ?? ""} onChange={(event) => updateFilters({ status: (event.target.value || undefined) as AdminUserFilters["status"] })}>
              <option value="">{t("users.allStatuses")}</option>
              <option value="enabled">{t("users.enabledStatus")}</option>
              <option value="disabled">{t("users.filters.disabled")}</option>
            </select>
          </form>
          {visible?.failed && (
            <Notice role="alert" tone="danger" actions={<Button size="sm" onClick={refresh}>{t("users.retryDirectory")}</Button>}>
              {t("users.directoryLoadFailed")}
            </Notice>
          )}
          {!visible && <Loading />}
          {page && (
            <>
              {shown.length === 0 && <p className="admin-users-muted">{t("users.noMatchingUsers")}</p>}
              <ul className="user-accounts">
                {shown.map((user) => (
                  <li key={user.id} className="user-account">
                    <div className="user-account-name">
                      <strong>{user.name}</strong>
                      <code>{user.id}</code>
                      <span className="admin-users-muted">{user.primaryGroupName ?? t("users.unassigned")}</span>
                    </div>
                    <div className="user-account-badges">
                      {user.admin && <Badge tone="info">{t("users.admin")}</Badge>}
                      {user.canDevelop && <Badge tone="neutral">{t("users.developer")}</Badge>}
                      {!user.enabled && <Badge tone="danger">{t("users.disabledBadge")}</Badge>}
                    </div>
                    <div className="user-account-actions">
                      <Button size="sm" disabled={busy} onClick={() => void rename(user)}>{t("users.rename")}</Button>
                      <Button size="sm" disabled={busy} onClick={() => void setAdmin(user)}>
                        {t(user.admin ? "users.removeAdmin" : "users.makeAdmin")}
                      </Button>
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() =>
                          void send(
                            `/api/v1/users/${encodeURIComponent(user.id)}`,
                            "PATCH",
                            { canDevelop: !user.canDevelop },
                            t(user.canDevelop ? "users.developerRemoved" : "users.developerGranted"),
                          )
                        }
                      >
                        {t(user.canDevelop ? "users.removeDeveloper" : "users.allowDeveloper")}
                      </Button>
                      {available && user.enabled && (
                        <Button size="sm" disabled={busy} onClick={() => void resetPassword(user)}>
                          {t("users.resetLink")}
                        </Button>
                      )}
                      {user.enabled && user.id !== selfId && (
                        <Button size="sm" disabled={busy} onClick={() => void signInAs(user)}>
                          {t("users.signInAs")}
                        </Button>
                      )}
                      <Button size="sm" disabled={busy} onClick={() => void setEnabled(user)}>
                        {t(user.enabled ? "users.disable" : "users.enable")}
                      </Button>
                    </div>
                  </li>
                ))}
              </ul>
              <div className="admin-users-pagination" aria-label={t("users.directoryPagination")} role="group">
                <span aria-live="polite">{t("users.directoryPage", { page: cursors.length })} · {t("users.people", { count: shown.length })}</span>
                <Button
                  size="sm"
                  disabled={cursors.length <= 1}
                  onClick={() => setPaging({ search, cursors: cursors.slice(0, -1) })}
                >
                  {t("users.previousPage")}
                </Button>
                <Button
                  size="sm"
                  disabled={!page.nextCursor}
                  onClick={() => setPaging({ search, cursors: [...cursors, page.nextCursor] })}
                >
                  {t("users.nextPage")}
                </Button>
              </div>
            </>
          )}
        </Panel>
      </div>
      <div id="invite-users">
        <Panel
          title={t("users.invite")}
          className="account-links"
          actions={links && <Badge tone="neutral">{t("users.unusedLinks", { count: links.filter((link) => link.state === "pending").length })}</Badge>}
        >
          {available === null ? (
            <Loading />
          ) : available ? (
            <form className="grant-form user-form" onSubmit={invite}>
              <Field label={t("users.name")} hint={t("users.nameHint")}>
                <input name="name" required maxLength={120} autoComplete="off" />
              </Field>
              <Field label={t("users.appRequests")}>
                <select name="canDevelop" defaultValue="false">
                  <option value="false">{t("users.notAllowed")}</option>
                  <option value="true">{t("users.allowed")}</option>
                </select>
              </Field>
              <Field label={t("users.linkLifetime")}>
                <select name="expiresInHours" defaultValue="168">
                  <option value="24">{t("users.days", { count: 1 })}</option>
                  <option value="72">{t("users.days", { count: 3 })}</option>
                  <option value="168">{t("users.days", { count: 7 })}</option>
                  <option value="720">{t("users.days", { count: 30 })}</option>
                </select>
              </Field>
              <div className="grant-form-actions">
                <Button type="submit" variant="primary" disabled={busy}>
                  {t("users.createInvitation")}
                </Button>
              </div>
            </form>
          ) : (
            <Notice tone="warning">{t("users.linksUnavailable")}</Notice>
          )}
          {issued && (
            <div className="account-link-issued" role="group" aria-label={t("users.issuedLink")} tabIndex={-1} ref={issuedBox}>
              <p>
                {t(issued.kind === "invitation" ? "users.issuedInvitation" : "users.issuedReset", {
                  name: issued.name ?? t("users.thisUser"),
                  until: when(issued.expiresAt),
                })}
              </p>
              <pre aria-label={t("users.linkAddress")}>
                <code>{issued.url}</code>
              </pre>
              <p className="account-link-warning">
                {t(issued.kind === "invitation" ? "users.invitationWarning" : "users.resetWarning")}
              </p>
              <div className="account-link-issued-actions">
                <Button variant="primary" onClick={copy}>
                  {t("users.copyAddress")}
                </Button>
                <Button onClick={() => setIssued(null)}>{t("users.close")}</Button>
                {copied && <span role="status">{copied}</span>}
              </div>
            </div>
          )}
          {recent.length > 0 && (
            <ul className="user-accounts account-link-list" aria-label={t("users.issuedLinks")}>
              {recent.map((link) => (
                <li key={link.id} className="user-account">
                  <div className="user-account-name">
                    <strong>{link.name ?? t("users.noName")}</strong>
                    <span>
                      {t(link.kind === "invitation" ? "users.kindInvitation" : "users.kindReset")} ·{" "}
                      {t("users.issuedBy", { name: link.createdByName })} ·{" "}
                      {link.state === "completed" && link.completedAt
                        ? t("users.usedAt", { when: when(link.completedAt) })
                        : t("users.expiresAt", { when: when(link.expiresAt) })}
                    </span>
                  </div>
                  <div className="user-account-badges">
                    <Badge tone={states[link.state]}>{t(`users.linkStates.${link.state}`)}</Badge>
                    {link.kind === "invitation" && link.canDevelop && <Badge tone="neutral">{t("users.developer")}</Badge>}
                  </div>
                  {revocable(link) && (
                    <div className="user-account-actions">
                      <Button size="sm" disabled={busy} onClick={() => void revoke(link)}>
                        {t("users.revokeLink")}
                      </Button>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
      <Panel title={t("users.directRegistration")}>
        <form className="grant-form user-form" onSubmit={register}>
          <Field label={t("users.loginId")} hint={t("users.loginIdHint")}>
            <input name="id" required pattern={SUBJECT} autoComplete="off" spellCheck={false} placeholder="00000000-0000-0000-0000-000000000000" />
          </Field>
          <Field label={t("users.displayName")}>
            <input name="name" required maxLength={120} autoComplete="off" />
          </Field>
          <Field label={t("users.appRequests")}>
            <select name="canDevelop" defaultValue="false">
              <option value="false">{t("users.notAllowed")}</option>
              <option value="true">{t("users.allowed")}</option>
            </select>
          </Field>
          <div className="grant-form-actions">
            <Button type="submit" variant="primary" disabled={busy}>
              {t("users.register")}
            </Button>
          </div>
        </form>
      </Panel>
      {dialog}
      {promptDialog}
    </>
  );
}
