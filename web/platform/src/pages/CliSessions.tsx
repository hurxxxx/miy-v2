import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
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
  const { t } = useTranslation("portal");
  return (
    <>
      <div className="page-header">
        <div>
          <h1>{t("cliSessions.title")}</h1>
          <p>
            {t("cliSessions.lead")}
          </p>
        </div>
      </div>
      <SessionsList title={t("cliSessions.loginSessions")} path="/api/v1/cli/sessions" />
      <SessionsList
        title={t("cliSessions.legacySessions")}
        path="/api/v1/cli/legacy-sessions"
      />
    </>
  );
}

function SessionsList({ title, path }: { title: string; path: string }) {
  const { t } = useTranslation("portal");
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
      setMessage(t("cliSessions.revoked"));
      await load();
    } catch {
      setMessage(t("cliSessions.revokeFailed"));
    } finally {
      setBusy(undefined);
    }
  }
  return (
    <Panel title={title}>
      {error && (
        <LoadError
          message={t("cliSessions.loadFailed")}
          onRetry={() => void load()}
        />
      )}
      {!data && !error && <Loading />}
      {message && <p role="status">{message}</p>}
      {data?.sessions.length === 0 && <p>{t("cliSessions.empty")}</p>}
      {data?.sessions.map((session) => (
        <div className="registration-row" key={session.id}>
          <div>
            <strong>{session.clientName || t("cliSessions.legacyClient")}</strong>
            <p>
              {t("cliSessions.connectionId")} <code>{session.id}</code>
            </p>
            <p>
              {t("cliSessions.approvedExpires", { createdAt: session.createdAt, expiresAt: session.expiresAt })}
            </p>
            {session.lastUsedAt && <p>{t("cliSessions.lastUsed", { lastUsedAt: session.lastUsedAt })}</p>}
          </div>
          {session.revokedAt ? (
            <p>{t("cliSessions.revokedState")}</p>
          ) : new Date(session.expiresAt).getTime() <= Date.now() ? (
            <p>{t("cliSessions.expired")}</p>
          ) : (
            <Button disabled={!!busy} onClick={() => void revoke(session.id)}>
              {t("cliSessions.revoke")}
            </Button>
          )}
        </div>
      ))}
      {data?.nextCursor && (
        <Button disabled={!!busy} onClick={() => void more()}>
          {t("cliSessions.more")}
        </Button>
      )}
    </Panel>
  );
}
