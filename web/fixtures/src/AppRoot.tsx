import { useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";
import { Panel, Button } from "@miy/ui";
import styles from "./app.module.css";
export default function AppRoot({
  runtime,
  title,
}: AppProps & { title: string }) {
  const [count, setCount] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setCount((c) => c + 1), 1000);
    return () => clearInterval(timer);
  }, []);
  if (runtime.location.pathname === "/crash")
    throw new Error("intentional_fixture_render_failure");
  const detail = runtime.location.pathname.startsWith("/detail/");
  return (
    <div className={styles.app} data-testid="app-root">
      <Panel title={title}>
        <p data-testid="route">
          {runtime.location.pathname}
          {runtime.location.search}
        </p>
        <p>설치: {runtime.installationId}</p>
        <p>릴리스: {runtime.releaseId}</p>
        {detail ? (
          <>
            <h3>분류 상세</h3>
            <p>사무 장비 · 합성 검증 데이터</p>
            <Button onClick={() => runtime.navigate("/")}>목록으로</Button>
          </>
        ) : (
          <>
            <h3>분류 목록</h3>
            <Button onClick={() => runtime.navigate("/detail/1?view=full")}>
              사무 장비 보기
            </Button>
          </>
        )}
        <p>화면 실행: {count}초</p>
      </Panel>
    </div>
  );
}
