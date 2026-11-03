import { useEffect, useState } from "react";
import { useLocation } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { Button, Panel } from "@miy/ui";

export function LoginRecovery() {
  const reason = new URLSearchParams(window.location.search).get("login");
  const messages: Record<string, string> = {
    expired:
      "로그인 요청이 만료됐거나 이미 처리됐습니다. 아래 로그인 버튼으로 새 로그인을 시작해 주세요.",
    forbidden:
      "이 계정은 플랫폼에 접근할 수 없습니다. 관리자에게 계정 상태와 권한을 확인해 주세요.",
    unavailable:
      "로그인 서버에 연결하지 못했습니다. 잠시 후 아래 로그인 버튼으로 다시 시도해 주세요.",
  };
  return reason && messages[reason] ? (
    <p role="alert">{messages[reason]}</p>
  ) : null;
}

type User = { name: string; csrf: string };
type DeviceRequest = {
  status: "pending" | "approved" | "denied" | "consumed" | "expired";
  expiresIn: number;
};

export function CliDeviceApproval({ user }: { user: User | null | undefined }) {
  const location = useLocation();
  const id = location.pathname.split("/").at(-1);
  const [code, setCode] = useState("");
  const [result, setResult] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [metadata, setMetadata] = useState<DeviceRequest>();
  const [deadline, setDeadline] = useState(0);
  const [seconds, setSeconds] = useState(0);
  const [reload, setReload] = useState(0);

  function updateMetadata(value: DeviceRequest) {
    if (
      !Number.isFinite(value.expiresIn) ||
      value.expiresIn < 0 ||
      value.expiresIn > 300 ||
      !["pending", "approved", "denied", "consumed", "expired"].includes(
        value.status,
      )
    ) {
      throw new Error("invalid_request_metadata");
    }
    setMetadata(value);
    setDeadline(Date.now() + value.expiresIn * 1000);
    setSeconds(value.expiresIn);
  }

  useEffect(() => {
    if (!user) return;
    const controller = new AbortController();
    setMetadata(undefined);
    setCode("");
    setResult("");
    setError("");
    request<DeviceRequest>(`/api/v1/cli/devices/${id}`, undefined, {
      signal: controller.signal,
    })
      .then(updateMetadata)
      .catch((failure: Error) => {
        if (controller.signal.aborted) return;
        if (failure.message === "not_found")
          updateMetadata({ status: "expired", expiresIn: 0 });
        else
          setError(
            "요청 정보를 불러오지 못했습니다. 연결 상태를 확인하고 다시 확인해 주세요.",
          );
      });
    return () => controller.abort();
  }, [user?.csrf, id, reload]);

  useEffect(() => {
    if (!deadline || metadata?.status !== "pending") return;
    const timer = window.setInterval(
      () => setSeconds(Math.max(0, Math.ceil((deadline - Date.now()) / 1000))),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [deadline, metadata?.status]);

  const ended = metadata && (metadata.status !== "pending" || seconds === 0);
  async function decide(decision: "approve" | "deny") {
    if (!user || busy || !metadata || ended) return;
    setBusy(true);
    setError("");
    try {
      await request(`/api/v1/cli/devices/${id}/decision`, user.csrf, {
        method: "POST",
        body: JSON.stringify({ userCode: code, decision }),
      });
      setResult(
        decision === "approve"
          ? "CLI 연결을 승인했습니다. 터미널로 돌아가세요."
          : "CLI 연결을 거절했습니다.",
      );
    } catch (failure) {
      if (
        failure instanceof Error &&
        failure.message === "login_request_expired_or_decided"
      ) {
        try {
          updateMetadata(
            await request<DeviceRequest>(`/api/v1/cli/devices/${id}`),
          );
        } catch (readFailure) {
          if (
            readFailure instanceof Error &&
            readFailure.message === "not_found"
          ) {
            updateMetadata({ status: "expired", expiresIn: 0 });
          } else {
            setError(
              "요청 상태를 다시 확인하지 못했습니다. 연결을 확인한 뒤 다시 시도해 주세요.",
            );
            return;
          }
        }
        setError(
          "요청이 이미 처리됐거나 코드가 일치하지 않습니다. 터미널에 표시된 최신 코드를 확인해 주세요.",
        );
      } else {
        setError(
          "승인하지 못했습니다. 로그인 상태와 연결을 확인하고 다시 시도해 주세요.",
        );
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel title="CLI 로그인 승인">
      <div className="cli-approval">
        <p>
          서버: <strong>{window.location.origin}</strong>
        </p>
        <p>
          <strong>{user?.name}</strong> 계정의 권한으로 CLI를 연결합니다.
        </p>
        <p>직접 시작한 요청인지 확인하고 터미널에 표시된 코드를 입력하세요.</p>
        <p>
          요청 ID: <code>{id}</code>
        </p>
        {result ? (
          <p role="status">{result}</p>
        ) : ended ? (
          <div role="status">
            <p>
              {metadata?.status === "approved" ||
              metadata?.status === "consumed"
                ? "이미 승인한 요청입니다. 터미널에서 결과를 확인하세요."
                : metadata?.status === "denied"
                  ? "거절한 요청입니다."
                  : "로그인 요청이 만료됐거나 더 이상 사용할 수 없습니다."}
            </p>
            <p>
              새 요청이 필요하면 터미널에서 아래 명령을 실행하고, 새로 표시된
              승인 주소와 코드를 사용하세요.
            </p>
            <pre>miy auth login</pre>
            <p>
              여러 서버를 사용하는 경우{" "}
              <code>miy --context 연결이름 auth login</code>으로 해당 서버를
              선택하세요.
            </p>
          </div>
        ) : metadata ? (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void decide("approve");
            }}
          >
            <p>
              남은 승인 시간:{" "}
              <strong role="timer">
                {Math.floor(seconds / 60)}분 {seconds % 60}초
              </strong>
            </p>
            <label>
              인증 코드{" "}
              <input
                name="userCode"
                value={code}
                onChange={(event) => setCode(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                maxLength={32}
                required
              />
            </label>
            <div className="cli-approval-actions">
              <Button type="submit" disabled={busy || !code.trim()}>
                이 계정으로 CLI 연결 승인
              </Button>
              <Button
                type="button"
                disabled={busy || !code.trim()}
                onClick={() => void decide("deny")}
              >
                거절
              </Button>
            </div>
            <p>연결 후에는 CLI의 로그인 세션 관리에서 철회할 수 있습니다.</p>
          </form>
        ) : (
          !error && <p role="status">요청 정보를 불러오는 중…</p>
        )}
        {error && !ended && !result && (
          <div role="alert">
            <p>{error}</p>
            {!metadata && (
              <Button onClick={() => setReload(reload + 1)}>
                요청 다시 확인
              </Button>
            )}
          </div>
        )}
      </div>
    </Panel>
  );
}
