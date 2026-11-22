import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Notice } from "../ui";

type Phase = "working" | "expired" | "forbidden" | "failed";

/**
 * The end of a user switch (docs/CORE-PERMISSIONS.md "사용자 전환"): the
 * console opened this portal at `/switch-user#<code>`. The code leaves the
 * address at once and goes to this portal's own `/auth/switch`; on success the
 * browser holds that person's ordinary session and the portal opens.
 */
export function SwitchUser() {
  const { t } = useTranslation("portal");
  const [phase, setPhase] = useState<Phase>("working");
  const code = useRef<string | null>(null);
  const started = useRef(false);

  const exchange = () => {
    if (!code.current) {
      setPhase("expired");
      return;
    }
    setPhase("working");
    fetch("/auth/switch", {
      method: "POST",
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ code: code.current }),
    })
      .then((response) => {
        if (response.status === 204) {
          code.current = null;
          window.location.replace("/");
          return;
        }
        // The server consumed or refused the code: it cannot be retried.
        if (response.status === 401 || response.status === 403) code.current = null;
        setPhase(response.status === 401 ? "expired" : response.status === 403 ? "forbidden" : "failed");
      })
      .catch(() => setPhase("failed"));
  };

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    code.current = window.location.hash.slice(1) || null;
    window.history.replaceState(null, "", "/switch-user");
    exchange();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <main className="login-page">
      <div className="login-card">
        <p className="login-brand">miy</p>
        {phase === "working" ? (
          <p role="status">{t("switchUser.working")}</p>
        ) : (
          <Notice
            role="alert"
            tone={phase === "failed" ? "warning" : "danger"}
            actions={
              phase === "failed" && code.current ? (
                <Button size="sm" onClick={exchange}>
                  {t("common:actions.retry")}
                </Button>
              ) : (
                <a href="/">{t("switchUser.home")}</a>
              )
            }
          >
            <p>{t(`switchUser.${phase}`)}</p>
          </Notice>
        )}
      </div>
    </main>
  );
}
