import { useEffect, useState } from "react";
import type { AppProps } from "@miy/sdk";

// A normal SDK consumer: loading data depends on the API callback identity.
export default function HealthApp({ runtime }: AppProps) {
  const [title, setTitle] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    runtime
      .api<{ title: string }>("/document")
      .then((data) => {
        if (active) setTitle(data.title);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [runtime.api]);
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
      <output data-testid="api-error">{error}</output>
    </section>
  );
}
