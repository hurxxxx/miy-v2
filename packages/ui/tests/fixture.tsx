import { StrictMode, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import "./app.css";
import "../../tokens/tokens.css";
import {
  Badge,
  Button,
  DirectoryPicker,
  Input,
  Panel,
  useConfirm,
  usePrompt,
  ShareEditor,
  FileTransferPanel,
  type FileService,
  type FileTransferState,
  type ShareProvider,
  type DirectoryOption,
  type DirectoryProvider,
} from "../src";

declare global {
  interface Window {
    uiFixture: {
      requests: AbortSignal[];
      unmountFeedback?: () => void;
      result?: string;
      dirty?: boolean;
      pendingRequest?: string | null;
      transfers?: readonly FileTransferState[];
    };
    uiInitialTransfers?: FileTransferState[];
    uiInitialShareRequest?: string;
  }
}
window.uiFixture = { requests: [] };
function Feedback() {
  const { confirm, confirmDialog } = useConfirm();
  const { prompt, promptDialog } = usePrompt();
  const [result, setResult] = useState("");
  const record = (value: unknown) => {
    const text = String(value);
    window.uiFixture.result = text;
    setResult(text);
  };
  return (
    <>
      <Button
        onClick={async () =>
          record(
            await confirm({
              title: "변경 확인",
              description: "이 변경을 적용할까요?",
              confirmLabel: "확인",
              cancelLabel: "취소",
            }),
          )
        }
      >
        확인 대화상자 열기
      </Button>
      <Button
        onClick={async () =>
          record(
            await prompt({
              title: "이름 변경",
              placeholder: "이름",
              defaultValue: "기존 이름",
              submitLabel: "확인",
              cancelLabel: "취소",
            }),
          )
        }
      >
        이름 입력 열기
      </Button>
      <Button
        onClick={() => {
          const labels = { confirmLabel: "확인", cancelLabel: "취소" };
          void confirm({
            title: "이전 확인",
            description: "이전 요청",
            ...labels,
          }).then((value) => record(`previous:${value}`));
          void confirm({ title: "새 확인", description: "새 요청", ...labels });
        }}
      >
        동시 확인
      </Button>
      <output data-testid="feedback-result">{result}</output>
      {confirmDialog}
      {promptDialog}
    </>
  );
}
function Fixture() {
  const [value, setValue] = useState<DirectoryOption[]>([
    { kind: "user", id: "locked", name: "고정 사용자" },
  ]);
  const [visible, setVisible] = useState(true);
  const [feedbackVisible, setFeedbackVisible] = useState(true);
  const [dark, setDark] = useState(false);
  // The frame runtime sets both on the document root; so does this fixture.
  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.dataset.miyTheme = dark ? "dark" : "light";
  }, [dark]);
  window.uiFixture.unmountFeedback = () => setFeedbackVisible(false);
  const provider = useMemo<DirectoryProvider>(
    () => ({
      async load({ signal, ...query }) {
        window.uiFixture.requests.push(signal);
        const url = new URL("/fixture-directory", window.location.origin);
        for (const [name, value] of Object.entries(query))
          if (value !== undefined) url.searchParams.set(name, String(value));
        // This mode deliberately ignores abort to verify that obsolete data is still discarded.
        const ignoreAbort = new URL(location.href).searchParams.has(
          "ignore-abort",
        );
        const response = await fetch(url, {
          signal: ignoreAbort ? undefined : signal,
        });
        if (!response.ok) throw new Error("directory_unavailable");
        return response.json();
      },
    }),
    [],
  );
  return (
    <main
      style={{
        maxWidth: 680,
        margin: "32px auto",
        padding: 16,
        background: "var(--ui-color-surface)",
      }}
    >
      <Panel
        title="사용자·그룹 및 기본 UI"
        actions={<Badge tone="accent">공통 컴포넌트</Badge>}
      >
        <div style={{ display: "grid", gap: 16 }}>
          <label style={{ display: "grid", gap: 6 }}>
            검증용 입력
            <Input id="outside" placeholder="문자 입력" />
          </label>
          {visible ? (
            <DirectoryPicker
              label="공유 대상"
              provider={provider}
              value={value}
              onChange={setValue}
              currentUserId="me"
              lockedKeys={new Set(["user:locked"])}
              pageSize={3}
            />
          ) : null}
          <output data-testid="selection">
            {value.map((item) => `${item.kind}:${item.id}`).join(",")}
          </output>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <Button onClick={() => setVisible((value) => !value)}>
              선택기 {visible ? "숨기기" : "표시"}
            </Button>
            <Button onClick={() => setDark((value) => !value)}>
              테마 전환
            </Button>
          </div>
          {feedbackVisible ? <Feedback /> : null}
        </div>
      </Panel>
    </main>
  );
}
function ShareFixture() {
  const provider = useMemo<ShareProvider>(() => {
    const send = async (path: string, signal: AbortSignal, input?: unknown) => {
      const response = await fetch(path, {
        method: input ? "POST" : "GET",
        signal,
        headers: input ? { "content-type": "application/json" } : undefined,
        body: input ? JSON.stringify(input) : undefined,
      });
      const data = await response.json();
      if (!response.ok) throw { code: data.error?.code };
      return data;
    };
    return {
      load: ({ signal }) => send("/fixture-share", signal),
      replace: (input, { signal }) => send("/fixture-share", signal, input),
      lookupResult: (id, { signal }) =>
        send(`/fixture-share/results/${id}`, signal),
    };
  }, []);
  const directory = useMemo<DirectoryProvider>(
    () => ({
      load: async ({ kind }) => ({
        items:
          kind === "group"
            ? [{ kind: "group", id: "design", name: "설계 그룹" }]
            : [{ kind: "user", id: "new", name: "새 사용자" }],
        nextCursor: null,
      }),
    }),
    [],
  );
  return (
    <main style={{ maxWidth: 640, margin: "32px auto", padding: 16 }}>
      <Panel title="공유 설정 검증">
        <ShareEditor
          provider={provider}
          directoryProvider={directory}
          initialPendingRequestId={window.uiInitialShareRequest}
          onDirtyChange={(value) => {
            window.uiFixture.dirty = value;
          }}
          onPendingRequestChange={(value) => {
            window.uiFixture.pendingRequest = value;
          }}
        />
      </Panel>
    </main>
  );
}
function FileFixture() {
  const [visible, setVisible] = useState(true);
  const service = useMemo<FileService>(() => {
    const request = async (
      path: string,
      signal: AbortSignal,
      init: RequestInit = {},
    ) => {
      const response = await fetch(`/fixture-files${path}`, {
        ...init,
        signal,
      });
      if (!response.ok) {
        const data = await response.json();
        throw { code: data.error?.code };
      }
      return response;
    };
    const json = async (path: string, signal: AbortSignal, input?: unknown) =>
      (
        await request(
          path,
          signal,
          input
            ? {
                method: "POST",
                headers: { "content-type": "application/json" },
                body: JSON.stringify(input),
              }
            : {},
        )
      ).json();
    return {
      create: (input, signal) => json("", signal, input),
      upload: async (id, file, { signal, onProgress }) => {
        onProgress?.({ loaded: file.size / 2, total: file.size });
        const effectiveSignal = new URL(location.href).searchParams.has(
          "ignore-upload-abort",
        )
          ? new AbortController().signal
          : signal;
        const response = await request(`/${id}/bytes`, effectiveSignal, {
          method: "PUT",
          body: file,
        });
        onProgress?.({ loaded: file.size, total: file.size });
        return response.json();
      },
      byRequest: (id, signal) => json(`/by-request/${id}`, signal),
      get: (id, signal) => json(`/${id}`, signal),
      attach: (id, input, signal) => json(`/${id}/attach`, signal, input),
      cancel: (id, signal) => json(`/${id}/cancel`, signal, {}),
      download: async (id, signal) =>
        (await request(`/${id}/download`, signal)).blob(),
    };
  }, []);
  return (
    <main style={{ maxWidth: 720, margin: "32px auto", padding: 16 }}>
      <Panel title="파일 UI 검증">
        {visible ? (
          <FileTransferPanel
            service={service}
            reference="document:42"
            initialTransfers={window.uiInitialTransfers}
            onStateChange={(value) => {
              window.uiFixture.transfers = value;
            }}
            onDirtyChange={(value) => {
              window.uiFixture.dirty = value;
            }}
          />
        ) : null}
        <Button onClick={() => setVisible((value) => !value)}>
          파일 UI {visible ? "숨기기" : "표시"}
        </Button>
      </Panel>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {new URL(location.href).searchParams.get("view") === "share" ? (
      <ShareFixture />
    ) : new URL(location.href).searchParams.get("view") === "files" ? (
      <FileFixture />
    ) : (
      <Fixture />
    )}
  </StrictMode>,
);
