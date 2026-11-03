import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Link } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { Button, Panel } from "@miy/ui";
import { useInstallationPolling, useInstallations } from "../data/installations";
import { useUser } from "../data/session";
import { environmentPath } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { RuntimeHealth } from "../components/RuntimeHealth";
import { RuntimeControls } from "../components/RuntimeControls";

type Registration = { id: string; appId: string; name: string; status: string; requester: string };
type DirectoryUser = { id: string; name: string };

export function Admin() {
  const user = useUser();
  if (!user.admin)
    return (
      <Panel title="관리자 권한이 필요합니다">
        <p>관리자로 로그인하세요.</p>
      </Panel>
    );
  return <AdminConsole csrf={user.csrf} />;
}

function AdminConsole({ csrf }: { csrf: string }) {
  const installations = useInstallations();
  useInstallationPolling();
  const [requests, setRequests] = useState<Registration[] | null>(null);
  const [users, setUsers] = useState<DirectoryUser[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [message, setMessage] = useState("");
  const loadDirectory = useCallback(async () => {
    try {
      const [nextRequests, nextUsers] = await Promise.all([
        request<Registration[]>("/api/v1/app-registrations"),
        request<DirectoryUser[]>("/api/v1/users"),
      ]);
      setRequests(nextRequests);
      setUsers(nextUsers);
      setLoadError(false);
    } catch {
      setLoadError(true);
    }
  }, []);
  const refresh = useCallback(() => {
    void loadDirectory();
    void installations.refresh();
  }, [loadDirectory, installations.refresh]);
  useEffect(() => {
    void loadDirectory();
    const timer = setInterval(() => void loadDirectory(), 3000);
    return () => clearInterval(timer);
  }, [loadDirectory]);
  async function decide(id: string, approve: boolean) {
    try {
      await request(`/api/v1/app-registrations/${id}/decision`, csrf, {
        method: "POST",
        body: JSON.stringify({ approve }),
      });
      setMessage(approve ? "앱 등록을 승인했습니다." : "등록을 거절했습니다.");
      refresh();
    } catch (e) {
      setMessage(String(e));
    }
  }
  async function grant(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    try {
      await request("/api/v1/grants", csrf, {
        method: "PUT",
        body: JSON.stringify({
          userId: form.get("user"),
          appId: form.get("app"),
          environment: form.get("environment"),
          permission: form.get("permission"),
          enabled: form.get("enabled") === "true",
        }),
      });
      setMessage("권한을 변경했습니다.");
    } catch (e) {
      setMessage(String(e));
    }
  }
  return (
    <>
      <h1>플랫폼 관리</h1>
      {loadError && (
        <LoadError message="앱 등록 신청 또는 사용자 목록을 불러오지 못했습니다." onRetry={refresh} />
      )}
      <Panel title="앱 등록 신청">
        {requests === null ? (
          !loadError && <p className="load-pending">불러오는 중…</p>
        ) : requests.filter((r) => r.status === "pending").length === 0 ? (
          <p className="empty-state">대기 중인 신청이 없습니다.</p>
        ) : (
          requests
            .filter((r) => r.status === "pending")
            .map((r) => (
              <div key={r.id}>
                <p>
                  {r.name} · {r.appId}
                </p>
                <Button onClick={() => decide(r.id, true)}>승인</Button>{" "}
                <Button onClick={() => decide(r.id, false)}>거절</Button>
              </div>
            ))
        )}
      </Panel>
      <Panel title="앱·환경 권한">
        <form onSubmit={grant}>
          <label>
            사용자{" "}
            <select name="user" disabled={users === null}>
              {(users ?? []).map((u) => (
                <option key={u.id} value={u.id}>
                  {u.name}
                </option>
              ))}
            </select>
          </label>{" "}
          <label>
            앱 ID <input name="app" required pattern="[a-z][a-z0-9-]*" />
          </label>{" "}
          <label>
            환경{" "}
            <select name="environment">
              <option value="dev">테스트</option>
              <option value="prod">운영</option>
            </select>
          </label>{" "}
          <label>
            권한{" "}
            <select name="permission">
              <option value="use">사용</option>
              <option value="develop">개발 (운영 배포 불가)</option>
              <option value="deploy">배포 (운영 승인·제어)</option>
            </select>
          </label>{" "}
          <select aria-label="부여 또는 회수" name="enabled">
            <option value="true">부여</option>
            <option value="false">회수</option>
          </select>{" "}
          <Button type="submit" disabled={users === null}>
            권한 적용
          </Button>
        </form>
      </Panel>
      {message && <p role="status">{message}</p>}
      {installations.items === null ? (
        installations.status === "error" ? (
          <LoadError message="앱 목록을 불러오지 못했습니다." onRetry={refresh} />
        ) : (
          <p className="load-pending">앱 목록을 불러오는 중…</p>
        )
      ) : (
        <>
          {installations.status === "error" && (
            <StaleNotice updatedAt={installations.updatedAt} onRetry={refresh} />
          )}
          <div className="grid">
            {installations.items
              .filter((a) => a.kind === "docker_app")
              .map((a) => (
                <Panel key={a.id} title={`${a.name} · ${a.environment}`}>
                  <p>
                    {a.version} · DB {a.dbStatus}
                  </p>
                  <Link to={environmentPath(a.environment, a.appId) as "/"}>앱 열기</Link>
                  <RuntimeHealth app={a} />
                  <RuntimeControls app={a} csrf={csrf} refresh={refresh} />
                </Panel>
              ))}
          </div>
        </>
      )}
    </>
  );
}
