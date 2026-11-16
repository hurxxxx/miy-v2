import { useRef, useState, type FormEvent } from "react";
import { request } from "@miy/sdk";
import { Badge, Button, Field, Panel } from "../ui";

type Report = {
  id: string; status: string; note: string; revision: number; createdAt: string;
  report: { message: string; diagnostics: Record<string, unknown> };
};
type Page = { entries: Report[]; nextCursor: number | null };
const statuses = { open: "접수", investigating: "조사 중", resolved: "해결", closed: "종료" };

export function SupportReports({ csrf }: { csrf: string }) {
  const [page, setPage] = useState<Page | null>(null);
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const sequence = useRef(0);

  async function load(status: string, before?: number) {
    const id = ++sequence.current;
    setBusy(true);
    setMessage("");
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    if (before) params.set("before", String(before));
    try {
      const result = await request<Page>(`/api/v1/support/reports?${params}`);
      if (id !== sequence.current) return;
      setPage((previous) => ({ ...result, entries: before ? [...(previous?.entries ?? []), ...result.entries] : result.entries }));
    } catch {
      if (id === sequence.current) setMessage("신고를 불러오지 못했습니다. 다시 조회하세요.");
    } finally {
      if (id === sequence.current) setBusy(false);
    }
  }

  async function update(event: FormEvent<HTMLFormElement>, report: Report) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setBusy(true);
    setMessage("");
    try {
      const result = await request<Report>(`/api/v1/support/reports/${report.id}`, csrf, {
        method: "PATCH", body: JSON.stringify({ status: form.get("status"), note: form.get("note"), revision: report.revision }),
      });
      setPage((previous) => previous && ({ ...previous, entries: previous.entries.map((entry) => entry.id === result.id ? result : entry) }));
      setMessage("처리 상태와 메모를 저장했습니다.");
    } catch (error) {
      setMessage(String(error).includes("report_changed_or_missing")
        ? "다른 관리자가 이 신고를 변경했습니다. 작성한 메모를 보관하고 다시 조회하세요."
        : "저장하지 못했습니다. 권한과 연결 상태를 확인하세요.");
    } finally { setBusy(false); }
  }

  return <Panel title="고장 신고">
    <p>CLI에서 보낸 비로그인 신고입니다. 신고 내용과 진단 정보는 제출자가 제공한 정보입니다.</p>
    <form className="access-toolbar" onSubmit={(event) => { event.preventDefault(); void load(filter); }} aria-label="고장 신고 조회">
      <Field label="처리 상태"><select value={filter} disabled={busy} onChange={(event) => { setFilter(event.target.value); setPage(null); }}>
        <option value="">전체</option>
        {Object.entries(statuses).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></Field>
      <Button type="submit" disabled={busy}>신고 조회</Button>
    </form>
    {message && <p role="status">{message}</p>}
    {page?.entries.length === 0 && <p>조건에 맞는 신고가 없습니다.</p>}
    {page?.entries.map((entry) => <details key={`${entry.id}:${entry.revision}`} className="support-report">
      <summary><Badge>{statuses[entry.status as keyof typeof statuses]}</Badge> {new Date(entry.createdAt).toLocaleString("ko-KR")} · {entry.report.message.slice(0, 80)}</summary>
      <p>신고 번호: <code>{entry.id}</code></p>
      <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{entry.report.message}</p>
      <details><summary>진단 정보</summary><pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{JSON.stringify(entry.report.diagnostics, null, 2)}</pre></details>
      <form className="grant-form" onSubmit={(event) => void update(event, entry)} aria-label={`신고 처리 ${entry.id}`}>
        <Field label="처리 상태"><select name="status" defaultValue={entry.status} disabled={busy}>
          {Object.entries(statuses).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
        </select></Field>
        <Field label="조사·해결 메모"><textarea name="note" maxLength={4000} rows={4} defaultValue={entry.note} disabled={busy} /></Field>
        <Button type="submit" disabled={busy}>처리 저장</Button>
      </form>
    </details>)}
    {page?.nextCursor && <Button disabled={busy} onClick={() => void load(filter, page.nextCursor!)}>이전 신고 더 보기</Button>}
  </Panel>;
}
