import { useEffect, useRef, useState } from "react";
import { useLocation, useRouter } from "@tanstack/react-router";
import { appTarget, request, type AppFrameLaunch, type Installation } from "@miy/sdk";
import {
  envelope,
  readFrameMessage,
  renewalFailure,
  type InputState,
  type ParentMessage,
} from "@miy/frame-protocol";
import { Button, Loading, Notice, Panel } from "../ui";

/** The frame's last reported input state, read synchronously by leave checks. */
export interface FrameInput {
  state: InputState;
  message?: string;
}

const SANDBOX =
  "allow-scripts allow-same-origin allow-forms allow-downloads allow-modals allow-popups allow-popups-to-escape-sandbox";
const UNKNOWN = "앱의 저장 상태를 확인할 수 없습니다. 이동하시겠습니까?";
const DIRTY = "저장하지 않은 내용이 있습니다. 이동하시겠습니까?";

/** The leave-check message for a frame's state; undefined when it may close. */
export function leaveMessage(input: FrameInput): string | undefined {
  if (input.state === "clean") return undefined;
  return input.state === "dirty" ? input.message || DIRTY : UNKNOWN;
}

/**
 * One installation's app in its own cross-origin frame (docs/APP-FRAMES.md). The
 * portal launches the frame with a one-time code in a form POST, keeps routing,
 * and answers the frame's screen messages; it never forwards API calls.
 */
export function AppFrame({
  app,
  basePath,
  csrf,
  input,
  onReleaseChanged,
}: {
  app: Installation;
  basePath: string;
  csrf: string;
  input: React.MutableRefObject<FrameInput>;
  onReleaseChanged: () => void;
}) {
  const router = useRouter();
  const location = useLocation();
  const frame = useRef<HTMLIFrameElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const [run, setRun] = useState(0);
  const [launch, setLaunch] = useState<AppFrameLaunch>();
  const [failure, setFailure] = useState<string>();
  const name = `miy-app-${app.id}-${run}`;
  const relative = {
    pathname: location.pathname.slice(basePath.length) || "/",
    search: location.searchStr,
    hash: location.hash ? `#${location.hash}` : "",
  };
  const current = useRef(relative);
  current.current = relative;
  // Messages go only to a frame that announced itself (not its initial about:blank).
  const ready = useRef(false);

  // A new launch for each run: new frame, new code, state unknown until reported.
  useEffect(() => {
    let active = true;
    ready.current = false;
    input.current = { state: "unknown" };
    setLaunch(undefined);
    setFailure(undefined);
    request<AppFrameLaunch>("/api/v1/app-frames", csrf, {
      method: "POST",
      body: JSON.stringify({ installationId: app.id }),
    })
      .then((value) => {
        if (active) setLaunch(value);
      })
      .catch((error: Error) => {
        if (active) setFailure(error.message);
      });
    return () => {
      active = false;
    };
  }, [app.id, csrf, run, input]);

  // Submit the code into the fresh (initial about:blank) frame: no history entry.
  useEffect(() => {
    if (launch && form.current) form.current.submit();
  }, [launch]);

  const send = (message: ParentMessage) => {
    if (launch && ready.current) frame.current?.contentWindow?.postMessage(envelope(message), launch.frameOrigin);
  };

  const restart = () => {
    const message = launch && leaveMessage(input.current);
    if (message && !window.confirm(message)) return;
    setRun((value) => value + 1);
  };

  // The portal router owns the address; the frame follows it.
  useEffect(() => {
    send({ type: "route.changed", payload: relative });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [relative.pathname, relative.search, relative.hash, launch]);

  useEffect(() => {
    if (!launch) return;
    let active = true;
    const launchedWindow = frame.current?.contentWindow;
    const reply = (message: ParentMessage) => {
      // A late renewal response belongs only to the document that requested it.
      if (active && ready.current && launchedWindow === frame.current?.contentWindow)
        launchedWindow?.postMessage(envelope(message), launch.frameOrigin);
    };
    const onMessage = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow || event.origin !== launch.frameOrigin) return;
      const message = readFrameMessage(event.data);
      if (!message) return;
      switch (message.type) {
        case "frame.ready":
          ready.current = true;
          send({ type: "route.changed", payload: current.current });
          break;
        case "route.navigate": {
          let href: string;
          try {
            href = appTarget(basePath, message.payload.path);
          } catch {
            return;
          }
          void router.navigate({ to: href as "/", replace: message.payload.replace });
          break;
        }
        case "guard.state":
          input.current = { state: message.payload.state, message: message.payload.message };
          break;
        case "token.renew":
          // The renewal target is this portal's own launch, never a frame claim.
          void fetch(`/api/v1/app-frames/${launch.frameId}/renew`, {
            method: "POST",
            credentials: "same-origin",
            headers: { "x-miy-csrf": csrf },
          })
            .then(async (response) => {
              const value = await response.json().catch(() => null);
              if (!response.ok)
                reply({ type: "token.failed", payload: { reason: renewalFailure(response.status, value?.error?.code) } });
              else if (typeof value?.code === "string")
                reply({ type: "token.code", payload: { code: value.code } });
              else reply({ type: "token.failed", payload: { reason: "token_renewal_unavailable" } });
            })
            .catch(() => reply({ type: "token.failed", payload: { reason: "token_renewal_unavailable" } }));
          break;
        case "release.changed":
          onReleaseChanged();
          break;
        case "frame.relaunch":
          // A bridge message is not consent to discard dirty or unknown input.
          // Clean reloads remain automatic; other cases need the explicit button.
          if (input.current.state === "clean") setRun((value) => value + 1);
          else setFailure("frame_reload_required");
          break;
        case "frame.error":
          if (message.payload.code === "frame_session_ended" || message.payload.code === "frame_access_denied")
            setFailure(message.payload.code);
          break;
      }
    };
    window.addEventListener("message", onMessage);
    return () => {
      active = false;
      window.removeEventListener("message", onMessage);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [launch, basePath, csrf, router]);

  return (
    <>
      {failure && (
        <Panel title={launch ? "앱 연결을 확인하세요" : "앱을 열 수 없습니다"}>
          <Notice
            role="alert"
            tone="danger"
            actions={
              <Button size="sm" onClick={restart}>
                다시 열기
              </Button>
            }
          >
            <p>
              {failure === "frame_session_ended"
                ? "앱 세션이 끝났습니다. 다시 열거나 다시 로그인하세요."
                : failure === "frame_access_denied"
                  ? "앱 사용 권한을 확인할 수 없습니다. 작성 중인 내용을 확인한 뒤 다시 열어주세요."
                  : failure === "frame_reload_required"
                    ? "앱을 다시 열어야 합니다. 작성 중인 내용을 확인한 뒤 다시 열어주세요."
                    : "앱을 다시 열거나 관리자에게 문의하세요."}
            </p>
          </Notice>
        </Panel>
      )}
      {!launch && !failure && <Loading>앱을 여는 중…</Loading>}
      <iframe
        key={name}
        ref={frame}
        name={name}
        title={app.name}
        className="app-frame"
        sandbox={SANDBOX}
        referrerPolicy="no-referrer"
        hidden={!launch}
      />
      {launch && (
        <form ref={form} method="post" action={launch.launchUrl} target={name} hidden>
          <input type="hidden" name="code" value={launch.code} />
          <input type="hidden" name="pathname" value={current.current.pathname} />
          <input type="hidden" name="search" value={current.current.search} />
          <input type="hidden" name="hash" value={current.current.hash} />
          <input type="hidden" name="basePath" value={basePath} />
        </form>
      )}
    </>
  );
}
