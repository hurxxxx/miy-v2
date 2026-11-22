import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Link, useLocation } from "@tanstack/react-router";
import type { Installation } from "@miy/sdk";
import { distributionLabelKeys, installationsIn, useInstallationPolling, useInstallations } from "../data/installations";
import { currentEnvironment, environmentPath, portalEnvironment } from "../lib/paths";
import { useSurface } from "../data/session";
import { LoadError, StaleNotice } from "../components/LoadState";
import { HomeCards } from "../components/home/HomeCards";
import { AppMark, EmptyState, Loading, Panel, monograms } from "../ui";

export function Home() {
  const { t } = useTranslation("portal");
  const installations = useInstallations();
  useInstallationPolling();
  const location = useLocation();
  const surface = useSurface();
  const environment = currentEnvironment(
    location.pathname,
    portalEnvironment(surface.surface, surface.legacyPortal) ?? "dev",
  );
  const refresh = () => void installations.refresh();
  const apps = installationsIn(installations.items, environment);
  const marks = useMemo(() => monograms(apps.map((a) => ({ key: a.id, name: a.name }))), [apps]);
  return (
    <>
      <div className="page-header">
        <div>
          <h1>{t(environment === "prod" ? "home.prodApps" : "home.devApps")}</h1>
          <p>{t("home.lead")}</p>
        </div>
      </div>
      {installations.items === null ? (
        installations.status === "error" ? (
          <LoadError message={t("home.loadFailed")} onRetry={refresh} />
        ) : (
          <Loading>{t("home.loading")}</Loading>
        )
      ) : (
        <>
          {installations.status === "error" && <StaleNotice updatedAt={installations.updatedAt} onRetry={refresh} />}
          {apps.length > 0 && <HomeCards environmentPath={(appId) => environmentPath(environment, appId)} />}
          {apps.length === 0 ? (
            <EmptyState title={t("home.emptyTitle")}>
              {t("home.emptyHint")}
            </EmptyState>
          ) : (
            Object.entries(distributionLabelKeys).map(([distribution, labelKey]) => {
              const group = apps.filter((app) => (app.distribution ?? "organization") === distribution);
              if (!group.length) return null;
              return (
                <section key={distribution} aria-labelledby={`apps-${distribution}`}>
                  <h2 className="page-section-title" id={`apps-${distribution}`}>{t(labelKey)}</h2>
                  <div className="card-grid">
                    {group.map((app) => (
                      <AppCard key={app.id} app={app} mark={marks.get(app.id) ?? "?"} environment={environment} />
                    ))}
                  </div>
                </section>
              );
            })
          )}
        </>
      )}
    </>
  );
}

function AppCard({
  app: a,
  mark,
  environment,
}: {
  app: Installation;
  mark: string;
  environment: string;
}) {
  const { t } = useTranslation("portal");
  const openable = a.kind !== "docker_app" || a.runtimeStatus === "running";
  return (
    <Panel
      className="app-card"
      title={
        <span className="app-card-title">
          <AppMark mark={mark} colorKey={a.appId} />
          <span>{a.name}</span>
        </span>
      }
    >
      {openable && (
        <Link className="app-card-open" to={environmentPath(environment, a.appId) as "/"}>
          {t("home.open")}
        </Link>
      )}
      {!openable && <p>{t("home.unavailable")}</p>}
    </Panel>
  );
}
