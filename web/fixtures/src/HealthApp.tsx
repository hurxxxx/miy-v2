import { useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";

// A normal SDK consumer: loading data depends on the API callback identity.
export default function HealthApp({ runtime }: AppProps) {
  const [title, setTitle] = useState("");
  const [saved, setSaved] = useState("");
  const [error, setError] = useState("");
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
      <output data-testid="api-error">{error}</output>
    </section>
  );
}
