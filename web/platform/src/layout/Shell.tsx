import { useEffect, useState } from "react";
import { Link, Outlet, useLocation } from "@tanstack/react-router";
import { request } from "@miy/sdk";
import { Button, Panel } from "@miy/ui";
import { InstallationsProvider, installationsIn, useInstallations } from "../data/installations";
import { useSession, type User } from "../data/session";
import { currentEnvironment, environmentPath, isCliApprovalPath, type Environment } from "../lib/paths";
import { clearAllAppGuards, confirmLeaveAllApps } from "../lib/navigation-guards";
import { LoadError } from "../components/LoadState";
import { Login } from "../pages/Login";
import { CliInstall } from "../pages/CliInstall";

export function Shell() {
  const { state, retry } = useSession();
  const location = useLocation();
  useEffect(() => {
    if (state.status === "ready" && location.pathname === "/") {
      const target = sessionStorage.getItem("miy.returnTo");
      if (isCliApprovalPath(target)) {
        sessionStorage.removeItem("miy.returnTo");
        window.location.assign(target!);
      }
    }
  }, [state.status, location.pathname]);
  // The installation guide is public; it needs no session. Render it directly:
  // during navigation the router outlet may already hold the next page.
  if (location.pathname === "/cli/install" && state.status !== "ready")
    return (
      <main className="cli-install-public">
        <Link to="/" className="brand">
          miy
        </Link>
        <CliInstall />
        <Link to="/">플랫폼으로 돌아가기</Link>
      </main>
    );
  if (state.status === "loading") return <main className="load-pending">불러오는 중…</main>;
  if (state.status === "error")
    return (
      <main className="login">
        <h1>miy</h1>
        <LoadError message="서버에 연결하지 못했습니다. 로그인 상태는 바뀌지 않았습니다." onRetry={retry} />
      </main>
    );
  if (state.status === "anonymous") return <Login />;
  const environment = currentEnvironment(location.pathname);
  if (environment === "invalid")
    return (
      <main>
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

function SignedInLayout({ user, environment }: { user: User; environment: Environment }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const location = useLocation();
  const installations = useInstallations();
  const refresh = installations.refresh;
  useEffect(() => setMenuOpen(false), [location.pathname]);
  useEffect(() => {
    void refresh();
  }, [refresh, location.pathname]);
  async function logout() {
    if (!confirmLeaveAllApps()) return;
    const data = await request<{ redirect: string }>("/auth/logout", user.csrf, { method: "POST" });
    clearAllAppGuards();
    window.location.assign(data.redirect);
  }
  return (
    <div className="shell">
      <aside id="platform-navigation" className={menuOpen ? "navigation-open" : ""}>
        <Link to="/" className="brand">
          miy
        </Link>
        <p className="label">{environment === "prod" ? "운영 · PROD" : "테스트 · DEV"}</p>
        <nav aria-label="실행 환경">
          <Link to={environmentPath("dev") as "/"}>테스트 영역</Link>
          <Link to={environmentPath("prod") as "/"}>운영 영역</Link>
          {user.admin && <Link to={"/admin" as "/"}>플랫폼 관리</Link>}
          <Link to={"/cli/install" as "/"}>CLI 설치</Link>
        </nav>
        <nav aria-label="앱">
          <Link to={environmentPath(environment) as "/"}>홈</Link>
          {installationsIn(installations.items, environment).map((a) => (
            <Link key={a.id} to={environmentPath(environment, a.appId) as "/"}>
              {a.name}
            </Link>
          ))}
          {installations.items === null &&
            (installations.status === "error" ? (
              <p className="nav-note">앱 목록을 불러오지 못했습니다.</p>
            ) : (
              <p className="nav-note">앱 목록을 불러오는 중…</p>
            ))}
        </nav>
      </aside>
      <div className="workspace">
        <header>
          <div className="workspace-heading">
            <button
              type="button"
              className="navigation-toggle"
              aria-controls="platform-navigation"
              aria-expanded={menuOpen}
              onClick={() => setMenuOpen(!menuOpen)}
            >
              {menuOpen ? "메뉴 닫기" : "메뉴 열기"}
            </button>
            <span>업무 공간</span>
          </div>
          <span>
            {user.name} <Button onClick={logout}>로그아웃</Button>
          </span>
        </header>
        <main>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
