import { useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { formatDateTime } from "../platform/time/time-utils";
import { request } from "@miy/sdk";
import { Badge, Button, Field, Panel } from "../ui";

type Report = {
  id: string; status: string; note: string; result: string; revision: number; createdAt: string;
  report: { threadId?: string; message: string; diagnostics: Record<string, unknown> };
};
type Page = { entries: Report[]; nextCursor: number | null };
const STATUSES = ["open", "investigating", "resolved", "closed"] as const;

export function SupportReports({ csrf }: { csrf: string }) {
  const { t } = useTranslation("console");
  const status = (value: string) => ((STATUSES as readonly string[]).includes(value) ? t(`reports.statuses.${value}`) : value);
  const [page, setPage] = useState<Page | null>(null);
  const [filter, setFilter] = useState("");
  const [thread, setThread] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const sequence = useRef(0);

  async function load(status: string, before?: number) {
    const id = ++sequence.current;
    setBusy(true);
    setMessage("");
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    if (thread.trim()) params.set("threadId", thread.trim());
    if (before) params.set("before", String(before));
    try {
      const result = await request<Page>(`/api/v1/support/reports?${params}`);
      if (id !== sequence.current) return;
      setPage((previous) => ({ ...result, entries: before ? [...(previous?.entries ?? []), ...result.entries] : result.entries }));
    } catch {
      if (id === sequence.current) setMessage(t("reports.loadFailed"));
    } finally {
      if (id === sequence.current) setBusy(false);
    }
  }

  async function update(event: FormEvent<HTMLFormElement>, report: Report) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const publishedResult = String(form.get("result") ?? "");
    if (Array.from(publishedResult).length > 32000) {
      setMessage(t("reports.resultTooLong"));
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      const result = await request<Report>(`/api/v1/support/reports/${report.id}`, csrf, {
        method: "PATCH", body: JSON.stringify({ status: form.get("status"), note: form.get("note"), result: publishedResult, revision: report.revision }),
      });
      setPage((previous) => previous && ({ ...previous, entries: previous.entries.map((entry) => entry.id === result.id ? result : entry) }));
      setMessage(t("reports.saved"));
    } catch (error) {
      setMessage(String(error).includes("report_changed_or_missing")
        ? t("reports.changedElsewhere")
        : t("reports.saveFailed"));
    } finally { setBusy(false); }
  }

  return <Panel title={t("reports.title")}>
    <p>{t("reports.description")}</p>
    <form className="access-toolbar" onSubmit={(event) => { event.preventDefault(); void load(filter); }} aria-label={t("reports.search")}>
      <Field label={t("reports.status")}><select value={filter} disabled={busy} onChange={(event) => { setFilter(event.target.value); setPage(null); }}>
        <option value="">{t("reports.all")}</option>
        {STATUSES.map((value) => <option key={value} value={value}>{status(value)}</option>)}
      </select></Field>
      <Field label={t("reports.thread")}><input value={thread} onChange={e=>{setThread(e.target.value);setPage(null);}} /></Field>
      <Button type="submit" disabled={busy}>{t("reports.search")}</Button>
    </form>
    {message && <p role="status">{message}</p>}
    {page?.entries.length === 0 && <p>{t("reports.empty")}</p>}
    {page?.entries.map((entry) => <details key={`${entry.id}:${entry.revision}`} className="support-report">
      <summary><Badge>{status(entry.status)}</Badge> {formatDateTime(entry.createdAt, { dateStyle: "medium", timeStyle: "medium" })} · {entry.report.message.slice(0, 80)}</summary>
      <p>{t("reports.number")} <code>{entry.id}</code></p>
      {entry.report.threadId && <p>{t("reports.thread")} <code>{entry.report.threadId}</code></p>}
      <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{entry.report.message}</p>
      <details><summary>{t("reports.diagnostics")}</summary><pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{JSON.stringify(entry.report.diagnostics, null, 2)}</pre></details>
      <form className="grant-form" onSubmit={(event) => void update(event, entry)} aria-label={t("reports.handle", { id: entry.id })}>
        <Field label={t("reports.status")}><select name="status" defaultValue={entry.status} disabled={busy}>
          {STATUSES.map((value) => <option key={value} value={value}>{status(value)}</option>)}
        </select></Field>
        <Field label={t("reports.note")}><textarea className="ui-input" name="note" maxLength={4000} rows={4} defaultValue={entry.note} disabled={busy} /></Field>
        <Field label={t("reports.result")}><textarea className="ui-input" name="result" rows={4} defaultValue={entry.result} disabled={busy} /></Field>
        <Button type="submit" disabled={busy}>{t("reports.save")}</Button>
      </form>
    </details>)}
    {page?.nextCursor && <Button disabled={busy} onClick={() => void load(filter, page.nextCursor!)}>{t("reports.more")}</Button>}
  </Panel>;
}
