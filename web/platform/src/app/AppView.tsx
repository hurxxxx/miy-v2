import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useRouter } from "@tanstack/react-router";
import { UI_CONTRACT, createNavigationGuards, type Installation } from "@miy/sdk";
import { Button, Loading, Notice, Panel } from "../ui";
import { useInstallations } from "../data/installations";
import { useUser } from "../data/session";
import { environmentPath, isAppPath } from "../lib/paths";
import { activeGuards } from "../lib/navigation-guards";
import { AppFrame, appNavigation, leaveMessage, type FrameInput } from "./AppFrame";

type Tab = {
  app: Installation;
  input: { current: FrameInput };
  admitted: boolean;
  failed?: boolean;
  basePath: string;
};

export function AppView({ environment, paths, onPanelOpened }: {
  environment: string; paths: Map<string, string>; onPanelOpened: (promotion: string) => void;
}) {
  const { t } = useTranslation("portal");
  const router = useRouter();
  const location = useLocation();
  const installs = useInstallations();
  const parts = location.pathname.split("/");
  const appId = isAppPath(location.pathname) ? (parts[1] === "environments" ? parts[4] : parts[2]) : undefined;
  const base = appId ? environmentPath(environment, appId) : "";
  const requested = appId ? installs.items?.find(a => a.appId === appId && a.environment === environment) : undefined;
  const available = requested && (requested.kind !== "docker_app" || requested.runtimeStatus === "running") && requested.ui.contractVersion === UI_CONTRACT;
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [limit, setLimit] = useState(false);
  const [previous, setPrevious] = useState<string>();
  const lastPage = useRef(environmentPath(environment));
  const selected = available ? tabs.find(tab => tab.app.id === requested.id) : undefined;
  // Keep the old editor active until the destination has secured a server slot.
  const active = appId && available ? (selected?.admitted ? selected.app.id : selected?.failed ? undefined : previous) : undefined;
  const href = location.pathname + location.searchStr + (location.hash ? "#" + location.hash : "");
  const locationRef = useRef(href);
  locationRef.current = href;
  useLayoutEffect(() => {
    if (!appId) {
      lastPage.current = href;
      setPrevious(undefined);
    } else if (selected?.admitted) {
      paths.set(base, href);
      lastPage.current = href;
      setPrevious(selected.app.id);
      const promotion = (location.state as { sidePanelPromotion?: string }).sidePanelPromotion;
      if (promotion) onPanelOpened(promotion);
    }
  }, [appId, selected, href, base, paths, location.state, onPanelOpened]);
  useEffect(() => {
    if (!installs.items) return;
    setTabs(old => {
      const next = old.filter(tab => {
        const current = installs.items!.find(a => a.id === tab.app.id);
        const keep = current && (current.kind !== "docker_app" || current.runtimeStatus === "running") &&
          (tab.admitted || current.id === requested?.id);
        if (!keep) paths.delete(environmentPath(environment, tab.app.appId));
        return keep;
      });
      if (available && !next.some(tab => tab.app.id === requested.id)) {
        const basePath = location.pathname.startsWith("/apps/") ? "/apps/" + requested.appId : base;
        next.push({ app: requested, basePath, input: { current: { state: "clean" } }, admitted: false });
      }
      return old.length === next.length && old.every((t,i) => t === next[i]) ? old : next;
    });
  }, [installs.items, requested, available, base, location.pathname, environment, paths]);

  const close = (tab: Tab) => {
    const message = leaveMessage(tab.input.current);
    if (message && !window.confirm(message)) return;
    const remaining = tabs.filter(t => t !== tab);
    const fallback = remaining.filter(t => t.admitted).at(-1);
    if (selected === tab || previous === tab.app.id) {
      const next = fallback && paths.get(environmentPath(environment, fallback.app.appId));
      router.history.replace(next ?? environmentPath(environment));
      setPrevious(fallback?.app.id);
    }
    paths.delete(environmentPath(environment, tab.app.appId));
    setTabs(remaining);
    setLimit(false);
  };
  return <>
    {tabs.some(t => t.admitted) && <nav className="open-app-tabs" aria-label={t("appFrame.tabs")}>
      {tabs.filter(t => t.admitted).map(tab => <div key={tab.app.id} className="open-app-tab" data-active={tab.app.id === active}>
        <button type="button" aria-current={tab.app.id === active ? "page" : undefined} onClick={() => {
          const key = environmentPath(environment, tab.app.appId);
          router.history.push(paths.get(key) ?? key);
        }}>{tab.app.name}{tab.input.current.state === "dirty" && <span aria-label={t("appFrame.unsaved")}> ●</span>}</button>
        <button type="button" aria-label={t("appFrame.closeTab", { name: tab.app.name })} onClick={() => close(tab)}>×</button>
      </div>)}
    </nav>}
    {limit && <Notice tone="warning" role="alert">{t("appFrame.full")}</Notice>}
    {appId && !available && (installs.items === null
      ? installs.status === "error" ? <Panel title={t("appView.loadFailedTitle")}><Button onClick={() => void installs.refresh()}>{t("common:actions.retry")}</Button></Panel> : <Loading />
      : <Panel title={t(!requested ? "appView.noAccessTitle" : requested.ui.contractVersion !== UI_CONTRACT ? "appView.unsupportedTitle" : "appView.notRunningTitle")} />)}
    {tabs.map(tab => {
      const current = installs.items?.find(a => a.id === tab.app.id);
      if (!current || (current.kind === "docker_app" && current.runtimeStatus !== "running")) return null;
      const canonical = environmentPath(environment, tab.app.appId);
      const shown = active === current.id || (!active && selected === tab);
      const saved = paths.get(canonical) ?? tab.basePath;
      const url = new URL(selected === tab ? href : saved, window.location.origin);
      const frameBase = url.pathname.startsWith("/apps/") ? "/apps/" + tab.app.appId : canonical;
      return <section key={tab.app.id} hidden={!shown} inert={!shown} aria-label={tab.app.name}>
        <TabFrame tab={tab} current={current} active={shown} basePath={frameBase}
          relative={{ pathname: url.pathname.slice(frameBase.length) || "/", search: url.search, hash: url.hash }}
          onInput={() => setTabs(old => [...old])}
          onSlot={result => {
            if (result === "full") {
              if (locationRef.current === href && selected === tab) {
                router.history.replace(lastPage.current === href ? environmentPath(environment) : lastPage.current, undefined, { ignoreBlocker: true });
                setLimit(true);
              }
              setTabs(old => old.filter(t => t.app.id !== tab.app.id));
            } else {
              if (result === "opened") setLimit(false);
              setTabs(old => old.map(t => t.app.id === tab.app.id && !t.admitted ? { ...t, admitted: result === "opened", failed: result === "failed" } : t));
            }
          }} />
      </section>;
    })}
  </>;
}

function TabFrame({ tab, current, active, basePath, relative, onInput, onSlot }: {
  tab: Tab; current: Installation; active: boolean; basePath: string;
  relative: { pathname: string; search: string; hash: string };
  onInput: () => void; onSlot: (result: "opened" | "failed" | "full") => void;
}) {
  const { t } = useTranslation("portal");
  const { csrf } = useUser();
  const router = useRouter();
  const [updated, setUpdated] = useState(false);
  const guards = useMemo(() => {
    const g = createNavigationGuards();
    g.register(() => leaveMessage(tab.input.current));
    return g;
  }, [tab.input]);
  useEffect(() => {
    activeGuards.add(guards);
    const unblock = router.history.block({
      blockerFn: ({ nextLocation }: { nextLocation: { pathname: string } }) => active && !appNavigation.active &&
        (nextLocation.pathname === basePath || nextLocation.pathname.startsWith(basePath + "/")) && !guards.confirmNavigation(),
      enableBeforeUnload: () => !!guards.message(),
    });
    return () => { unblock(); activeGuards.delete(guards); };
  }, [guards, router, active, basePath]);
  const health = current.health?.status;
  return <>
    <div className="app-session-bar">
      {(updated || current.releaseId !== tab.app.releaseId) && <Notice tone="warning" actions={<Button onClick={() => window.location.reload()}>{t("appView.reload")}</Button>}>
        <strong>{t("appView.changedTitle")}</strong><p>{t("appView.changed")}</p></Notice>}
      {(health === "unhealthy" || health === "degraded") && <Notice role="status" aria-label={t("appView.availabilityLabel")} tone="warning">{t(health === "unhealthy" ? "appView.unhealthy" : "appView.degraded")}</Notice>}
    </div>
    <AppFrame app={tab.app} active={active} csrf={csrf} basePath={basePath} input={tab.input}
      tabLocation={relative} onSlot={onSlot} onInput={onInput} onReleaseChanged={() => setUpdated(true)} />
  </>;
}
