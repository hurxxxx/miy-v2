import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { request } from "@miy/sdk";
import { Button, Field } from "../ui";

export function Signup() {
  const { t } = useTranslation("portal");
  const [state, setState] = useState<"ready" | "sending" | "sent">("ready");
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (state !== "ready") return;
    const form = new FormData(event.currentTarget);
    setState("sending");
    setError("");
    try {
      await request("/api/v1/signup-requests", undefined, {
        method: "POST", body: JSON.stringify({ name: form.get("name"), email: form.get("email") }),
      });
      setState("sent");
    } catch (failure) {
      const code = failure instanceof Error ? failure.message : "";
      setError(t(code === "invalid_email" ? "join.errors.invalid_email" :
        code === "invalid_user_name" ? "join.errors.invalid_user_name" :
        code === "rate_limited" || code === "signup_queue_full" ? "signup.busy" : "signup.failed"));
      setState("ready");
    }
  }
  return <main className="login-page"><div className="login-card account-link-card">
    <p className="login-brand">miy</p>
    <h1>{t(state === "sent" ? "signup.received" : "signup.title")}</h1>
    {state === "sent" ? <p role="status">{t("signup.next")}</p> :
      <form className="account-link-form" onSubmit={submit}>
        <p>{t("signup.lead")}</p>
        <Field label={t("join.name")}><input name="name" required maxLength={120} autoComplete="name" /></Field>
        <Field label={t("join.email")}><input name="email" type="email" required maxLength={254} autoComplete="email" /></Field>
        {error && <p role="alert">{error}</p>}
        <Button type="submit" variant="primary" disabled={state === "sending"}>{t("signup.submit")}</Button>
      </form>}
    <div className="login-footer"><a href="/">{t("join.goToLogin")}</a></div>
  </div></main>;
}
