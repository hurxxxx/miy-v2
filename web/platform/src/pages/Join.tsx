import { useEffect, useState, type FormEvent } from "react";
import { Trans, useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
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
  | { status: "resetUnknown" }
  | { status: "error"; message: string }
  | { status: "ready"; link: AccountLink }
  | { status: "done"; kind: LinkKind; loginUrl: string; endingSessions: boolean };

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

// Server error codes with their own message (portal join.errors.*).
const FAILURES = new Set([
  "invalid_username",
  "invalid_email",
  "invalid_user_name",
  "password_policy",
  "account_exists",
  "account_rejected",
  "identity_unavailable",
  "account_links_unavailable",
  "network_unavailable",
]);

// A name inside a translated sentence. Trans replaces a component's children, so
// the name is a prop: it stays plain text and is never parsed as markup.
function Name({ value }: { value: string }) {
  return <strong>{value}</strong>;
}

function code(error: unknown) {
  return error instanceof Error ? error.message : "";
}

/** The message for a failed request: the code's own message, or `fallback`. */
function failureMessage(t: TFunction<"portal">, error: unknown, fallback: string) {
  return FAILURES.has(code(error)) ? t(`join.errors.${code(error)}`) : fallback;
}

export function Join() {
  const { t, i18n } = useTranslation("portal");
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
            message: failureMessage(t, failure, t("join.linkCheckFailed")),
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
      setError(t("join.passwordMismatch"));
      return;
    }
    const invitation = phase.link.kind === "invitation";
    setBusy(true);
    setError("");
    try {
      const result = await request<{ status: string; kind: LinkKind; loginUrl: string }>(
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
      setPhase({
        status: "done",
        kind: result.kind,
        loginUrl: result.loginUrl,
        endingSessions: result.status === "ending_sessions",
      });
    } catch (failure) {
      if (code(failure) === "password_reset_unknown") {
        forget();
        setPhase({ status: "resetUnknown" });
      } else if (code(failure) === "account_link_invalid") {
        forget();
        setPhase({ status: "invalid" });
      } else {
        setError(failureMessage(t, failure, t("join.submitFailed")));
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="login-page">
      <div className="login-card account-link-card">
        <p className="login-brand">miy</p>
        {phase.status === "loading" && <Loading>{t("join.checking")}</Loading>}
        {phase.status === "invalid" && (
          <>
            <h1>{t("join.invalidTitle")}</h1>
            <p className="login-lead">
              {t("join.invalidLead")}
            </p>
          </>
        )}
        {phase.status === "resetUnknown" && (
          <>
            <h1>{t("join.resetUnknownTitle")}</h1>
            <p className="login-lead" role="status">
              {t("join.resetUnknownLead")}
            </p>
          </>
        )}
        {phase.status === "error" && (
          <>
            <h1>{t("join.errorTitle")}</h1>
            <p role="alert">{phase.message}</p>
            <Button variant="primary" onClick={() => setAttempt(attempt + 1)}>
              {t("join.retry")}
            </Button>
          </>
        )}
        {phase.status === "ready" && (
          <form className="account-link-form" onSubmit={submit}>
            {phase.link.kind === "invitation" ? (
              <>
                <h1>{t("join.createTitle")}</h1>
                <p className="login-lead">{t("join.createLead")}</p>
                <Field label={t("join.username")} hint={t("join.usernameHint")}>
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
                <Field label={t("join.name")} hint={t("join.nameHint")}>
                  <input name="name" required maxLength={120} defaultValue={phase.link.name ?? ""} autoComplete="name" />
                </Field>
                <Field label={t("join.email")}>
                  <input name="email" type="email" required maxLength={254} autoComplete="email" />
                </Field>
              </>
            ) : (
              <>
                <h1>{t("join.resetTitle")}</h1>
                <p className="login-lead">
                  {phase.link.name ? (
                    <Trans t={t} i18nKey="join.resetFor" components={{ name: <Name value={phase.link.name} /> }} />
                  ) : (
                    t("join.resetLead")
                  )}
                </p>
              </>
            )}
            <Field
              label={t(phase.link.kind === "invitation" ? "join.password" : "join.newPassword")}
              hint={t("join.passwordHint", { min: phase.link.passwordMinLength })}
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
            <Field label={t("join.confirmPassword")}>
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
              {t(busy ? "join.busy" : phase.link.kind === "invitation" ? "join.createSubmit" : "join.changePassword")}
            </Button>
            <p className="account-link-expiry">
              {t("join.expiry", {
                date: new Date(phase.link.expiresAt).toLocaleString(i18n.language, { dateStyle: "medium", timeStyle: "short" }),
              })}
            </p>
          </form>
        )}
        {phase.status === "done" && (
          <>
            <h1>{t(phase.endingSessions ? "join.doneEnding" : phase.kind === "invitation" ? "join.doneCreated" : "join.doneReset")}</h1>
            <p className="login-lead" role="status">
              {t(
                phase.endingSessions
                  ? "join.doneEndingLead"
                  : phase.kind === "invitation"
                    ? "join.doneCreatedLead"
                    : "join.doneResetLead",
              )}
            </p>
            <a className="login-primary" href={phase.loginUrl}>
              {t("join.goToLogin")}
            </a>
          </>
        )}
      </div>
    </main>
  );
}
