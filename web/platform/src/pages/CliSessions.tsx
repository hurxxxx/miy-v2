import { useCallback, useEffect, useState } from "react";
import { request } from "@miy/sdk";
import { useUser } from "../data/session";
import { LoadError } from "../components/LoadState";
import { Button, Loading, Panel } from "../ui";

type Session = {
  id: string;
  createdAt: string;
  expiresAt: string;
  clientName?: string;
  lastUsedAt?: string;
  revokedAt?: string | null;
};
type SessionPage = { sessions: Session[]; nextCursor?: string | null };

async function readSessions(path: string) {
  const value = await request<SessionPage>(path);
  if (!Array.isArray(value.sessions)) throw new Error("invalid_session_list");
  return value;
}

export function CliSessions() {
  return (
    <>
      <div className="page-header">
        <div>
          <h1>내 CLI 연결</h1>
          <p>
            직접 승인한 연결인지 확인하세요. 브라우저 로그아웃은 CLI 연결을
            철회하지 않습니다.
          </p>
        </div>
      </div>
      <SessionsList title="CLI 로그인 세션" path="/api/v1/cli/sessions" />
      <SessionsList
        title="기존 방식 CLI 연결"
        path="/api/v1/cli/legacy-sessions"
      />
    </>
  );
}

function SessionsList({ title, path }: { title: string; path: string }) {
  const user = useUser();
  const [data, setData] = useState<SessionPage>();
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState<string>();
  const [message, setMessage] = useState("");
  const load = useCallback(async () => {
    try {
      setData(await readSessions(path));
      setError(false);
    } catch {
      setError(true);
    }
  }, [path]);
  useEffect(() => {
    void load();
  }, [load]);
  async function more() {
    if (!data?.nextCursor) return;
    setBusy("more");
    try {
      const next = await readSessions(
        `${path}?cursor=${encodeURIComponent(data.nextCursor)}`,
      );
      setData({ ...next, sessions: [...data.sessions, ...next.sessions] });
      setError(false);
    } catch {
      setError(true);
    } finally {
      setBusy(undefined);
    }
  }
  async function revoke(id: string) {
    setBusy(id);
    setMessage("");
    try {
      await request(`${path}/${encodeURIComponent(id)}`, user.csrf, {
        method: "DELETE",
      });
      setMessage("CLI 연결을 철회했습니다.");
      await load();
    } catch {
      setMessage("연결을 철회하지 못했습니다. 다시 시도해 주세요.");
    } finally {
      setBusy(undefined);
    }
  }
  return (
    <Panel title={title}>
      {error && (
        <LoadError
          message="CLI 연결 목록을 불러오지 못했습니다."
          onRetry={() => void load()}
        />
      )}
      {!data && !error && <Loading />}
      {message && <p role="status">{message}</p>}
      {data?.sessions.length === 0 && <p>표시할 연결이 없습니다.</p>}
      {data?.sessions.map((session) => (
        <div className="registration-row" key={session.id}>
          <div>
            <strong>{session.clientName || "기존 CLI"}</strong>
            <p>
              연결 ID: <code>{session.id}</code>
            </p>
            <p>
              승인: {session.createdAt} · 만료: {session.expiresAt}
            </p>
            {session.lastUsedAt && <p>마지막 사용: {session.lastUsedAt}</p>}
          </div>
          {session.revokedAt ? (
            <p>철회됨</p>
          ) : new Date(session.expiresAt).getTime() <= Date.now() ? (
            <p>만료됨</p>
          ) : (
            <Button disabled={!!busy} onClick={() => void revoke(session.id)}>
              연결 철회
            </Button>
          )}
        </div>
      ))}
      {data?.nextCursor && (
        <Button disabled={!!busy} onClick={() => void more()}>
          연결 더 보기
        </Button>
      )}
    </Panel>
  );
}
