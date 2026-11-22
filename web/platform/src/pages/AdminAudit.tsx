import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { formatDateTime } from "../platform/time/time-utils";
import { request, type AuditEntry as Entry, type AuditPage as Page } from "@miy/sdk";
import { Button, Field, Loading, Panel } from "../ui";
import type { DirectoryUser } from "./AdminAccess";

type Filter = { actor: string; action: string; from: string; to: string };

const actionKinds = [
  { value: "", label: "all" },
  { value: "group", label: "group" },
  { value: "grant.", label: "grant" },
  { value: "user.", label: "user" },
  { value: "app.", label: "app" },
];
// Action IDs as translation keys: the dots of an ID become underscores.
const KNOWN_ACTIONS = new Set([
  "group.created",
  "group.renamed",
  "group.deactivated",
  "group.activated",
  "group.parent_set",
  "group.member_added",
  "group.member_removed",
  "user.primary_group_set",
  "user.disabled",
  "user.enabled",
  "user.registered",
  "user.renamed",
  "user.admin.enabled",
  "user.admin.disabled",
  "user.developer.enabled",
  "user.developer.disabled",
  "group_grant.enabled",
  "group_grant.disabled",
  "grant.enabled",
  "grant.disabled",
  "app.admission_set",
  "app.disabled",
  "app.enabled",
  "user.profile.rename",
]);

function describe(t: TFunction<"console">, action: string) {
  const request = action.match(/^([A-Z]+):(\d{3})$/);
  if (request) return t("audit.request", { method: request[1], status: request[2] });
  return KNOWN_ACTIONS.has(action) ? t(`audit.actions.${action.replaceAll(".", "_")}`) : action;
}

function query(filter: Filter, cursor: number | null) {
  const params = new URLSearchParams({ limit: "50" });
  if (filter.actor) params.set("actor", filter.actor);
  if (filter.action) params.set("action", filter.action);
  // Dates are local calendar days; the end day is included.
  if (filter.from) params.set("from", new Date(`${filter.from}T00:00:00`).toISOString());
  if (filter.to) {
    const end = new Date(`${filter.to}T00:00:00`);
    end.setDate(end.getDate() + 1);
    params.set("to", end.toISOString());
  }
  if (cursor !== null) params.set("cursor", String(cursor));
  return `/api/v1/audit?${params}`;
}

export function AuditLog({ users, report }: { users: DirectoryUser[] | null; report: (message: string) => void }) {
  const { t } = useTranslation("console");
  const [result, setResult] = useState<(Page & { filter: Filter }) | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const sequence = useRef(0);
  const load = useCallback(
    async (current: Filter, cursor: number | null) => {
      const requestId = ++sequence.current;
      setBusy(true);
      setFailed(false);
      if (cursor === null) setResult(null);
      try {
        const page = await request<Page>(query(current, cursor));
        if (requestId !== sequence.current) return;
        setResult((previous) => ({
          ...page,
          filter: current,
          entries: cursor === null ? page.entries : [...(previous?.entries ?? []), ...page.entries],
        }));
      } catch (error) {
        if (requestId !== sequence.current) return;
        setFailed(true);
        report(t("audit.loadFailedWith", { error: String(error) }));
      } finally {
        if (requestId === sequence.current) setBusy(false);
      }
    },
    [report, t],
  );
  useEffect(() => {
    void load({ actor: "", action: "", from: "", to: "" }, null);
    return () => {
      sequence.current++;
    };
  }, [load]);
  function apply(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const current = {
      actor: String(form.get("actor") ?? ""),
      action: String(form.get("action") ?? ""),
      from: String(form.get("from") ?? ""),
      to: String(form.get("to") ?? ""),
    };
    void load(current, null);
  }
  const entries = result?.entries ?? null;
  return (
    <Panel title={t("audit.title")}>
      <form className="access-toolbar" onSubmit={apply} aria-label={t("audit.conditions")}>
        <Field label={t("audit.actor")}>
          <select name="actor" defaultValue="">
            <option value="">{t("audit.kinds.all")}</option>
            {(users ?? []).map((u) => (
              <option key={u.id} value={u.id}>
                {u.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("audit.action")}>
          <select name="action" defaultValue="">
            {actionKinds.map((k) => (
              <option key={k.value} value={k.value}>
                {t(`audit.kinds.${k.label}`)}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("audit.from")}>
          <input type="date" name="from" />
        </Field>
        <Field label={t("audit.to")}>
          <input type="date" name="to" />
        </Field>
        <Button type="submit" variant="primary" disabled={busy}>
          {t("audit.search")}
        </Button>
      </form>
      {entries === null ? (
        failed ? (
          <p className="ui-empty-inline">{t("audit.loadFailed")}</p>
        ) : (
          <Loading />
        )
      ) : entries.length === 0 ? (
        <p className="ui-empty-inline">{t("audit.empty")}</p>
      ) : (
        <div className="access-table-scroll" role="region" aria-label={t("audit.list")} tabIndex={0}>
          <table className="access-table">
            <thead>
              <tr>
                <th scope="col">{t("audit.at")}</th>
                <th scope="col">{t("audit.actor")}</th>
                <th scope="col">{t("audit.action")}</th>
                <th scope="col">{t("audit.target")}</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <tr key={e.id}>
                  <td>{formatDateTime(e.at, { dateStyle: "medium", timeStyle: "medium" })}</td>
                  <td>{e.actorName ?? e.actor ?? t("audit.system")}</td>
                  <td>{describe(t, e.action)}</td>
                  <td>
                    <code>{e.target ?? "—"}</code>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {result && result.nextCursor !== null && (
        <div className="grant-form-actions">
          <Button onClick={() => void load(result.filter, result.nextCursor)} disabled={busy}>
            {t("audit.more")}
          </Button>
        </div>
      )}
    </Panel>
  );
}
