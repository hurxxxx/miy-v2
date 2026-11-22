import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode, type SyntheticEvent } from "react";
import { Link, Outlet, useLocation, useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { InstallationsProvider, installationsIn, useInstallations } from "../data/installations";
import { useSession, useSurface, type LoginOptions, type User } from "../data/session";
import { currentEnvironment, environmentOrigin, environmentPath, isAccountLinkPath, isCliApprovalPath, isConsolePath, isAppPath, pathEnvironment, portalEnvironment, surfaceUrl, type Environment } from "../lib/paths";
import { signOut } from "../lib/sign-out";
import { LoadError } from "../components/LoadState";
import { NewTabLink } from "../components/NewTabLink";
import { CommandPalette, useCommandPaletteShortcut, type Command } from "../components/CommandPalette";
import { NotificationBell } from "../components/notifications/NotificationBell";
import { Login } from "../pages/Login";
import { CliInstall } from "../pages/CliInstall";
import { Join } from "../pages/Join";
import { AppMark, Badge, Button, Loading, Panel, monograms } from "../ui";
import { CloseIcon, HomeIcon, MenuIcon, SettingsIcon, TerminalIcon } from "../ui/icons";
import { AppBarEditor } from "./AppBarEditor";
import { AppBarLauncherMenu, AppLauncherButton } from "./AppBarLauncherMenus";
import type { AppBarAppItem } from "./app-bar-model";
import { useAppBarController } from "./useAppBarController";

export function Shell() {
  const { t } = useTranslation("portal");
  const { state, options, retry } = useSession();
  const location = useLocation();
  useEffect(() => {
    if (state.status === "ready" && options?.surface === "console" && location.pathname === "/") {
      const target = sessionStorage.getItem("miy.returnTo");
      if (isCliApprovalPath(target)) {
        sessionStorage.removeItem("miy.returnTo");
        window.location.assign(target!);
      }
    }
  }, [state.status, options?.surface, location.pathname]);
  // The installation guide is public; it needs no session. Render it directly:
  // during navigation the router outlet may already hold the next page.
  if (location.pathname === "/cli/install" && state.status !== "ready")
    return (
      <div className="cli-install-public">
        <div className="public-bar">
          <Link to="/" className="brand">
            miy
          </Link>
          <Link to="/">{t("shell.backToPlatform")}</Link>
        </div>
        <main>
          <CliInstall />
        </main>
      </div>
    );
  // Invitation and password reset links need no session. They live on the console,
  // next to the sign-in page; a portal sends these paths there like other console
  // pages, without forwarding any input.
  if (options?.surface === "console" && isAccountLinkPath(location.pathname)) return <Join />;
  const ownEnvironment = options ? portalEnvironment(options.surface, options.legacyPortal) : null;
  if (options && options.surface !== "console" && isConsolePath(location.pathname))
    return <SurfaceRedirect href={surfaceUrl(options.consoleOrigin, location.pathname)} label={t("shell.console")} />;
  // Each portal runs one environment. Published CLI links and bookmarks name the
  // other environment's paths on this origin: move them before sign-in or any app
  // import, in the same tab.
  const requested = pathEnvironment(location.pathname);
  if (options && ownEnvironment && requested && requested !== ownEnvironment)
    return (
      <SurfaceRedirect
        href={surfaceUrl(environmentOrigin(options, requested), location.pathname)}
        label={t(requested === "dev" ? "surfaces.dev_portal" : "surfaces.portal")}
      />
    );
  if (options?.surface === "console" && isAppPath(location.pathname))
    return <main className="boot"><Panel title={t("shell.openAppsInPortal")}>
      <p>{t("shell.consoleRunsNoApps")}</p>
      <NewTabLink href={surfaceUrl(environmentOrigin(options, requested ?? "dev"), location.pathname)}>{t("shell.openInPortal")}</NewTabLink>
    </Panel></main>;
  if (state.status === "loading")
    return (
      <main className="boot">
        <Loading />
      </main>
    );
  if (state.status === "error")
    return (
      <main className="login-page">
        <div className="login-card">
          <p className="login-brand">miy</p>
          <LoadError message={t("shell.serverUnreachable")} onRetry={retry} />
        </div>
      </main>
    );
  if (state.status === "anonymous") return <Login />;
  const environment = currentEnvironment(location.pathname, ownEnvironment ?? "dev");
  if (environment === "invalid")
    return (
      <main className="boot">
        <Panel title={t("shell.unknownEnvironment")}>
          <p>{t("shell.useEnvironmentAddress")}</p>
          <Link to={environmentPath(ownEnvironment ?? "dev") as "/"}>
            {t(ownEnvironment === "prod" ? "shell.toProd" : "shell.toDev")}
          </Link>
        </Panel>
      </main>
    );
  return (
    <InstallationsProvider>
      <SignedInLayout user={state.user} environment={environment} />
    </InstallationsProvider>
  );
}

function SurfaceRedirect({ href, label }: { href: string; label: string }) {
  const { t } = useTranslation("portal");
  useEffect(() => { window.location.replace(href); }, [href]);
  // This page replaces itself; the fallback must not open a second tab.
  return <main className="boot"><p>{t("shell.redirecting", { label })} <a href={href}>{t("shell.continue")}</a></p></main>;
}

const ENVIRONMENTS: Environment[] = ["dev", "prod"];

function EnvironmentSwitch({
  environment,
  className,
  options,
}: {
  environment: Environment;
  className: string;
  options: LoginOptions;
}) {
  const { t } = useTranslation("portal");
  const own = portalEnvironment(options.surface, options.legacyPortal);
  return (
    <nav aria-label={t("shell.environment")} className={`env-switch ${className}`}>
      {ENVIRONMENTS.map((id) => ({ id, label: t(`shell.environments.${id}`) })).map((item) =>
        // On a portal the other environment is another origin (a new tab); the
        // console manages both environments in place.
        own && item.id !== own ? (
          <NewTabLink
            key={item.id}
            href={surfaceUrl(environmentOrigin(options, item.id), environmentPath(item.id))}
            className={`env-option env-${item.id}`}
          >
            {item.label}
          </NewTabLink>
        ) : (
          <Link
            key={item.id}
            to={environmentPath(item.id) as "/"}
            activeOptions={{ exact: true }}
            className={`env-option env-${item.id}`}
            aria-current={item.id === environment ? "true" : undefined}
          >
            {item.label}
          </Link>
        ),
      )}
    </nav>
  );
}

function RailLink({
  to,
  label,
  short,
  icon,
  active,
}: {
  to: string;
  label: string;
  short?: string;
  icon: ReactNode;
  active: boolean;
}) {
  // Same-origin navigation stays in this tab (integrated routing). Links to another
  // origin (portal <-> console) use NewTabLink instead.
  return (
    <Link
      to={to as "/"}
      activeOptions={{ exact: true }}
      className="rail-item"
      aria-label={label}
      aria-current={active ? "page" : undefined}
    >
      <span className="rail-icon">{icon}</span>
      {short && short !== label ? (
        <>
          <span className="rail-label rail-label-short">{short}</span>
          <span className="rail-label rail-label-full">{label}</span>
        </>
      ) : (
        <span className="rail-label">{label}</span>
      )}
    </Link>
  );
}

function pageTitle(t: TFunction<"portal">, pathname: string, environment: Environment, appName?: string, console = false) {
  if (pathname === "/admin") return t("shell.titles.admin");
  if (pathname === "/cli/install") return t("shell.titles.cliInstall");
  if (isCliApprovalPath(pathname)) return t("shell.titles.cliApproval");
  if (pathname === "/cli/sessions") return t("shell.titles.cliSessions");
  if (pathname === "/settings" || pathname.startsWith("/settings/")) return t("shell.titles.settings");
  if (appName) return appName;
  if (console) return t("shell.titles.managedApps");
  return t(environment === "prod" ? "shell.titles.prodApps" : "shell.titles.devApps");
}

function SignedInLayout({ user, environment }: { user: User; environment: Environment }) {
  const { t } = useTranslation("portal");
  const options = useSurface();
  const isConsole = options.surface === "console";
  const [menuOpen, setMenuOpen] = useState(false);
  const [tooltip, setTooltip] = useState<{ text: string; top: number } | null>(null);
  const location = useLocation();
  const installations = useInstallations();
  const refresh = installations.refresh;
  const toggle = useRef<HTMLButtonElement>(null);
  const navigation = useRef<HTMLElement>(null);
  const pathname = location.pathname;
  useEffect(() => setMenuOpen(false), [pathname]);
  useEffect(() => {
    void refresh();
  }, [refresh, pathname]);
  const apps = useMemo(() => installationsIn(installations.items, environment), [installations.items, environment]);
  const marks = useMemo(() => monograms(apps.map((a) => ({ key: a.id, name: a.name }))), [apps]);
  // The launcher (ported from AI-DO's app bar): the rail lists the pinned apps,
  // or every app while nothing is pinned; the menu lists every app by category.
  const appItems = useMemo<AppBarAppItem[]>(
    () => (isConsole ? [] : apps.map((a) => ({ id: a.appId, title: a.name, mark: marks.get(a.id) ?? "?" }))),
    [apps, isConsole, marks],
  );
  const launcher = useAppBarController({ currentPathname: pathname, user, visibleItems: appItems });
  // Ctrl/⌘+K: go to an app or a platform page (frames forward the keys).
  const navigate = useNavigate();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const openPalette = useCallback(() => setPaletteOpen(true), []);
  useCommandPaletteShortcut(openPalette);
  const commands = useMemo<Command[]>(() => {
    const go = (to: string) => () => void navigate({ to: to as "/" });
    return [
      { id: "home", label: t("commandPalette.home"), run: go(environmentPath(environment)) },
      ...appItems.map((item) => ({
        id: `app-${item.id}`,
        label: item.title,
        hint: t("commandPalette.appHint"),
        run: go(environmentPath(environment, item.id)),
      })),
      { id: "settings", label: t("commandPalette.settings"), run: go("/settings") },
      ...(isConsole && user.admin ? [{ id: "admin", label: t("commandPalette.admin"), run: go("/admin") }] : []),
      ...(isConsole ? [{ id: "cli-sessions", label: t("commandPalette.cliSessions"), run: go("/cli/sessions") }] : []),
    ];
  }, [appItems, environment, isConsole, navigate, t, user.admin]);
  const launcherOpen = launcher.state.launcherOpen || launcher.state.appBarEditorOpen;
  const launcherOpenRef = useRef(launcherOpen);
  useEffect(() => {
    launcherOpenRef.current = launcherOpen;
  }, [launcherOpen]);
  useEffect(() => {
    if (!menuOpen) return;
    navigation.current?.querySelector<HTMLElement>("a, button")?.focus();
    const onKey = (event: KeyboardEvent) => {
      // Escape closes the launcher first; the drawer stays open behind it.
      if (event.key !== "Escape" || launcherOpenRef.current) return;
      setMenuOpen(false);
      toggle.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [menuOpen]);
  const parts = pathname.split("/");
  const activeAppId = parts[1] === "environments" ? parts[4] : parts[1] === "apps" ? parts[2] : undefined;
  const activeApp = activeAppId ? apps.find((a) => a.appId === activeAppId) : undefined;
  const homeActive = pathname === "/" || pathname === environmentPath(environment);

  function showTooltip(event: SyntheticEvent<HTMLElement>) {
    const item = (event.target as HTMLElement).closest<HTMLElement>(".rail-item");
    if (!item || window.matchMedia("(max-width: 767px)").matches) return;
    const rect = item.getBoundingClientRect();
    const text = [item.getAttribute("aria-label"), item.getAttribute("aria-description")].filter(Boolean).join(" · ");
    setTooltip({ text, top: rect.top + rect.height / 2 });
  }
  const settingsActive = pathname === "/settings" || pathname.startsWith("/settings/");
  return (
    <div className={`shell env-is-${environment}`}>
      <a className="skip-link" href="#platform-main">
        {t("shell.skipToContent")}
      </a>
      <aside
        id="platform-navigation"
        ref={navigation}
        className={`rail ${menuOpen ? "navigation-open" : ""}`}
        aria-label={t("shell.platformNavigation")}
        onMouseOver={showTooltip}
        onFocus={showTooltip}
        onMouseLeave={() => setTooltip(null)}
        onBlur={() => setTooltip(null)}
      >
        <Link to="/" activeOptions={{ exact: true }} className="brand rail-brand" aria-label={t("shell.homeLabel")}>
          miy
        </Link>
        <EnvironmentSwitch environment={environment} className="env-switch-drawer" options={options} />
        <nav aria-label={t("shell.apps")} className="rail-section rail-apps">
          <RailLink
            to={environmentPath(environment)}
            label={t("shell.home")}
            icon={<HomeIcon />}
            active={homeActive}
          />
          {!isConsole && (
            <AppLauncherButton
              buttonRef={launcher.launcherButtonRef}
              onToggle={launcher.onToggleLauncher}
              open={launcher.state.launcherOpen}
              t={launcher.t}
            />
          )}
          {launcher.pinnedItems.map((item) => (
            <RailLink
              key={item.id}
              to={environmentPath(environment, item.id)}
              label={item.title}
              icon={<AppMark mark={item.mark} colorKey={item.id} />}
              active={item.id === activeAppId}
            />
          ))}
          {installations.items === null && (
            <p className="rail-note">
              {t(installations.status === "error" ? "shell.appsLoadFailed" : "shell.appsLoading")}
            </p>
          )}
        </nav>
        <nav aria-label={t("shell.platform")} className="rail-section rail-platform">
          {isConsole && user.admin && (
            <RailLink to="/admin" label={t("shell.administration")} short={t("shell.administrationShort")} icon={<SettingsIcon />} active={pathname === "/admin"} />
          )}
          {isConsole && <RailLink to="/cli/sessions" label={t("shell.myCliSessions")} icon={<TerminalIcon />} active={pathname === "/cli/sessions"} />}
          {!isConsole && <NewTabLink className="rail-item" aria-label={t(user.canManageConsole ? "shell.console" : "shell.myCliSessions")}
            href={surfaceUrl(options.consoleOrigin, user.canManageConsole ? environmentPath(environment) : "/cli/sessions")}>
            <span className="rail-icon"><SettingsIcon /></span><span className="rail-label">{t(user.canManageConsole ? "shell.console" : "shell.myCliSessions")}</span>
          </NewTabLink>}
          {isConsole && <NewTabLink className="rail-item" aria-label={t("shell.portal")} href={surfaceUrl(environmentOrigin(options, environment), environmentPath(environment))}>
            <span className="rail-icon"><HomeIcon /></span><span className="rail-label">{t("shell.portal")}</span>
          </NewTabLink>}
          <RailLink
            to="/cli/install"
            label={t("shell.cliInstall")}
            short={t("shell.cliInstallShort")}
            icon={<TerminalIcon />}
            active={pathname === "/cli/install"}
          />
        </nav>
      </aside>
      {menuOpen && <div className="drawer-backdrop" aria-hidden="true" onClick={() => setMenuOpen(false)} />}
      {tooltip && !launcherOpen && (
        <div className="rail-tooltip" aria-hidden="true" style={{ top: tooltip.top }}>
          {tooltip.text}
        </div>
      )}
      {launcher.state.launcherOpen && (
        <AppBarLauncherMenu
          appsStatus={installations.items !== null ? "ready" : installations.status === "error" ? "error" : "loading"}
          categories={launcher.categories}
          contextLabel={t(`shell.environments.${environment}`)}
          currentPathname={pathname}
          items={appItems}
          linkFor={(appId) => environmentPath(environment, appId)}
          menuRef={launcher.menuRef}
          onClose={launcher.onCloseLauncherMenus}
          onOpenEditor={launcher.onOpenEditor}
          t={launcher.t}
        />
      )}
      {launcher.state.appBarEditorOpen && (
        <AppBarEditor
          appBarLayoutError={launcher.state.appBarLayoutError}
          appBarLayoutSaving={launcher.state.appBarLayoutSaving}
          dialogRef={launcher.editorRef}
          draftItems={launcher.draftItems}
          draftPinnedAppIds={launcher.state.draftPinnedAppIds}
          launcherCategories={launcher.categories.categories ?? []}
          onClose={launcher.onCloseEditor}
          onMovePinnedApp={launcher.onMovePinnedApp}
          onReset={launcher.onResetDraft}
          onSave={launcher.onSaveLayout}
          onTogglePinnedApp={launcher.onTogglePinnedApp}
          pinnedEligibleAppIds={launcher.pinnedEligibleAppIds}
          t={launcher.t}
        />
      )}
      <div className="workspace">
        <header className="shell-header">
          <button
            ref={toggle}
            type="button"
            className="navigation-toggle"
            aria-controls="platform-navigation"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen(!menuOpen)}
          >
            {menuOpen ? <CloseIcon /> : <MenuIcon />}
            <span className="visually-hidden">{t(menuOpen ? "shell.closeMenu" : "shell.openMenu")}</span>
          </button>
          <div className="header-context">
            <EnvironmentSwitch environment={environment} className="env-switch-header" options={options} />
            <span className="env-badge-mobile">
              <Badge tone={environment === "prod" ? "warning" : "info"}>{t(`shell.environmentBadges.${environment}`)}</Badge>
            </span>
            <p className="header-title">{pageTitle(t, pathname, environment, activeApp?.name, isConsole)}</p>
          </div>
          <div className="header-account">
            <NotificationBell
              csrf={user.csrf}
              enabled={!isConsole}
              environmentPath={(appId) => environmentPath(environment, appId)}
            />
            <Link
              to="/settings"
              className="account-name"
              title={t("shell.mySettings")}
              aria-current={settingsActive ? "page" : undefined}
            >
              {user.name}
            </Link>
            <Button size="sm" onClick={() => void signOut(user.csrf)}>
              {t("shell.signOut")}
            </Button>
          </div>
        </header>
        <main id="platform-main" tabIndex={-1}>
          <Outlet />
        </main>
        <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} commands={commands} />
      </div>
    </div>
  );
}
