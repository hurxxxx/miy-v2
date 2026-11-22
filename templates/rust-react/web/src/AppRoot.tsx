import { useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";
import "./app.css";
import { Panel } from "@miy/ui";
import { getPing } from "./api";

export default function AppRoot({ runtime }: AppProps) {
  const [status, setStatus] = useState("API 연결 확인 중…");
  const { api } = runtime;
  useEffect(() => {
    const controller = new AbortController();
    void getPing({ api }, controller.signal).then(
      ({ message }) => { if (!controller.signal.aborted) setStatus(message); },
      () => { if (!controller.signal.aborted) setStatus("API 연결을 확인할 수 없습니다."); },
    );
    return () => controller.abort();
  }, [api]);
  return <Panel title="새 앱"><p role="status">{status}</p></Panel>;
}
