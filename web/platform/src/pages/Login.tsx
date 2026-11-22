import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { isCliApprovalPath } from "../lib/paths";
import { LoginRecovery } from "./LoginRecovery";
import { useSurface } from "../data/session";

export function Login() {
  const { t } = useTranslation("portal");
  const options = useSurface();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState("");
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
      setError(t("login.failed"));
      setBusy(undefined);
    }
  }
  return (
    <main className="login-page">
      <div className="login-card">
        <p className="login-brand">miy</p>
        <h1>{t("login.title")}</h1>
        <p className="login-lead">
          {t(options?.developmentLogin ? "login.devLead" : "login.lead")}
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
                <strong>{busy === account.account ? t("login.connecting") : account.label}</strong>
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
          {t(options?.developmentLogin ? "login.oauth" : "login.signIn")}
        </a>
        <p className="login-help">
          {t("login.help")}
        </p>
        <div className="login-footer">
          <Link to={"/cli/install" as "/"} className="cli-install-link">
            {t("login.cliInstall")}
          </Link>
        </div>
      </div>
    </main>
  );
}
