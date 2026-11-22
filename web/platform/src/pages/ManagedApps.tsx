import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation } from "@tanstack/react-router";
import { request, type App } from "@miy/sdk";
import { distributionLabelKeys, useManagedInstallations } from "../data/installations";
import { useSurface, useUser } from "../data/session";
import { currentEnvironment, environmentOrigin, environmentPath, surfaceUrl } from "../lib/paths";
import { LoadError, StaleNotice } from "../components/LoadState";
import { NewTabLink } from "../components/NewTabLink";
import { RuntimeControls } from "../components/RuntimeControls";
import { RuntimeHealth } from "../components/RuntimeHealth";
import {
  AppMark,
  Badge,
  Button,
  EmptyState,
  Loading,
  Panel,
  monograms,
} from "../ui";

type AppPage = {
  apps: App[];
  nextCursor?: string | null;
};

async function readApps(path: string) {
  const value = await request<AppPage>(path);
  if (!Array.isArray(value.apps)) throw new Error("invalid_app_list");
  return value;
}

export function ManagedApps() {
  const { t } = useTranslation("portal");
  const user = useUser();
  const surface = useSurface();
  const managed = useManagedInstallations();
  const environment = currentEnvironment(useLocation().pathname);
  const [catalog, setCatalog] = useState<AppPage>();
  const [catalogError, setCatalogError] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const loadCatalog = useCallback(async () => {
    try {
      setCatalog(await readApps("/api/v1/apps?limit=200"));
      setCatalogError(false);
    } catch {
      setCatalogError(true);
    }
  }, []);
  useEffect(() => {
    void loadCatalog();
  }, [loadCatalog]);
  async function moreApps() {
    if (!catalog?.nextCursor || loadingMore) return;
    setLoadingMore(true);
    try {
      const next = await readApps(
        `/api/v1/apps?limit=200&cursor=${encodeURIComponent(catalog.nextCursor)}`,
      );
      setCatalog({ ...next, apps: [...catalog.apps, ...next.apps] });
      setCatalogError(false);
    } catch {
      setCatalogError(true);
    } finally {
      setLoadingMore(false);
    }
  }
  const apps = (managed.items ?? []).filter(
    (app) => app.environment === environment,
  );
  const marks = useMemo(
    () => monograms(apps.map((app) => ({ key: app.id, name: app.name }))),
    [apps],
  );
  const refresh = () => {
    void managed.refresh();
    void loadCatalog();
  };
  return (
    <>
      <div className="page-header">
        <div>
          <h1>{t("managedApps.title")}</h1>
          <p>
            {t(environment === "prod" ? "managedApps.descriptionProd" : "managedApps.descriptionDev")}
          </p>
        </div>
      </div>
      {managed.items === null ? (
        managed.status === "error" ? (
          <LoadError
            message={t("managedApps.loadFailed")}
            onRetry={refresh}
          />
        ) : (
          <Loading>{t("managedApps.loading")}</Loading>
        )
      ) : (
        <>
          {managed.status === "error" && (
            <StaleNotice updatedAt={managed.updatedAt} onRetry={refresh} />
          )}
          {apps.length === 0 ? (
            <EmptyState title={t("managedApps.emptyTitle")}>
              {t("managedApps.emptyHint")}
            </EmptyState>
          ) : (
            <div className="card-grid">
              {apps.map((app) => (
                <Panel
                  key={app.id}
                  className="app-card"
                  title={
                    <span className="app-card-title">
                      <AppMark
                        mark={marks.get(app.id) ?? "?"}
                        colorKey={app.appId}
                      />
                      <span>{app.name}</span>
                    </span>
                  }
                  actions={
                    app.appDisabled && (
                      <Badge tone="warning">{t("managedApps.disabled")}</Badge>
                    )
                  }
                >
                  <div className="app-card-meta">
                    <Badge>{t(distributionLabelKeys[app.distribution ?? "organization"])}</Badge>
                    <Badge>{app.runtimeStatus === "recovery_required" ? t("managedApps.recoveryRequired") : app.runtimeStatus}</Badge>
                    <Badge>DB {app.dbStatus}</Badge>
                    <Badge>{app.runtimeStatus === "recovery_required" ? `${t("managedApps.lastGood")} ` : ""}v{app.version || "—"}</Badge>
                    <code>{app.appId}</code>
                  </div>
                  <RuntimeHealth app={app} />
                  {app.canOpen ? (
                    <NewTabLink
                      className="app-card-open"
                      href={surfaceUrl(
                        environment === "invalid" ? surface.portalOrigin : environmentOrigin(surface, environment),
                        environmentPath(environment, app.appId),
                      )}
                    >
                      {t("managedApps.openInPortal")}
                    </NewTabLink>
                  ) : (
                    <p className="ui-empty-inline">
                      {t(app.appDisabled ? "managedApps.disabledNotice" : "managedApps.cannotOpen")}
                    </p>
                  )}
                  {app.kind === "docker_app" && app.canManage && (
                    <RuntimeControls
                      app={app}
                      csrf={user.csrf}
                      refresh={refresh}
                    />
                  )}
                </Panel>
              ))}
            </div>
          )}
        </>
      )}
      <h2 className="page-section-title">{t("managedApps.catalogTitle")}</h2>
      <p>
        {t("managedApps.catalogLead")}
      </p>
      {catalogError && (
        <LoadError
          message={t("managedApps.catalogLoadFailed")}
          onRetry={() => void loadCatalog()}
        />
      )}
      {!catalog && !catalogError && <Loading />}
      {catalog &&
        (catalog.apps.length ? (
          <ul>
            {catalog.apps.map((app) => (
              <li key={app.id}>
                {app.name} · <code>{app.id}</code> · {t(distributionLabelKeys[app.distribution ?? "organization"])}
              </li>
            ))}
          </ul>
        ) : (
          <p className="ui-empty-inline">{t("managedApps.catalogEmpty")}</p>
        ))}
      {catalog?.nextCursor && (
        <Button disabled={loadingMore} onClick={() => void moreApps()}>
          {t("managedApps.catalogMore")}
        </Button>
      )}
    </>
  );
}
