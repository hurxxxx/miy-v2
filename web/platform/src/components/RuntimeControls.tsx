import { useState } from "react";
import { request, type Installation } from "@miy/sdk";
import { Button } from "@miy/ui";
import { requestId } from "../lib/paths";

const labels: Record<string, string> = {
  start: "시작",
  stop: "중지",
  restart: "재시작",
  remove: "앱 제거(DB 보존)",
};

export function RuntimeControls({
  app,
  csrf,
  refresh,
}: {
  app: Installation;
  csrf: string;
  refresh: () => void;
}) {
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  async function control(action: string) {
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
      if (!plan.canSubmit) throw new Error("현재 상태에서 실행할 수 없습니다. 새로고침 후 확인하세요.");
      if (
        (app.environment === "prod" || action === "remove") &&
        !window.confirm(`${app.name} (${app.appId}) / ${app.environment}: ${action} 실행
실행 중인 앱에 영향을 줍니다. 데이터베이스와 업무 데이터는 보존됩니다. 계속할까요?`)
      )
        return;
      const job = await request<{ id: string }>(`/api/v1/apps/${app.appId}/runtime`, csrf, {
        method: "POST",
        body: JSON.stringify({
          action,
          environment: app.environment,
          expectedState: plan.expectedState,
          confirmed: app.environment === "prod" || action === "remove",
          idempotencyKey: requestId(),
        }),
      });
      for (let i = 0; i < 240; i++) {
        const state = await request<{ status: string; step: string; errorCode?: string }>(
          `/api/v1/runtime/jobs/${job.id}`,
        );
        if (state.status === "failed") throw new Error(state.errorCode || "작업 실패");
        if (state.status === "succeeded") {
          refresh();
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
      throw new Error("작업이 지연되고 있습니다. 새로고침해 상태를 확인하세요.");
    } catch (error) {
      setError(error instanceof Error ? error.message : "작업을 완료하지 못했습니다.");
    } finally {
      setBusy("");
      refresh();
    }
  }
  if (app.runtimeStatus === "removed")
    return <p>앱 실행 환경이 제거됐습니다. DB는 보존됩니다. CLI에서 다시 배포할 수 있습니다.</p>;
  return (
    <div className="runtime-controls">
      <p>{app.imageId?.slice(0, 22)}…</p>
      <div>
        {(app.runtimeStatus === "running" ? ["stop", "restart", "remove"] : ["start", "remove"]).map((action) => (
          <Button key={action} disabled={!!busy} onClick={() => control(action)}>
            {labels[action]}
          </Button>
        ))}
      </div>
      {busy && <p role="status">Docker 작업을 처리하고 있습니다…</p>}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
