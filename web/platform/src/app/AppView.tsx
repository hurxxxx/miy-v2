import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useLocation, useRouter } from "@tanstack/react-router";
import { UI_CONTRACT, createNavigationGuards, request, type Installation } from "@miy/sdk";
import { Button, Loading, Notice, Panel } from "../ui";
import { useUser } from "../data/session";
import { currentEnvironment, environmentPath } from "../lib/paths";
import { activeGuards } from "../lib/navigation-guards";
import { AppFrame, leaveMessage, type FrameInput } from "./AppFrame";

function AppAvailability({ health }: { health: Installation["health"] }) {
  const { t } = useTranslation("portal");
  if (health?.status !== "degraded" && health?.status !== "unhealthy") return null;
  return <Notice role="status" aria-label={t("appView.availabilityLabel")} tone="warning">
    <p>{t(health.status === "unhealthy" ? "appView.unhealthy" : "appView.degraded")}</p>
  </Notice>;
}

export function AppView() {
  const location = useLocation();
  const parts = location.pathname.split("/");
  const environment = currentEnvironment(location.pathname);
  const appId = parts[1] === "environments" ? parts[4]! : parts[2]!;
  const basePath = parts[1] === "environments" ? environmentPath(environment, appId) : "/apps/" + appId;
  return (
    <AppSession key={`${environment}:${appId}`} appId={appId} environment={environment} basePath={basePath} />
  );
}

// A route change creates a new session; observations never replace its pinned release.
function AppSession({ appId, environment, basePath }: { appId: string; environment: string; basePath: string }) {
  const { t } = useTranslation("portal");
  const user = useUser();
  const router = useRouter();
  const [app, setApp] = useState<Installation | null>();
  const [loadFailed, setLoadFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [health, setHealth] = useState<Installation["health"]>();
  const [updated, setUpdated] = useState(false);
  // The app reports its input state from its frame; leave checks read the last one.
  const input = useRef<FrameInput>({ state: "clean" });
  const guards = useMemo(() => {
    const created = createNavigationGuards();
    created.register(() => leaveMessage(input.current));
    return created;
  }, [app?.id, app?.releaseId]);
  useEffect(() => {
    activeGuards.add(guards);
    const unblock = router.history.block({
      blockerFn: () => !guards.confirmNavigation(),
      enableBeforeUnload: () => !!guards.message(),
    });
    return () => {
      unblock();
      activeGuards.delete(guards);
    };
  }, [guards, router]);
  useEffect(() => {
    setApp(undefined);
    setLoadFailed(false);
    setUpdated(false);
    let active = true;
    const controller = new AbortController();
    request<Installation[]>("/api/v1/installations", undefined, { signal: controller.signal })
      .then((items) => {
        if (active) {
          const found = items.find((a) => a.appId === appId && a.environment === environment) ?? null;
          setApp(found);
          setHealth(found?.health);
        }
      })
      .catch(() => {
        if (active) setLoadFailed(true);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [appId, environment, attempt]);
  useEffect(() => {
    if (!app) return;
    const session = app;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const controller = new AbortController();
    async function poll() {
      try {
        const items = await request<Installation[]>("/api/v1/installations", undefined, {
          signal: controller.signal,
        });
        if (!active) return;
        const current = items.find((a) => a.id === session.id);
        if (
          !current ||
          current.releaseId !== session.releaseId ||
          current.runtimeStatus !== session.runtimeStatus
        )
          setUpdated(true);
        else setHealth(current.health);
      } catch {
        if (active) setUpdated(true);
      } finally {
        if (active) timer = setTimeout(poll, 3000);
      }
    }
    timer = setTimeout(poll, 3000);
    return () => {
      active = false;
      clearTimeout(timer);
      controller.abort();
    };
  }, [app]);
  if (loadFailed)
    return (
      <Panel title={t("appView.loadFailedTitle")}>
        <Notice
          role="alert"
          tone="danger"
          actions={
            <Button size="sm" onClick={() => setAttempt((value) => value + 1)}>
              {t("common:actions.retry")}
            </Button>
          }
        >
          <p>{t("appView.loadFailed")}</p>
        </Notice>
      </Panel>
    );
  if (app === undefined) return <Loading>{t("appView.loading")}</Loading>;
  if (!app)
    return (
      <Panel title={t("appView.noAccessTitle")}>
        <p>{t("appView.noAccess")}</p>
      </Panel>
    );
  if (app.kind === "docker_app" && app.runtimeStatus !== "running")
    return (
      <Panel title={t("appView.notRunningTitle")}>
        <p>{t("appView.notRunning")}</p>
        <Link to={environmentPath(environment) as "/"}>{t("appView.home")}</Link>
      </Panel>
    );
  if (app.ui.contractVersion !== UI_CONTRACT)
    return (
      <Panel title={t("appView.unsupportedTitle")}>
        <p>{t("appView.unsupported")}</p>
      </Panel>
    );
  return (
    <>
      <div className="app-session-bar">
        {updated && (
          <Notice
            tone="warning"
            actions={
              <Button size="sm" variant="primary" onClick={() => window.location.reload()}>
                {t("appView.reload")}
              </Button>
            }
          >
            <p>
              <strong>{t("appView.changedTitle")}</strong>
            </p>
            <p>{t("appView.changed")}</p>
          </Notice>
        )}
        <AppAvailability health={health ?? app.health} />
      </div>
      <AppFrame
        key={app.id + app.releaseId}
        app={app}
        basePath={basePath}
        csrf={user.csrf}
        input={input}
        onReleaseChanged={() => setUpdated(true)}
      />
    </>
  );
}
