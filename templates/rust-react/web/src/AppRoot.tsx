import { useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";
import "./app.css";
import { Panel } from "@miy/ui";
import { getPing } from "./api";

export default function AppRoot({ runtime }: AppProps) {
  const { api, location, navigate } = runtime;
  const [status, setStatus] = useState("API 연결 확인 중…");
  // App-session data. Route-specific effects use the relevant route parameter.
  useEffect(() => {
    const controller = new AbortController();
    void getPing({ api }, controller.signal).then(
      ({ message }) => { if (!controller.signal.aborted) setStatus(message); },
      () => { if (!controller.signal.aborted) setStatus("API 연결을 확인할 수 없습니다."); },
    );
    return () => controller.abort();
  }, [api]);
  const detail = location.pathname === "/detail";
  return (
    <Panel title="새 앱">
      <p role="status">{status}</p>
      <nav aria-label="앱 메뉴">
        <button type="button" aria-current={!detail ? "page" : undefined} onClick={() => navigate("/")}>
          개요
        </button>
        <button type="button" aria-current={detail ? "page" : undefined} onClick={() => navigate("/detail")}>
          상세
        </button>
      </nav>
      <section aria-label="본문">
        {detail ? <p>상세 화면입니다.</p> : <p>개요 화면입니다.</p>}
      </section>
    </Panel>
  );
}
