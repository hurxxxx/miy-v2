import { useEffect, useState } from "react";
import { Link } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { isCliApprovalPath } from "../lib/paths";
import { LoginRecovery } from "./LoginRecovery";

type LoginOptions = {
  developmentLogin: boolean;
  developmentOrigin: string;
  accounts: { account: string; label: string; description: string }[];
  oauthUrl: string;
};

export function Login() {
  const [options, setOptions] = useState<LoginOptions>();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState("");
  useEffect(() => {
    request<LoginOptions>("/auth/options")
      .then((options) =>
        setOptions(
          options.developmentLogin && options.developmentOrigin !== window.location.origin
            ? { ...options, developmentLogin: false, accounts: [] }
            : options,
        ),
      )
      .catch(() => setError("로그인 정보를 불러올 수 없습니다."));
  }, []);
  async function enter(account: string) {
    setBusy(account);
    setError("");
    try {
      const result = await request<{ redirect: string }>("/auth/development", undefined, {
        method: "POST",
        body: JSON.stringify({ account }),
      });
      window.location.assign(isCliApprovalPath(window.location.pathname) ? window.location.href : result.redirect);
    } catch {
      setError("로그인하지 못했습니다. 다시 시도해 주세요.");
      setBusy(undefined);
    }
  }
  return (
    <main className="login-page">
      <div className="login-card">
        <p className="login-brand">miy</p>
        <h1>로그인</h1>
        <p className="login-lead">
          {options?.developmentLogin ? "개발 계정을 선택하면 바로 시작합니다." : "업무 앱과 데이터를 한곳에서"}
        </p>
        <LoginRecovery />
        {options?.developmentLogin && (
          <div className="quick-accounts">
            {options.accounts.map((account, index) => (
              <button
                type="button"
                key={account.account}
                className={`account-button ${index === 0 ? "primary" : ""}`}
                disabled={!!busy}
                onClick={() => enter(account.account)}
              >
                <strong>{busy === account.account ? "접속 중…" : account.label}</strong>
                <span>{account.description}</span>
              </button>
            ))}
          </div>
        )}
        {error && <p role="alert">{error}</p>}
        <a
          className={`login-primary ${options?.developmentLogin ? "oauth-link" : ""}`}
          onClick={() => {
            if (isCliApprovalPath(window.location.pathname))
              sessionStorage.setItem("miy.returnTo", window.location.pathname);
          }}
          href={options?.oauthUrl || "/auth/login"}
        >
          {options?.developmentLogin ? "OAuth로 로그인" : "로그인"}
        </a>
        <div className="login-footer">
          <Link to={"/cli/install" as "/"} className="cli-install-link">
            CLI 설치 안내
          </Link>
        </div>
      </div>
    </main>
  );
}
