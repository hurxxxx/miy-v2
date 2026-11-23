import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { consoleReturnPath } from "../lib/paths";
import { LoginRecovery } from "./LoginRecovery";
import { useSurface } from "../data/session";

export function Login() {
  const { t } = useTranslation("portal");
  const options = useSurface();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState("");
  const returnTo = () => options.surface === "console"
    ? consoleReturnPath(`${window.location.pathname}${window.location.search}${window.location.hash}`, window.location.origin)
    : null;
  async function enter(account: string) {
    setBusy(account);
    setError("");
    try {
      const result = await request<{ redirect: string }>("/auth/development", undefined, {
        method: "POST",
        body: JSON.stringify({ account }),
      });
      window.location.assign(returnTo() ?? result.redirect);
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
            if (options.surface !== "console") return;
            const target = returnTo();
            if (target) sessionStorage.setItem("miy.returnTo", target);
            else sessionStorage.removeItem("miy.returnTo");
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
