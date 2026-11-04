import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Link } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { useInstallations, useManagedInstallations } from "../data/installations";
import { useUser } from "../data/session";
import { environmentPath } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { RuntimeHealth } from "../components/RuntimeHealth";
import { RuntimeControls } from "../components/RuntimeControls";
import { Badge, Button, EmptyState, Field, Loading, Panel } from "../ui";
import { AppAccessAdmin, GroupAdmin, useGroups, type DirectoryUser } from "./AdminAccess";
import { AuditLog } from "./AdminAudit";

type Registration = { id: string; appId: string; name: string; status: string; requester: string };

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
  // The runtime panel uses the management list; the shared list feeds the rail.
  const installations = useInstallations();
  const managed = useManagedInstallations();
  const [requests, setRequests] = useState<Registration[] | null>(null);
  const [users, setUsers] = useState<DirectoryUser[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [message, setMessage] = useState("");
  const groups = useGroups(setMessage);
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
    void managed.refresh();
  }, [loadDirectory, installations.refresh, managed.refresh]);
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
  const pending = requests?.filter((r) => r.status === "pending") ?? [];
  return (
    <>
      <div className="page-header">
        <div>
          <h1>플랫폼 관리</h1>
          <p>앱 등록 신청, 사용자·그룹의 앱 권한과 회사 사용 상태, 실행 중인 앱을 관리합니다.</p>
        </div>
      </div>
      {loadError && <LoadError message="앱 등록 신청 또는 사용자 목록을 불러오지 못했습니다." onRetry={refresh} />}
      {message && (
        <p role="status" className="status-message">
          {message}
        </p>
      )}
      <div className="admin-grid">
        <Panel title="앱 등록 신청" actions={requests && <Badge tone={pending.length ? "info" : "neutral"}>{pending.length}건</Badge>}>
          {requests === null ? (
            !loadError && <Loading />
          ) : pending.length === 0 ? (
            <p className="ui-empty-inline">대기 중인 신청이 없습니다.</p>
          ) : (
            pending.map((r) => (
              <div key={r.id} className="registration-row">
                <p>
                  <strong>{r.name}</strong> · <code>{r.appId}</code>
                </p>
                <div className="registration-actions">
                  <Button size="sm" variant="primary" onClick={() => decide(r.id, true)}>
                    승인
                  </Button>
                  <Button size="sm" onClick={() => decide(r.id, false)}>
                    거절
                  </Button>
                </div>
              </div>
            ))
          )}
        </Panel>
        <Panel title="앱·환경 권한">
          <form className="grant-form" onSubmit={grant}>
            <Field label="사용자">
              <select name="user" disabled={users === null}>
                {(users ?? []).map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="앱 ID">
              <input name="app" required pattern="[a-z][a-z0-9-]*" autoComplete="off" />
            </Field>
            <Field label="환경">
              <select name="environment">
                <option value="dev">테스트</option>
                <option value="prod">운영</option>
              </select>
            </Field>
            <Field label="권한">
              <select name="permission">
                <option value="use">사용</option>
                <option value="develop">개발 (운영 배포 불가)</option>
                <option value="deploy">배포 (운영 승인·제어)</option>
              </select>
            </Field>
            <Field label="부여 또는 회수">
              <select name="enabled">
                <option value="true">부여</option>
                <option value="false">회수</option>
              </select>
            </Field>
            <div className="grant-form-actions">
              <Button type="submit" variant="primary" disabled={users === null}>
                권한 적용
              </Button>
            </div>
          </form>
        </Panel>
      </div>
      <h2 className="page-section-title">사용자·그룹 권한</h2>
      <div className="admin-grid">
        <GroupAdmin csrf={csrf} users={users} report={setMessage} directory={groups} />
        <AppAccessAdmin csrf={csrf} users={users} report={setMessage} directory={groups} />
      </div>
      <AuditLog users={users} report={setMessage} />
      <h2 className="page-section-title">앱 실행 상태</h2>
      {managed.items === null ? (
        managed.status === "error" ? (
          <LoadError message="앱 목록을 불러오지 못했습니다." onRetry={refresh} />
        ) : (
          <Loading>앱 목록을 불러오는 중…</Loading>
        )
      ) : (
        <>
          {managed.status === "error" && <StaleNotice updatedAt={managed.updatedAt} onRetry={refresh} />}
          {managed.items.filter((a) => a.kind === "docker_app").length === 0 ? (
            <EmptyState title="배포된 앱이 없습니다." />
          ) : (
            <div className="grid">
              {managed.items
                .filter((a) => a.kind === "docker_app")
                .map((a) => (
                  <Panel
                    key={a.id}
                    title={`${a.name} · ${a.environment}`}
                    actions={a.appDisabled && <Badge tone="warning">회사 사용 중지</Badge>}
                  >
                    <p>
                      {a.version} · DB {a.dbStatus}
                    </p>
                    {a.canOpen ? (
                      <Link to={environmentPath(a.environment, a.appId) as "/"}>앱 열기</Link>
                    ) : (
                      <p className="ui-empty-inline">
                        {a.appDisabled
                          ? "회사 사용 중지 중이라 열 수 없습니다. 시작·중지·배포는 계속할 수 있습니다."
                          : "지금은 이 앱을 열 수 없습니다."}
                      </p>
                    )}
                    <RuntimeHealth app={a} />
                    <RuntimeControls app={a} csrf={csrf} refresh={refresh} />
                  </Panel>
                ))}
            </div>
          )}
        </>
      )}
    </>
  );
}
