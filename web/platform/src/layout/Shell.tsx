import { useEffect, useMemo, useRef, useState, type ReactNode, type SyntheticEvent } from "react";
import { Link, Outlet, useLocation } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { InstallationsProvider, installationsIn, useInstallations } from "../data/installations";
import { useSession, useSurface, type User } from "../data/session";
import { currentEnvironment, environmentPath, isCliApprovalPath, isConsolePath, isAppPath, surfaceUrl, type Environment } from "../lib/paths";
import { clearAllAppGuards, confirmLeaveAllApps } from "../lib/navigation-guards";
import { LoadError } from "../components/LoadState";
import { NewTabLink } from "../components/NewTabLink";
import { Login } from "../pages/Login";
import { CliInstall } from "../pages/CliInstall";
import { AppMark, Badge, Button, Loading, Panel, monograms } from "../ui";
import { CloseIcon, HomeIcon, MenuIcon, SettingsIcon, TerminalIcon } from "../ui/icons";

export function Shell() {
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
          <Link to="/">플랫폼으로 돌아가기</Link>
        </div>
        <main>
          <CliInstall />
        </main>
      </div>
    );
  if (options?.surface === "portal" && isConsolePath(location.pathname))
    return <ConsoleRedirect href={surfaceUrl(options.consoleOrigin, location.pathname)} />;
  if (options?.surface === "console" && isAppPath(location.pathname))
    return <main className="boot"><Panel title="업무 앱은 포털에서 열어 주세요">
      <p>관리 콘솔에서는 업무 앱을 실행하지 않습니다.</p>
      <NewTabLink href={surfaceUrl(options.portalOrigin, location.pathname)}>업무 포털에서 열기</NewTabLink>
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
          <LoadError message="서버에 연결하지 못했습니다. 로그인 상태는 바뀌지 않았습니다." onRetry={retry} />
        </div>
      </main>
    );
  if (state.status === "anonymous") return <Login />;
  const environment = currentEnvironment(location.pathname);
  if (environment === "invalid")
    return (
      <main className="boot">
        <Panel title="존재하지 않는 실행 환경입니다">
          <p>테스트 또는 운영 영역의 주소를 사용하세요.</p>
          <Link to={environmentPath("dev") as "/"}>테스트 영역으로</Link>
        </Panel>
      </main>
    );
  return (
    <InstallationsProvider>
      <SignedInLayout user={state.user} environment={environment} />
    </InstallationsProvider>
  );
}

function ConsoleRedirect({ href }: { href: string }) {
  useEffect(() => { window.location.replace(href); }, [href]);
  return <main className="boot"><p>관리 콘솔로 이동합니다. <NewTabLink href={href}>계속</NewTabLink></p></main>;
}

const ENVIRONMENTS: { id: Environment; label: string; badge: string }[] = [
  { id: "dev", label: "테스트 영역", badge: "테스트 · DEV" },
  { id: "prod", label: "운영 영역", badge: "운영 · PROD" },
];

function EnvironmentSwitch({ environment, className }: { environment: Environment; className: string }) {
  return (
    <nav aria-label="실행 환경" className={`env-switch ${className}`}>
      {ENVIRONMENTS.map((item) => (
        <Link
          key={item.id}
          to={environmentPath(item.id) as "/"}
          activeOptions={{ exact: true }}
          className={`env-option env-${item.id}`}
          aria-current={item.id === environment ? "true" : undefined}
        >
          {item.label}
        </Link>
      ))}
    </nav>
  );
}

function RailLink({
  to,
  label,
  short,
  icon,
  active,
  newTab = false,
}: {
  to: string;
  label: string;
  short?: string;
  icon: ReactNode;
  active: boolean;
  newTab?: boolean;
}) {
  const content = (
    <>
      <span className="rail-icon">{icon}</span>
      {short && short !== label ? (
        <>
          <span className="rail-label rail-label-short">{short}</span>
          <span className="rail-label rail-label-full">{label}</span>
        </>
      ) : (
        <span className="rail-label">{label}</span>
      )}
    </>
  );
  if (newTab)
    return (
      <NewTabLink href={to} className="rail-item" aria-label={label} aria-current={active ? "page" : undefined}>
        {content}
      </NewTabLink>
    );
  return (
    <Link
      to={to as "/"}
      activeOptions={{ exact: true }}
      className="rail-item"
      aria-label={label}
      aria-current={active ? "page" : undefined}
    >
      {content}
    </Link>
  );
}

function pageTitle(pathname: string, environment: Environment, appName?: string, console = false) {
  if (pathname === "/admin") return "플랫폼 관리";
  if (pathname === "/cli/install") return "CLI 설치";
  if (isCliApprovalPath(pathname)) return "CLI 로그인 승인";
  if (pathname === "/cli/sessions") return "내 CLI 연결";
  if (appName) return appName;
  if (console) return "내가 관리하는 앱";
  return environment === "prod" ? "운영 앱" : "테스트 앱";
}

function SignedInLayout({ user, environment }: { user: User; environment: Environment }) {
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
  useEffect(() => {
    if (!menuOpen) return;
    navigation.current?.querySelector<HTMLElement>("a, button")?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setMenuOpen(false);
      toggle.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [menuOpen]);

  const apps = installationsIn(installations.items, environment);
  const marks = useMemo(() => monograms(apps.map((a) => ({ key: a.id, name: a.name }))), [apps]);
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
  async function logout() {
    if (!confirmLeaveAllApps()) return;
    const data = await request<{ redirect: string }>("/auth/logout", user.csrf, { method: "POST" });
    clearAllAppGuards();
    window.location.assign(data.redirect);
  }
  const current = ENVIRONMENTS.find((item) => item.id === environment)!;
  return (
    <div className={`shell env-is-${environment}`}>
      <a className="skip-link" href="#platform-main">
        본문으로 건너뛰기
      </a>
      <aside
        id="platform-navigation"
        ref={navigation}
        className={`rail ${menuOpen ? "navigation-open" : ""}`}
        aria-label="플랫폼 탐색"
        onMouseOver={showTooltip}
        onFocus={showTooltip}
        onMouseLeave={() => setTooltip(null)}
        onBlur={() => setTooltip(null)}
      >
        <Link to="/" activeOptions={{ exact: true }} className="brand rail-brand" aria-label="miy 홈">
          miy
        </Link>
        <EnvironmentSwitch environment={environment} className="env-switch-drawer" />
        <nav aria-label="앱" className="rail-section rail-apps">
          <RailLink
            to={environmentPath(environment)}
            label="홈"
            icon={<HomeIcon />}
            active={homeActive}
          />
          {!isConsole && apps.map((a) => (
            <RailLink
              key={a.id}
              to={environmentPath(environment, a.appId)}
              label={a.name}
              icon={<AppMark mark={marks.get(a.id) ?? "?"} colorKey={a.appId} />}
              active={a.appId === activeAppId}
              newTab
            />
          ))}
          {installations.items === null && (
            <p className="rail-note">
              {installations.status === "error" ? "앱 목록을 불러오지 못했습니다." : "앱 목록을 불러오는 중…"}
            </p>
          )}
        </nav>
        <nav aria-label="플랫폼" className="rail-section rail-platform">
          {isConsole && user.admin && (
            <RailLink to="/admin" label="플랫폼 관리" short="관리" icon={<SettingsIcon />} active={pathname === "/admin"} />
          )}
          {isConsole && <RailLink to="/cli/sessions" label="내 CLI 연결" icon={<TerminalIcon />} active={pathname === "/cli/sessions"} />}
          {!isConsole && <NewTabLink className="rail-item" aria-label={user.canManageConsole ? "관리 콘솔" : "내 CLI 연결"}
            href={surfaceUrl(options.consoleOrigin, user.canManageConsole ? environmentPath(environment) : "/cli/sessions")}>
            <span className="rail-icon"><SettingsIcon /></span><span className="rail-label">{user.canManageConsole ? "관리 콘솔" : "내 CLI 연결"}</span>
          </NewTabLink>}
          {isConsole && <NewTabLink className="rail-item" aria-label="업무 포털" href={surfaceUrl(options.portalOrigin, environmentPath(environment))}>
            <span className="rail-icon"><HomeIcon /></span><span className="rail-label">업무 포털</span>
          </NewTabLink>}
          <RailLink
            to="/cli/install"
            label="CLI 설치"
            short="CLI"
            icon={<TerminalIcon />}
            active={pathname === "/cli/install"}
          />
        </nav>
      </aside>
      {menuOpen && <div className="drawer-backdrop" aria-hidden="true" onClick={() => setMenuOpen(false)} />}
      {tooltip && (
        <div className="rail-tooltip" aria-hidden="true" style={{ top: tooltip.top }}>
          {tooltip.text}
        </div>
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
            <span className="visually-hidden">{menuOpen ? "메뉴 닫기" : "메뉴 열기"}</span>
          </button>
          <div className="header-context">
            <EnvironmentSwitch environment={environment} className="env-switch-header" />
            <span className="env-badge-mobile">
              <Badge tone={environment === "prod" ? "warning" : "info"}>{current.badge}</Badge>
            </span>
            <p className="header-title">{pageTitle(pathname, environment, activeApp?.name, isConsole)}</p>
          </div>
          <div className="header-account">
            <span className="account-name" title={user.name}>
              {user.name}
            </span>
            <Button size="sm" onClick={logout}>
              로그아웃
            </Button>
          </div>
        </header>
        <main id="platform-main" tabIndex={-1}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
