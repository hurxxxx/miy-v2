import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ApiError, request } from "@miy/sdk";

type Reply = { port: number; state: string; code: string };

export function PreviewLogin({ id, csrf }: { id: string; csrf: string }) {
  const { t } = useTranslation("portal");
  const started = useRef(false);
  const [message, setMessage] = useState<"working" | "forbidden" | "expired" | "failed">("working");
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    request<Reply>(`/api/v1/preview/browser/${id}/authorize`, csrf, { method: "POST" })
      .then(({ port, state, code }) => {
        if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("invalid_port");
        const callback = new URL(`http://127.0.0.1:${port}/__miy/login/callback`);
        callback.search = new URLSearchParams({ state, code }).toString();
        window.location.replace(callback.href);
      })
      .catch((error: unknown) => {
        setMessage(error instanceof ApiError && error.status === 403 ? "forbidden"
          : error instanceof ApiError && error.status === 401 ? "expired" : "failed");
      });
  }, [id, csrf]);
  return <main className="boot"><p role={message === "working" ? "status" : "alert"}>{t(`previewLogin.${message}`)}</p></main>;
}
