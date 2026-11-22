import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { request, type Installation } from "@miy/sdk";
import { Button, useConfirm } from "../ui";
import { requestId } from "../lib/paths";

export function RuntimeControls({
  app,
  csrf,
  refresh,
}: {
  app: Installation;
  csrf: string;
  refresh: () => void;
}) {
  const { t } = useTranslation("portal");
  const label = (action: string) => t(`runtimeControls.actions.${action}`);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const { confirm, dialog, cancel } = useConfirm();
  const generation = useRef(0);
  const mounted = useRef(true);
  const returnFocus = useRef<HTMLElement | null>(null);
  useEffect(() => {
    mounted.current = true;
    generation.current++;
    return () => {
      mounted.current = false;
      generation.current++;
      cancel();
    };
  }, [
    app.id,
    app.appId,
    app.environment,
    app.releaseId,
    app.runtimeStatus,
    app.canManage,
    cancel,
  ]);
  useEffect(() => {
    if (busy || !returnFocus.current) return;
    const trigger = returnFocus.current;
    returnFocus.current = null;
    const frame = requestAnimationFrame(() => {
      // A confirmed action keeps its opener disabled until the job settles.
      // Do not steal focus if the user moved elsewhere while it was running.
      const focused = document.activeElement;
      if (
        trigger.isConnected &&
        !trigger.matches(":disabled") &&
        (!focused || focused === document.body || !focused.isConnected)
      )
        trigger.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [busy]);
  async function control(action: string, trigger: HTMLElement) {
    const startedIn = generation.current;
    const targetIsCurrent = () =>
      mounted.current && generation.current === startedIn;
    setBusy(action);
    setError("");
    try {
      const plan = await request<{ canSubmit: boolean; expectedState: unknown; changes: unknown }>(
        `/api/v1/apps/${app.appId}/runtime/preflight`,
        csrf,
        {
          method: "POST",
          body: JSON.stringify({ target: { appId: app.appId, environment: app.environment }, action }),
        },
      );
      if (!targetIsCurrent()) return;
      if (!plan.canSubmit) throw new Error(t("runtimeControls.cannotRun"));
      const needsConfirmation = app.environment === "prod" || action === "remove";
      if (needsConfirmation) returnFocus.current = trigger;
      // The dialog names the exact target the preflight inspected.
      if (
        needsConfirmation &&
        !(await confirm({
          title: t("runtimeControls.confirmTitle", { action: label(action) }),
          body: (
            <>
              <p>
                {t("runtimeControls.target")} <strong>{app.name}</strong> (<code>{app.appId}</code>) · {t("runtimeControls.environment")} <strong>{app.environment}</strong>
              </p>
              <p>{t("runtimeControls.action", { action })}</p>
              <p>{t("runtimeControls.impact")}</p>
            </>
          ),
          confirmLabel: label(action),
          returnFocus: trigger,
          tone: action === "remove" || action === "stop" ? "danger" : "primary",
        }))
      )
        return;
      if (!targetIsCurrent()) return;
      const job = await request<{ id: string }>(`/api/v1/apps/${app.appId}/runtime`, csrf, {
        method: "POST",
        body: JSON.stringify({
          action,
          environment: app.environment,
          expectedState: plan.expectedState,
          confirmed: needsConfirmation,
          idempotencyKey: requestId(),
        }),
      });
      for (let i = 0; i < 240; i++) {
        if (!mounted.current) return;
        const state = await request<{ status: string; step: string; errorCode?: string }>(
          `/api/v1/runtime/jobs/${job.id}`,
        );
        if (!mounted.current) return;
        if (state.status === "failed") throw new Error(state.errorCode || t("runtimeControls.jobFailed"));
        if (state.status === "succeeded") {
          refresh();
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      throw new Error(t("runtimeControls.delayed"));
    } catch (error) {
      if (mounted.current) setError(error instanceof Error ? error.message : t("runtimeControls.failed"));
    } finally {
      if (mounted.current) {
        setBusy("");
        refresh();
      }
    }
  }
  if (app.runtimeStatus === "removed")
    return <p>{t("runtimeControls.removed")}</p>;
  const actions = app.runtimeStatus === "recovery_required" ? ["remove"]
    : app.runtimeStatus === "running" ? ["stop", "restart", "remove"] : ["start", "remove"];
  return (
    <div className="runtime-controls">
      {app.imageId && <p className="runtime-image">{app.imageId.slice(0, 22)}…</p>}
      <div>
        {actions.map((action) => (
          <Button key={action} size="sm" disabled={!!busy} onClick={(event) => control(action, event.currentTarget)}>
            {label(action)}
          </Button>
        ))}
      </div>
      {busy && <p role="status">{t("runtimeControls.busy")}</p>}
      {error && <p role="alert">{t("runtimeControls.error", { error })}</p>}
      {dialog}
    </div>
  );
}
