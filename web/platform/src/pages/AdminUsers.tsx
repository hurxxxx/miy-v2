import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { request } from "@miy/sdk";
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
type LinkState = "pending" | "completed" | "revoked" | "expired" | "inactive";
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
const states: Record<LinkState, { label: string; tone: Tone }> = {
  pending: { label: "사용 전", tone: "info" },
  completed: { label: "사용함", tone: "success" },
  revoked: { label: "취소됨", tone: "neutral" },
  expired: { label: "만료됨", tone: "neutral" },
  inactive: { label: "사용 불가", tone: "warning" },
};

function failure(error: unknown) {
  const text = String(error);
  if (text.includes("user_exists")) return "이미 등록된 로그인 계정 ID입니다.";
  if (text.includes("invalid_user_id")) return "로그인 계정 ID는 소문자 UUID 형식이어야 합니다.";
  if (text.includes("invalid_user_name")) return "표시 이름은 1~120자로 입력하세요.";
  if (text.includes("last_enabled_administrator")) return "사용 중인 마지막 관리자는 해제하거나 중지할 수 없습니다.";
  if (text.includes("account_links_unavailable")) return "이 서버에는 계정 링크 발급이 설정되지 않았습니다.";
  if (text.includes("account_link_limit")) return "사용 전인 링크가 너무 많습니다. 쓰지 않는 링크를 취소한 뒤 다시 발급하세요.";
  if (text.includes("account_link_not_pending")) return "이미 사용됐거나 취소된 링크입니다.";
  if (text.includes("user_disabled")) return "사용 중지된 계정에는 재설정 링크를 발급할 수 없습니다.";
  if (text.includes("invalid_account_link")) return "입력한 값을 확인해 주세요.";
  return text;
}

function when(value: string) {
  return new Date(value).toLocaleString("ko-KR", { dateStyle: "short", timeStyle: "short" });
}

export function UserAdmin({ csrf, selfId, users, report, refresh }: Props) {
  const { confirm, dialog } = useConfirm();
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
      report(failure(error));
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
      report(failure(error));
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
      "초대 링크를 만들었습니다. 아래 주소를 초대할 사람에게 전달하세요.",
    );
    if (ok) form.reset();
  }

  async function resetPassword(user: Account) {
    const ok = await confirm({
      title: "비밀번호 재설정 링크를 만들까요?",
      body: (
        <p>
          <strong>{user.name}</strong> 계정의 새 비밀번호를 정할 수 있는 24시간짜리 링크를 만듭니다. 이전에 만든 재설정
          링크는 취소됩니다. 링크를 가진 사람이 비밀번호를 바꿀 수 있으므로 본인에게만 전달하세요.
        </p>
      ),
      confirmLabel: "링크 만들기",
      tone: "primary",
    });
    if (!ok) return;
    await issue(
      { kind: "password_reset", userId: user.id },
      user.name,
      "비밀번호 재설정 링크를 만들었습니다. 아래 주소를 본인에게 전달하세요.",
    );
  }

  async function revoke(link: AccountLink) {
    setBusy(true);
    try {
      await request(`/api/v1/account-links/${encodeURIComponent(link.id)}/revoke`, csrf, { method: "POST" });
      report("링크를 취소했습니다.");
    } catch (error) {
      report(failure(error));
    } finally {
      setBusy(false);
      void loadLinks();
    }
  }

  async function copy() {
    if (!issued) return;
    try {
      await navigator.clipboard.writeText(issued.url);
      setCopied("복사했습니다.");
    } catch {
      setCopied("주소를 선택해 직접 복사해 주세요.");
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

  const recent = (links ?? []).filter((link) => link.state === "pending").concat(
    (links ?? []).filter((link) => link.state !== "pending").slice(0, 10),
  );
  return (
    <>
      <Panel
        title="사용자 초대"
        className="account-links"
        actions={links && <Badge tone="neutral">사용 전 {links.filter((link) => link.state === "pending").length}건</Badge>}
      >
        {available === null ? (
          <Loading />
        ) : available ? (
          <form className="grant-form user-form" onSubmit={invite}>
            <Field label="이름" hint="초대받은 사람이 계정을 만들 때 고칠 수 있습니다.">
              <input name="name" required maxLength={120} autoComplete="off" />
            </Field>
            <Field label="새 앱 등록 신청">
              <select name="canDevelop" defaultValue="false">
                <option value="false">허용 안 함</option>
                <option value="true">허용</option>
              </select>
            </Field>
            <Field label="링크 유효 기간">
              <select name="expiresInHours" defaultValue="168">
                <option value="24">1일</option>
                <option value="72">3일</option>
                <option value="168">7일</option>
                <option value="720">30일</option>
              </select>
            </Field>
            <div className="grant-form-actions">
              <Button type="submit" variant="primary" disabled={busy}>
                초대 링크 만들기
              </Button>
            </div>
          </form>
        ) : (
          <Notice tone="warning">
            이 서버에는 계정 링크 발급이 설정되지 않았습니다. 로그인 서버에서 계정을 만든 뒤 아래 “로그인 서버 계정 직접
            등록”을 사용하세요.
          </Notice>
        )}
        {issued && (
          <div className="account-link-issued" role="group" aria-label="방금 만든 링크" tabIndex={-1} ref={issuedBox}>
            <p>
              <strong>{issued.name ?? "이 사용자"}</strong>
              {issued.kind === "invitation" ? "에게 전달할 초대 링크" : " 계정의 비밀번호 재설정 링크"} ·{" "}
              {when(issued.expiresAt)}까지
            </p>
            <pre aria-label="링크 주소">
              <code>{issued.url}</code>
            </pre>
            <p className="account-link-warning">
              이 주소는 지금만 표시되며 한 번만 쓸 수 있습니다. 주소를 가진 사람이
              {issued.kind === "invitation" ? " 계정을 만들 수 있으니" : " 비밀번호를 바꿀 수 있으니"} 본인에게만
              전달하세요.
            </p>
            <div className="account-link-issued-actions">
              <Button variant="primary" onClick={copy}>
                주소 복사
              </Button>
              <Button onClick={() => setIssued(null)}>닫기</Button>
              {copied && <span role="status">{copied}</span>}
            </div>
          </div>
        )}
        {recent.length > 0 && (
          <ul className="user-accounts account-link-list" aria-label="발급한 링크">
            {recent.map((link) => (
              <li key={link.id} className="user-account">
                <div className="user-account-name">
                  <strong>{link.name ?? "이름 없음"}</strong>
                  <span>
                    {link.kind === "invitation" ? "초대" : "비밀번호 재설정"} · {link.createdByName} 발급 ·{" "}
                    {link.state === "completed" && link.completedAt
                      ? `${when(link.completedAt)} 사용`
                      : `${when(link.expiresAt)} 만료`}
                  </span>
                </div>
                <div className="user-account-badges">
                  <Badge tone={states[link.state].tone}>{states[link.state].label}</Badge>
                  {link.kind === "invitation" && link.canDevelop && <Badge tone="neutral">앱 개발</Badge>}
                </div>
                {link.state === "pending" && (
                  <div className="user-account-actions">
                    <Button size="sm" disabled={busy} onClick={() => void revoke(link)}>
                      링크 취소
                    </Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </Panel>
      <Panel title="로그인 서버 계정 직접 등록">
        <form className="grant-form user-form" onSubmit={register}>
          <Field label="로그인 계정 ID" hint="로그인 서버(Keycloak)에 이미 있는 계정의 ID. 새 사용자는 위 초대 링크를 쓰세요.">
            <input name="id" required pattern={SUBJECT} autoComplete="off" spellCheck={false} placeholder="00000000-0000-0000-0000-000000000000" />
          </Field>
          <Field label="표시 이름">
            <input name="name" required maxLength={120} autoComplete="off" />
          </Field>
          <Field label="새 앱 등록 신청">
            <select name="canDevelop" defaultValue="false">
              <option value="false">허용 안 함</option>
              <option value="true">허용</option>
            </select>
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
                  {available && user.enabled && (
                    <Button size="sm" disabled={busy} onClick={() => void resetPassword(user)}>
                      비밀번호 재설정 링크
                    </Button>
                  )}
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
