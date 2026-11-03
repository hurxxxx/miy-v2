import { useCallback, useEffect, useState, type FormEvent } from "react";
import { request } from "@miy/sdk";
import { Button, Field, Loading, Panel } from "../ui";
import type { DirectoryUser } from "./AdminAccess";

type Entry = {
  id: number;
  at: string;
  actor: string | null;
  actorName: string | null;
  action: string;
  target: string | null;
  requestId: string;
};
type Page = { entries: Entry[]; nextCursor: number | null };
type Filter = { actor: string; action: string; from: string; to: string };

const actionKinds = [
  { value: "", label: "전체" },
  { value: "group", label: "그룹·그룹 권한" },
  { value: "user.", label: "주 소속" },
  { value: "app.", label: "앱 입장·사용 중지" },
];
const actionLabel: Record<string, string> = {
  "group.created": "그룹 만듦",
  "group.renamed": "그룹 이름 변경",
  "group.deactivated": "그룹 비활성화",
  "group.activated": "그룹 다시 사용",
  "group.parent_set": "상위 조직 변경",
  "group.member_added": "구성원 추가",
  "group.member_removed": "구성원 제외",
  "user.primary_group_set": "주 소속 지정",
  "group_grant.enabled": "그룹 권한 부여",
  "group_grant.disabled": "그룹 권한 회수",
  "app.admission_set": "입장 대상 변경",
  "app.disabled": "앱 사용 중지",
  "app.enabled": "앱 사용 재개",
};

function describe(action: string) {
  const request = action.match(/^([A-Z]+):(\d{3})$/);
  return request ? `요청 ${request[1]} · ${request[2]}` : (actionLabel[action] ?? action);
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
  const [filter, setFilter] = useState<Filter>({ actor: "", action: "", from: "", to: "" });
  const [entries, setEntries] = useState<Entry[] | null>(null);
  const [next, setNext] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(
    async (current: Filter, cursor: number | null) => {
      setBusy(true);
      try {
        const page = await request<Page>(query(current, cursor));
        setEntries((previous) => (cursor === null ? page.entries : [...(previous ?? []), ...page.entries]));
        setNext(page.nextCursor);
      } catch (error) {
        report(`감사 기록을 불러오지 못했습니다: ${String(error)}`);
      } finally {
        setBusy(false);
      }
    },
    [report],
  );
  useEffect(() => {
    void load({ actor: "", action: "", from: "", to: "" }, null);
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
    setFilter(current);
    void load(current, null);
  }
  return (
    <Panel title="감사 기록">
      <form className="access-toolbar" onSubmit={apply} aria-label="감사 기록 조건">
        <Field label="행위자">
          <select name="actor" defaultValue="">
            <option value="">전체</option>
            {(users ?? []).map((u) => (
              <option key={u.id} value={u.id}>
                {u.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="동작">
          <select name="action" defaultValue="">
            {actionKinds.map((k) => (
              <option key={k.value} value={k.value}>
                {k.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="시작일">
          <input type="date" name="from" />
        </Field>
        <Field label="종료일">
          <input type="date" name="to" />
        </Field>
        <Button type="submit" variant="primary" disabled={busy}>
          조회
        </Button>
      </form>
      {entries === null ? (
        <Loading />
      ) : entries.length === 0 ? (
        <p className="ui-empty-inline">조건에 맞는 기록이 없습니다.</p>
      ) : (
        <table className="access-table">
          <thead>
            <tr>
              <th scope="col">시각</th>
              <th scope="col">행위자</th>
              <th scope="col">동작</th>
              <th scope="col">대상</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.id}>
                <td>{new Date(e.at).toLocaleString("ko-KR")}</td>
                <td>{e.actorName ?? e.actor ?? "시스템"}</td>
                <td>{describe(e.action)}</td>
                <td>
                  <code>{e.target ?? "—"}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {next !== null && (
        <div className="grant-form-actions">
          <Button onClick={() => void load(filter, next)} disabled={busy}>
            더 보기
          </Button>
        </div>
      )}
    </Panel>
  );
}
