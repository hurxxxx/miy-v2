import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "@tanstack/react-router";
import { ExternalLink, X } from "lucide-react";
import { createNavigationGuards, type Installation, type InstallationPanel } from "@miy/sdk";
import { AppFrame, leaveMessage, type FrameInput } from "../../app/AppFrame";
import { activeGuards } from "../../lib/navigation-guards";
import { environmentPath } from "../../lib/paths";

export type OpenPanel = { app: Installation; panel: InstallationPanel };

export const panelKey = (open: OpenPanel) => `${open.app.appId}/${open.panel.id}`;

/** Narrow screens open the app instead of a panel (docs/APP-FRAMES.md "보조 프레임"). */
export const PANEL_MEDIA = "(min-width: 1024px)";

function locationOf(path: string) {
  const url = new URL(path, "https://miy.invalid");
  return { pathname: url.pathname, search: url.search, hash: url.hash };
}

/**
 * The secondary frame (docs/APP-FRAMES.md "보조 프레임"): one app's panel screen
 * beside the main frame. Its location stays in memory; the address follows the
 * main frame only. The shell asks before closing or switching when the panel
 * reports unsaved input (`input`), and sign-out asks it like every open app.
 */
export function SidePanel({
  open,
  environment,
  csrf,
  input,
  onClose,
}: {
  open: OpenPanel;
  environment: string;
  csrf: string;
  input: React.MutableRefObject<FrameInput>;
  onClose: (force?: boolean) => void;
}) {
  const { t } = useTranslation("portal");
  const navigate = useNavigate();
  const [run, setRun] = useState(0);
  const [location, setLocation] = useState(() => locationOf(open.panel.path));
  const basePath = environmentPath(environment, open.app.appId);
  const guards = useMemo(() => {
    const created = createNavigationGuards();
    created.register(() => leaveMessage(input.current));
    return created;
    // A new frame run is a new guard: the old one's state no longer applies.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open.app.id, open.panel.id, run, input]);
  useEffect(() => {
    activeGuards.add(guards);
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (guards.message()) event.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => {
      window.removeEventListener("beforeunload", beforeUnload);
      activeGuards.delete(guards);
    };
  }, [guards]);
  const title = t("panels.label", { title: open.panel.title });
  return (
    <aside className="side-panel" aria-label={title}>
      <header className="side-panel-header">
        <div className="side-panel-title">
          <strong>{open.panel.title}</strong>
          <span>{open.app.name}</span>
        </div>
        <button
          type="button"
          className="side-panel-button"
          aria-label={t("panels.openInApp")}
          title={t("panels.openInApp")}
          onClick={() => {
            const message = leaveMessage(input.current);
            if (message && !window.confirm(message)) return;
            void navigate({ to: `${basePath}${location.pathname}${location.search}${location.hash}` as "/" });
            onClose(true);
          }}
        >
          <ExternalLink size={15} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="side-panel-button"
          aria-label={t("panels.close")}
          title={t("panels.close")}
          onClick={() => onClose()}
        >
          <X size={16} aria-hidden="true" />
        </button>
      </header>
      <AppFrame
        key={`${open.app.id}:${open.panel.id}:${run}`}
        app={open.app}
        basePath={basePath}
        csrf={csrf}
        input={input}
        onReleaseChanged={() => setRun((value) => value + 1)}
        panel={{ location, navigate: (path) => setLocation(locationOf(path)), title }}
      />
    </aside>
  );
}
