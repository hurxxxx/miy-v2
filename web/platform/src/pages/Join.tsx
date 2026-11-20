import { useEffect, useState, type FormEvent } from "react";
import { request } from "@miy/sdk";
import { Button, Field, Loading } from "../ui";

// An administrator hands over a one-time link: an invitation lets the person create
// their own sign-in account, a reset link lets them set a new password. The secret
// sits in the URL fragment, which browsers neither send to servers nor log.
type LinkKind = "invitation" | "password_reset";
type AccountLink = {
  kind: LinkKind;
  name: string | null;
  expiresAt: string;
  passwordMinLength: number;
  portalOrigin: string;
};
type Phase =
  | { status: "loading" }
  | { status: "invalid" }
  | { status: "error"; message: string }
  | { status: "ready"; link: AccountLink }
  | { status: "done"; kind: LinkKind; loginUrl: string };

const STORE = "miy.accountLink";
const TOKEN = /^[A-Za-z0-9_-]{43}$/;
const USERNAME = "[a-z0-9][a-z0-9._\\-]{2,31}";

/** Read the secret once, take it out of the address bar and keep it for this tab only. */
function takeToken(): string | null {
  const fragment = window.location.hash.slice(1);
  if (fragment) {
    window.history.replaceState(null, "", window.location.pathname);
    if (!TOKEN.test(fragment)) return null;
    try {
      sessionStorage.setItem(STORE, fragment);
    } catch {
      // Private browsing can refuse storage; the token still works until reload.
    }
    return fragment;
  }
  try {
    const kept = sessionStorage.getItem(STORE);
    return kept && TOKEN.test(kept) ? kept : null;
  } catch {
    return null;
  }
}

function forget() {
  try {
    sessionStorage.removeItem(STORE);
  } catch {
    // Nothing was stored.
  }
}

const failures: Record<string, string> = {
  invalid_username: "아이디는 영문 소문자·숫자로 시작하고, 영문 소문자·숫자·마침표·밑줄·하이픈으로 3~32자여야 합니다.",
  invalid_email: "이메일 주소를 확인해 주세요.",
  invalid_user_name: "이름은 1~120자로 입력해 주세요.",
  password_policy: "비밀번호가 조건에 맞지 않습니다. 길이를 늘리고 아이디와 다르게 정해 주세요.",
  account_exists: "이미 사용 중인 아이디 또는 이메일입니다. 다른 값을 입력해 주세요.",
  account_rejected: "입력한 값을 로그인 서버가 받지 않았습니다. 아이디와 이메일을 확인해 주세요.",
  identity_unavailable: "로그인 서버에 연결하지 못했습니다. 잠시 후 다시 시도해 주세요.",
  account_links_unavailable: "이 서버에서는 링크로 계정을 만들 수 없습니다. 관리자에게 문의해 주세요.",
  network_unavailable: "서버에 연결하지 못했습니다. 연결 상태를 확인하고 다시 시도해 주세요.",
};

function code(error: unknown) {
  return error instanceof Error ? error.message : "";
}

export function Join() {
  const [token] = useState(takeToken);
  const [phase, setPhase] = useState<Phase>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!token) {
      setPhase({ status: "invalid" });
      return;
    }
    const controller = new AbortController();
    setPhase({ status: "loading" });
    request<AccountLink>("/api/v1/account-links/inspect", undefined, {
      method: "POST",
      body: JSON.stringify({ token }),
      signal: controller.signal,
    })
      .then((link) => setPhase({ status: "ready", link }))
      .catch((failure: unknown) => {
        if (controller.signal.aborted) return;
        if (code(failure) === "account_link_invalid") {
          forget();
          setPhase({ status: "invalid" });
        } else {
          setPhase({
            status: "error",
            message: failures[code(failure)] ?? "링크를 확인하지 못했습니다. 잠시 후 다시 시도해 주세요.",
          });
        }
      });
    return () => controller.abort();
  }, [token, attempt]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (phase.status !== "ready" || busy) return;
    const data = new FormData(event.currentTarget);
    const password = String(data.get("password"));
    if (password !== String(data.get("confirm"))) {
      setError("비밀번호와 비밀번호 확인이 서로 다릅니다.");
      return;
    }
    const invitation = phase.link.kind === "invitation";
    setBusy(true);
    setError("");
    try {
      const result = await request<{ kind: LinkKind; loginUrl: string }>(
        "/api/v1/account-links/complete",
        undefined,
        {
          method: "POST",
          body: JSON.stringify({
            token,
            password,
            ...(invitation && {
              username: String(data.get("username")).trim(),
              name: String(data.get("name")).trim(),
              email: String(data.get("email")).trim(),
            }),
          }),
        },
      );
      forget();
      setPhase({ status: "done", kind: result.kind, loginUrl: result.loginUrl });
    } catch (failure) {
      if (code(failure) === "account_link_invalid") {
        forget();
        setPhase({ status: "invalid" });
      } else {
        setError(failures[code(failure)] ?? "처리하지 못했습니다. 잠시 후 다시 시도해 주세요.");
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="login-page">
      <div className="login-card account-link-card">
        <p className="login-brand">miy</p>
        {phase.status === "loading" && <Loading>링크를 확인하는 중…</Loading>}
        {phase.status === "invalid" && (
          <>
            <h1>사용할 수 없는 링크입니다</h1>
            <p className="login-lead">
              링크가 만료됐거나 이미 사용됐습니다. 주소를 끝까지 복사했는지 확인하고, 계속 열리지 않으면
              관리자에게 새 링크를 요청해 주세요.
            </p>
          </>
        )}
        {phase.status === "error" && (
          <>
            <h1>링크를 확인하지 못했습니다</h1>
            <p role="alert">{phase.message}</p>
            <Button variant="primary" onClick={() => setAttempt(attempt + 1)}>
              다시 확인
            </Button>
          </>
        )}
        {phase.status === "ready" && (
          <form className="account-link-form" onSubmit={submit}>
            {phase.link.kind === "invitation" ? (
              <>
                <h1>계정 만들기</h1>
                <p className="login-lead">초대받은 계정의 아이디와 비밀번호를 직접 정합니다.</p>
                <Field label="아이디" hint="로그인에 쓰는 이름. 영문 소문자·숫자·마침표·밑줄·하이픈 3~32자">
                  <input
                    name="username"
                    required
                    pattern={USERNAME}
                    maxLength={32}
                    autoComplete="username"
                    autoCapitalize="none"
                    spellCheck={false}
                  />
                </Field>
                <Field label="이름" hint="플랫폼 화면에 표시됩니다.">
                  <input name="name" required maxLength={120} defaultValue={phase.link.name ?? ""} autoComplete="name" />
                </Field>
                <Field label="이메일">
                  <input name="email" type="email" required maxLength={254} autoComplete="email" />
                </Field>
              </>
            ) : (
              <>
                <h1>비밀번호 재설정</h1>
                <p className="login-lead">
                  {phase.link.name ? (
                    <>
                      <strong>{phase.link.name}</strong> 계정의 새 비밀번호를 정합니다.
                    </>
                  ) : (
                    "계정의 새 비밀번호를 정합니다."
                  )}
                </p>
              </>
            )}
            <Field
              label={phase.link.kind === "invitation" ? "비밀번호" : "새 비밀번호"}
              hint={`${phase.link.passwordMinLength}자 이상. 아이디와 같을 수 없습니다.`}
            >
              <input
                name="password"
                type="password"
                required
                minLength={phase.link.passwordMinLength}
                maxLength={256}
                autoComplete="new-password"
              />
            </Field>
            <Field label="비밀번호 확인">
              <input
                name="confirm"
                type="password"
                required
                minLength={phase.link.passwordMinLength}
                maxLength={256}
                autoComplete="new-password"
              />
            </Field>
            {error && <p role="alert">{error}</p>}
            <Button type="submit" variant="primary" disabled={busy} className="account-link-submit">
              {busy ? "처리 중…" : phase.link.kind === "invitation" ? "계정 만들기" : "비밀번호 변경"}
            </Button>
            <p className="account-link-expiry">
              이 링크는 {new Date(phase.link.expiresAt).toLocaleString("ko-KR", { dateStyle: "medium", timeStyle: "short" })}까지 한
              번만 쓸 수 있습니다.
            </p>
          </form>
        )}
        {phase.status === "done" && (
          <>
            <h1>{phase.kind === "invitation" ? "계정을 만들었습니다" : "비밀번호를 변경했습니다"}</h1>
            <p className="login-lead" role="status">
              {phase.kind === "invitation"
                ? "방금 정한 아이디와 비밀번호로 로그인하세요. 사용할 앱은 관리자가 권한을 부여한 뒤 보입니다."
                : "새 비밀번호로 다시 로그인하세요. 이전에 로그인한 브라우저는 로그아웃됩니다."}
            </p>
            <a className="login-primary" href={phase.loginUrl}>
              로그인하러 가기
            </a>
          </>
        )}
      </div>
    </main>
  );
}
