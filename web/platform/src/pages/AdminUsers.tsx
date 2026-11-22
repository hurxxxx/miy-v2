import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { request } from "@miy/sdk";
import { formatDateTime } from "../platform/time/time-utils";
import { Badge, Button, Field, Loading, Notice, Panel, useConfirm, type Tone } from "../ui";
import type { DirectoryUser } from "./AdminAccess";

// The identity provider only authenticates. A person can sign in once their account
// is registered here: by accepting an administrator's invitation link, or after an
// administrator registers an account that already exists in the identity provider.
// A registered person holds no app permission until one is granted below.
export type Account = DirectoryUser & { admin: boolean; canDevelop: boolean; enabled: boolean };

type Props = {
  csrf: string;
  selfId: string;
  users: Account[] | null;
  report: (message: string) => void;
  refresh: () => void;
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
] as const;

function failure(t: TFunction<"console">, error: unknown) {
  const text = String(error);
  const code = FAILURES.find((value) => text.includes(value));
  return code ? t(`users.failures.${code}`) : text;
}

function when(value: string) {
  return formatDateTime(value, { dateStyle: "short", timeStyle: "short" });
}

type UserFilter = "all" | "admin" | "developer" | "disabled";

/** A spreadsheet-safe CSV cell: quoted, and never read as a formula. */
function cell(value: string) {
  const text = /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
  return `"${text.replaceAll('"', '""')}"`;
}

function downloadCsv(t: TFunction<"console">, users: Account[]) {
  const header = ["id", "name", "admin", "canDevelop", "enabled"].map(cell).join(",");
  const rows = users.map((user) =>
    [user.id, user.name, String(user.admin), String(user.canDevelop), String(user.enabled)].map(cell).join(","),
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

export function UserAdmin({ csrf, selfId, users, report, refresh }: Props) {
  const { t } = useTranslation("console");
  const { confirm, dialog } = useConfirm();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<UserFilter>("all");
  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (users ?? []).filter(
      (user) =>
        (!needle || user.name.toLowerCase().includes(needle) || user.id.includes(needle)) &&
        (filter === "all" ||
          (filter === "admin" && user.admin) ||
          (filter === "developer" && user.canDevelop) ||
          (filter === "disabled" && !user.enabled)),
    );
  }, [users, query, filter]);
  const [busy, setBusy] = useState(false);
  const [links, setLinks] = useState<AccountLink[] | null>(null);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [issued, setIssued] = useState<Issued | null>(null);
  const [copied, setCopied] = useState("");
  const issuedBox = useRef<HTMLDivElement>(null);

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

  async function resetPassword(user: Account) {
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

  async function setAdmin(user: Account) {
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

  async function setEnabled(user: Account) {
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

  const active = (link: AccountLink) => revocable(link) || link.state === "resetting";
  const recent = (links ?? []).filter(active).concat((links ?? []).filter((link) => !active(link)));
  return (
    <>
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
      <Panel
        title={t("users.accounts")}
        actions={
          users && (
            <>
              <Badge tone="neutral">{t("users.people", { count: users.length })}</Badge>
              <Button size="sm" onClick={() => downloadCsv(t, shown)} disabled={shown.length === 0}>
                {t("users.exportCsv")}
              </Button>
            </>
          )
        }
      >
        {users === null ? (
          <Loading />
        ) : (
          <>
          <div className="user-filters" role="search">
            <input
              type="search"
              aria-label={t("users.search")}
              placeholder={t("users.searchPlaceholder")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            <select aria-label={t("users.filter")} value={filter} onChange={(event) => setFilter(event.target.value as UserFilter)}>
              <option value="all">{t("users.filters.all")}</option>
              <option value="admin">{t("users.filters.admin")}</option>
              <option value="developer">{t("users.filters.developer")}</option>
              <option value="disabled">{t("users.filters.disabled")}</option>
            </select>
            {shown.length !== users.length && <span role="status">{t("users.shown", { count: shown.length })}</span>}
          </div>
          <ul className="user-accounts">
            {shown.map((user) => (
              <li key={user.id} className="user-account">
                <div className="user-account-name">
                  <strong>{user.name}</strong>
                  <code>{user.id}</code>
                </div>
                <div className="user-account-badges">
                  {user.admin && <Badge tone="info">{t("users.admin")}</Badge>}
                  {user.canDevelop && <Badge tone="neutral">{t("users.developer")}</Badge>}
                  {!user.enabled && <Badge tone="danger">{t("users.disabledBadge")}</Badge>}
                </div>
                <div className="user-account-actions">
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
                  <Button size="sm" disabled={busy} onClick={() => void setEnabled(user)}>
                    {t(user.enabled ? "users.disable" : "users.enable")}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
          </>
        )}
      </Panel>
      {dialog}
    </>
  );
}
