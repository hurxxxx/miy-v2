import { useEffect, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { useLocation } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { Button, Panel } from "../ui";
import { useSurface } from "../data/session";

export function LoginRecovery() {
  const { t } = useTranslation("portal");
  const reason = new URLSearchParams(window.location.search).get("login");
  const messages: Record<string, string> = {
    expired: t("loginRecovery.expired"),
    forbidden: t("loginRecovery.forbidden"),
    unavailable: t("loginRecovery.unavailable"),
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

// A name inside a translated sentence. Trans replaces a component's children, so
// the name is a prop: it stays plain text and is never parsed as markup.
function Name({ value }: { value?: string }) {
  return <strong>{value}</strong>;
}

export function CliDeviceApproval({ user }: { user: User | null | undefined }) {
  const { t } = useTranslation("portal");
  const { portalOrigin, consoleOrigin } = useSurface();
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
          setError(t("cliDeviceApproval.loadFailed"));
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
        t(decision === "approve" ? "cliDeviceApproval.approved" : "cliDeviceApproval.denied"),
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
            setError(t("cliDeviceApproval.recheckFailed"));
            return;
          }
        }
        setError(t("cliDeviceApproval.alreadyDecided"));
      } else {
        setError(t("cliDeviceApproval.approveFailed"));
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel title={t("cliDeviceApproval.title")}>
      <div className="cli-approval">
        <p>
          {t("cliDeviceApproval.server")} <strong>{portalOrigin}</strong>
        </p>
        <p>{t("cliDeviceApproval.approvalOrigin")} <strong>{consoleOrigin}</strong></p>
        <p>
          <Trans t={t} i18nKey="cliDeviceApproval.connectAs" components={{ name: <Name value={user?.name} /> }} />
        </p>
        <p>{t("cliDeviceApproval.verify")}</p>
        <p>
          {t("cliDeviceApproval.requestId")} <code>{id}</code>
        </p>
        {result ? (
          <p role="status">{result}</p>
        ) : ended ? (
          <div role="status">
            <p>
              {t(
                metadata?.status === "approved" ||
                  metadata?.status === "consumed"
                  ? "cliDeviceApproval.alreadyApproved"
                  : metadata?.status === "denied"
                    ? "cliDeviceApproval.deniedRequest"
                    : "cliDeviceApproval.expired",
              )}
            </p>
            <p>
              {t("cliDeviceApproval.newRequest")}
            </p>
            <pre>miy auth login</pre>
            <p>
              <Trans t={t} i18nKey="cliDeviceApproval.multipleServers" components={{ code: <code /> }} />
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
              {t("cliDeviceApproval.remaining")}{" "}
              <strong role="timer">
                {t("cliDeviceApproval.timer", { minutes: Math.floor(seconds / 60), seconds: seconds % 60 })}
              </strong>
            </p>
            <label>
              {t("cliDeviceApproval.code")}{" "}
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
                {t("cliDeviceApproval.approve")}
              </Button>
              <Button
                type="button"
                disabled={busy || !code.trim()}
                onClick={() => void decide("deny")}
              >
                {t("cliDeviceApproval.deny")}
              </Button>
            </div>
            <p>{t("cliDeviceApproval.revokeLater")}</p>
          </form>
        ) : (
          !error && <p role="status">{t("cliDeviceApproval.loading")}</p>
        )}
        {error && !ended && !result && (
          <div role="alert">
            <p>{error}</p>
            {!metadata && (
              <Button onClick={() => setReload(reload + 1)}>
                {t("cliDeviceApproval.recheck")}
              </Button>
            )}
          </div>
        )}
      </div>
    </Panel>
  );
}
