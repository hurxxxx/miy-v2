import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useRouter } from "@tanstack/react-router";
import { appTarget, request, type AppFrameLaunch, type Installation } from "@miy/sdk";
import {
  envelope,
  readFrameMessage,
  renewalFailure,
  type InputState,
  type ParentMessage,
} from "@miy/frame-protocol";
import { useDisplaySettings } from "../data/settings";
import { COMMAND_PALETTE_EVENT } from "../components/CommandPalette";
import { i18n } from "../platform/i18n";
import { Button, Loading, Notice, Panel } from "../ui";
import { closeFrame, releaseClosedDocumentFrames, rememberClosedDocumentFrame } from "./frame-slots";

/**
 * A side-panel frame keeps its own location (docs/APP-FRAMES.md "보조 프레임"):
 * the portal address follows the main frame only.
 */
export interface PanelRouting {
  location: { pathname: string; search: string; hash: string };
  navigate: (path: string, replace: boolean) => void;
  /** The frame's accessible name, told apart from the main frame of the same app. */
  title: string;
}

/**
 * Set while the portal follows an app's own `runtime.navigate`: the app chose
 * that move knowing its input state, and its frame stays, so the portal's leave
 * check does not ask (browser history and portal links still ask).
 */
export const appNavigation = { active: false };

/** The frame's last reported input state, read synchronously by leave checks. */
export interface FrameInput {
  state: InputState;
  message?: string;
}

const SANDBOX =
  "allow-scripts allow-same-origin allow-forms allow-downloads allow-modals allow-popups allow-popups-to-escape-sandbox";

/**
 * Browser features delegated to the frame (iframe `allow`): only those the
 * release requested and an administrator approved, as Core lists them in the
 * launch (docs/APP-FRAMES.md 3.5). Set before the launch form navigates.
 */
const DELEGABLE = new Set(["clipboard-write", "fullscreen"]);
function frameAllow(launch: AppFrameLaunch | null | undefined): string | undefined {
  if (!launch) return undefined;
  try {
    const origin = new URL(launch.frameOrigin);
    if (origin.origin !== launch.frameOrigin ||
        !["https:", "http:"].includes(origin.protocol)) return undefined;
    const allowed = (launch.allow ?? [])
      .filter((feature) => DELEGABLE.has(feature))
      .map((feature) => `${feature} ${origin.origin}`);
    return allowed.length ? allowed.join("; ") : undefined;
  } catch {
    return undefined;
  }
}

/** The leave-check message for a frame's state, in the current language; undefined when it may close. */
export function leaveMessage(input: FrameInput): string | undefined {
  if (input.state === "clean") return undefined;
  return input.state === "dirty"
    ? input.message || i18n.t("portal:appFrame.leaveDirty")
    : i18n.t("portal:appFrame.leaveUnknown");
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
  panel,
}: {
  app: Installation;
  basePath: string;
  csrf: string;
  input: React.MutableRefObject<FrameInput>;
  onReleaseChanged?: () => void;
  /** Present for a side panel: its own location instead of the portal router's. */
  panel?: PanelRouting;
}) {
  const { t } = useTranslation("portal");
  const router = useRouter();
  const location = useLocation();
  const frame = useRef<HTMLIFrameElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const [run, setRun] = useState(0);
  const [launch, setLaunch] = useState<AppFrameLaunch>();
  const [failure, setFailure] = useState<string>();
  const name = `miy-app-${app.id}-${run}${panel ? "-panel" : ""}`;
  const relative = panel
    ? panel.location
    : {
        pathname: location.pathname.slice(basePath.length) || "/",
        search: location.searchStr,
        hash: location.hash ? `#${location.hash}` : "",
      };
  const panelRouting = useRef(panel);
  panelRouting.current = panel;
  const current = useRef(relative);
  current.current = relative;
  // Messages go only to a frame that announced itself (not its initial about:blank).
  const ready = useRef(false);

  // A new launch for each run: new frame, new code, state unknown until reported.
  useEffect(() => {
    let active = true;
    let frameId: string | undefined;
    const close = (id: string) => {
      void closeFrame(id, csrf).catch(() => undefined);
    };
    const release = () => {
      if (!frameId) return;
      close(frameId);
      frameId = undefined;
    };
    const pageHidden = (event: PageTransitionEvent) => {
      if (!event.persisted) {
        active = false;
        if (frameId) rememberClosedDocumentFrame(frameId);
        release();
      }
    };
    window.addEventListener("pagehide", pageHidden);
    ready.current = false;
    input.current = { state: "unknown" };
    setLaunch(undefined);
    setFailure(undefined);
    releaseClosedDocumentFrames(csrf)
      .then(() => active ? request<AppFrameLaunch>("/api/v1/app-frames", csrf, {
        method: "POST",
        body: JSON.stringify({ installationId: app.id }),
      }) : undefined)
      .then((value) => {
        if (!value) return;
        if (active) {
          frameId = value.frameId;
          setLaunch(value);
        } else close(value.frameId);
      })
      .catch((error: Error) => {
        if (!active) return;
        // No frame ran, so there is no input to lose: leaving needs no question.
        input.current = { state: "clean" };
        setFailure(error.message);
      });
    return () => {
      active = false;
      window.removeEventListener("pagehide", pageHidden);
      release();
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

  // Display settings: the launch form carries the first value, later changes follow.
  const { settings } = useDisplaySettings();
  const latestSettings = useRef(settings);
  latestSettings.current = settings;
  useEffect(() => {
    send({ type: "settings.changed", payload: settings });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings, launch]);

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
          send({ type: "settings.changed", payload: latestSettings.current });
          break;
        case "route.navigate": {
          let href: string;
          try {
            href = appTarget(basePath, message.payload.path);
          } catch {
            return;
          }
          if (panelRouting.current) {
            panelRouting.current.navigate(message.payload.path, message.payload.replace);
            break;
          }
          appNavigation.active = true;
          void router
            .navigate({ to: href as "/", replace: message.payload.replace })
            .finally(() => {
              appNavigation.active = false;
            });
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
          if (onReleaseChanged) onReleaseChanged();
          else setFailure("release_changed_reload_required");
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
        case "shortcut":
          // Keys typed in the frame never reach this document; the shell listens.
          window.dispatchEvent(new CustomEvent(COMMAND_PALETTE_EVENT));
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
        <Panel title={t(launch ? "appFrame.checkConnectionTitle" : "appFrame.cannotOpenTitle")}>
          <Notice
            role="alert"
            tone="danger"
            actions={
              <Button size="sm" onClick={restart}>
                {t("appFrame.reopen")}
              </Button>
            }
          >
            <p>
              {t(
                failure === "frame_session_ended"
                  ? "appFrame.sessionEnded"
                  : failure === "frame_access_denied"
                    ? "appFrame.accessDenied"
                    : failure === "frame_reload_required" || failure === "release_changed_reload_required"
                      ? "appFrame.reloadRequired"
                      : "appFrame.failed",
              )}
            </p>
          </Notice>
        </Panel>
      )}
      {!launch && !failure && <Loading>{t("appFrame.opening")}</Loading>}
      <iframe
        key={name}
        ref={frame}
        name={name}
        title={panel?.title ?? app.name}
        className={panel ? "app-frame app-frame-panel" : "app-frame"}
        sandbox={SANDBOX}
        referrerPolicy="no-referrer"
        allow={frameAllow(launch)}
        hidden={!launch}
      />
      {launch && (
        <form ref={form} method="post" action={launch.launchUrl} target={name} hidden>
          <input type="hidden" name="code" value={launch.code} />
          <input type="hidden" name="pathname" value={current.current.pathname} />
          <input type="hidden" name="search" value={current.current.search} />
          <input type="hidden" name="hash" value={current.current.hash} />
          <input type="hidden" name="basePath" value={basePath} />
          <input type="hidden" name="theme" value={latestSettings.current.theme} />
          <input type="hidden" name="locale" value={latestSettings.current.locale} />
          <input type="hidden" name="timeZone" value={latestSettings.current.timeZone} />
          <input type="hidden" name="dateFormat" value={latestSettings.current.dateFormat} />
        </form>
      )}
    </>
  );
}
