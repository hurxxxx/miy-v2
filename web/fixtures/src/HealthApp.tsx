import { useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";

// A normal SDK consumer: loading data depends on the API callback identity.
export default function HealthApp({ runtime }: AppProps) {
  const [title, setTitle] = useState("");
  const [saved, setSaved] = useState("");
  const [error, setError] = useState("");
  const [progress, setProgress] = useState("");
  useEffect(() => {
    let active = true;
    runtime
      .api<{ title: string }>("/document")
      .then((data) => {
        if (active) {
          setTitle(data.title);
          setSaved(data.title);
        }
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [runtime.api]);
  // Unsaved edits ask the host before leaving the app or signing out.
  useEffect(
    () =>
      runtime.setNavigationState({
        dirty: title !== saved,
        message: "저장하지 않은 제목이 있습니다. 이동할까요?",
      }),
    [runtime.setNavigationState, title, saved],
  );
  return (
    <section>
      <output data-testid="installation">{runtime.installationId}</output>
      <input
        aria-label="문서 제목"
        value={title}
        onChange={(event) => setTitle(event.target.value)}
      />
      <button
        onClick={() => {
          void runtime.api("/probe").catch((error) => setError(String(error)));
        }}
      >
        API 확인
      </button>
      <button
        onClick={() => {
          setError("");
          void runtime
            .api("/document", { method: "PUT", body: JSON.stringify({ title }) })
            .then(() => setSaved(title))
            .catch((error) => setError(String(error)));
        }}
      >
        저장
      </button>
      {/* An upload that reports its progress through fileApi. */}
      <button
        onClick={() => {
          void runtime
            .fileApi("/upload", {
              method: "PUT",
              body: new Blob([new Uint8Array(256 * 1024)]),
              onUploadProgress: (sent, total) => setProgress(`${sent}/${total}`),
            })
            .then((response) => setProgress((value) => `${value} 완료 ${response.status}`))
            .catch((error) => setError(String(error)));
        }}
      >
        업로드
      </button>
      <output data-testid="upload-progress">{progress}</output>
      {/* The app's own route change (runtime.navigate) inside itself. */}
      <button onClick={() => runtime.navigate("/next")}>다음 화면</button>
      <output data-testid="api-error">{error}</output>
    </section>
  );
}
