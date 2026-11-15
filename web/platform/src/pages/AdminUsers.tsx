import { useState, type FormEvent } from "react";
import { request } from "@miy/sdk";
import { Badge, Button, Field, Loading, Panel, useConfirm } from "../ui";
import type { DirectoryUser } from "./AdminAccess";

// The identity provider only authenticates. A person can sign in after an
// administrator registers the provider's user ID here, and holds no app
// permission until one is granted below.
export type Account = DirectoryUser & { admin: boolean; canDevelop: boolean; enabled: boolean };

type Props = {
  csrf: string;
  selfId: string;
  users: Account[] | null;
  report: (message: string) => void;
  refresh: () => void;
};

const SUBJECT = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";

function failure(error: unknown) {
  const text = String(error);
  if (text.includes("user_exists")) return "이미 등록된 로그인 계정 ID입니다.";
  if (text.includes("invalid_user_id")) return "로그인 계정 ID는 소문자 UUID 형식이어야 합니다.";
  if (text.includes("invalid_user_name")) return "표시 이름은 1~120자로 입력하세요.";
  if (text.includes("last_enabled_administrator")) return "사용 중인 마지막 관리자는 해제하거나 중지할 수 없습니다.";
  return text;
}

export function UserAdmin({ csrf, selfId, users, report, refresh }: Props) {
  const { confirm, dialog } = useConfirm();
  const [busy, setBusy] = useState(false);

  async function send(path: string, method: "POST" | "PATCH", body: unknown, done: string) {
    setBusy(true);
    try {
      await request(path, csrf, { method, body: JSON.stringify(body) });
      report(done);
      refresh();
      return true;
    } catch (error) {
      report(failure(error));
      return false;
    } finally {
      setBusy(false);
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
        canDevelop: data.get("canDevelop") === "on",
      },
      "사용자를 등록했습니다. 앱 권한은 아래에서 따로 부여하세요.",
    );
    if (ok) form.reset();
  }

  async function setAdmin(user: Account) {
    const next = !user.admin;
    const ok = await confirm({
      title: next ? "관리자로 지정할까요?" : "관리자 지정을 해제할까요?",
      body: next ? (
        <p>
          <strong>{user.name}</strong> 계정이 사용자·그룹·권한·앱 등록 승인과 모든 앱의 배포·제어를 할 수 있게 됩니다.
        </p>
      ) : (
        <p>
          <strong>{user.name}</strong> 계정이 플랫폼 관리 기능을 더 이상 사용할 수 없습니다.
          {user.id === selfId && " 지금 로그인한 본인 계정입니다."}
        </p>
      ),
      confirmLabel: next ? "관리자 지정" : "지정 해제",
      tone: next ? "primary" : "danger",
    });
    if (!ok) return;
    await send(`/api/v1/users/${encodeURIComponent(user.id)}`, "PATCH", { admin: next }, next ? "관리자로 지정했습니다." : "관리자 지정을 해제했습니다.");
  }

  async function setEnabled(user: Account) {
    const next = !user.enabled;
    if (!next) {
      const ok = await confirm({
        title: "이 계정의 사용을 중지할까요?",
        body: (
          <p>
            <strong>{user.name}</strong> 계정의 로그인·CLI·진행 중인 작업이 모두 차단됩니다. 권한 설정은 지우지 않습니다.
          </p>
        ),
        confirmLabel: "사용 중지",
        tone: "danger",
      });
      if (!ok) return;
    }
    await send(`/api/v1/users/${encodeURIComponent(user.id)}/state`, "POST", { enabled: next }, next ? "계정 사용을 다시 허용했습니다." : "계정 사용을 중지했습니다.");
  }

  return (
    <>
      <Panel title="사용자 등록">
        <form className="grant-form" onSubmit={register}>
          <Field label="로그인 계정 ID" hint="로그인 서버(Keycloak) 사용자 상세의 ID. 계정은 로그인 서버에서 먼저 만듭니다.">
            <input name="id" required pattern={SUBJECT} autoComplete="off" spellCheck={false} placeholder="00000000-0000-0000-0000-000000000000" />
          </Field>
          <Field label="표시 이름">
            <input name="name" required maxLength={120} autoComplete="off" />
          </Field>
          <Field label="앱 개발">
            <label className="user-option">
              <input type="checkbox" name="canDevelop" /> 새 앱 등록을 신청할 수 있음
            </label>
          </Field>
          <div className="grant-form-actions">
            <Button type="submit" variant="primary" disabled={busy}>
              사용자 등록
            </Button>
          </div>
        </form>
      </Panel>
      <Panel title="사용자 계정" actions={users && <Badge tone="neutral">{users.length}명</Badge>}>
        {users === null ? (
          <Loading />
        ) : (
          <ul className="user-accounts">
            {users.map((user) => (
              <li key={user.id} className="user-account">
                <div className="user-account-name">
                  <strong>{user.name}</strong>
                  <code>{user.id}</code>
                </div>
                <div className="user-account-badges">
                  {user.admin && <Badge tone="info">관리자</Badge>}
                  {user.canDevelop && <Badge tone="neutral">앱 개발</Badge>}
                  {!user.enabled && <Badge tone="danger">사용 중지</Badge>}
                </div>
                <div className="user-account-actions">
                  <Button size="sm" disabled={busy} onClick={() => void setAdmin(user)}>
                    {user.admin ? "관리자 해제" : "관리자 지정"}
                  </Button>
                  <Button
                    size="sm"
                    disabled={busy}
                    onClick={() =>
                      void send(
                        `/api/v1/users/${encodeURIComponent(user.id)}`,
                        "PATCH",
                        { canDevelop: !user.canDevelop },
                        user.canDevelop ? "앱 개발 신청 자격을 해제했습니다." : "앱 개발 신청 자격을 부여했습니다.",
                      )
                    }
                  >
                    {user.canDevelop ? "앱 개발 해제" : "앱 개발 허용"}
                  </Button>
                  <Button size="sm" disabled={busy} onClick={() => void setEnabled(user)}>
                    {user.enabled ? "사용 중지" : "사용 재개"}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Panel>
      {dialog}
    </>
  );
}
