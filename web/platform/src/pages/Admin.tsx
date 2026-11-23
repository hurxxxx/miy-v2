import { useState } from "react";
import { Link, useLocation } from "@tanstack/react-router";
import { AppWindow, ClipboardList, LayoutDashboard, LayoutGrid, LifeBuoy, Network, UserRound, UsersRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSurface, useUser } from "../data/session";
import { Panel } from "../ui";
import { AdminApps } from "./AdminApps";
import { AuditLog } from "./AdminAudit";
import { OrganizationAdmin, PermissionGroupsAdmin } from "./AdminGroups";
import { LauncherCategoriesAdmin } from "./AdminLauncher";
import { AdminOverview } from "./AdminOverview";
import { SupportReports } from "./AdminReports";
import { UserAdmin } from "./AdminUsers";

const sections = [
  { label: null, items: [{ id: "overview", to: "/admin", icon: LayoutDashboard }] },
  { label: "directory", items: [
    { id: "users", to: "/admin/users", icon: UserRound },
    { id: "organization", to: "/admin/organization", icon: Network },
    { id: "groups", to: "/admin/groups", icon: UsersRound },
  ] },
  { label: "platform", items: [
    { id: "apps", to: "/admin/apps", icon: AppWindow },
    { id: "launcher", to: "/admin/launcher", icon: LayoutGrid },
  ] },
  { label: "operations", items: [
    { id: "audit", to: "/admin/audit", icon: ClipboardList },
    { id: "support", to: "/admin/support", icon: LifeBuoy },
  ] },
] as const;

type Section = (typeof sections)[number]["items"][number]["id"];

export function Admin() {
  const { t } = useTranslation("console");
  const user = useUser();
  const { surface } = useSurface();
  const pathname = useLocation({ select: (location) => location.pathname });
  if (surface !== "console" || !user.admin) {
    return <Panel title={t("admin.adminRequired")}><p>{t("admin.signInAsAdmin")}</p></Panel>;
  }

  const section = sections.flatMap(({ items }) => [...items]).find(({ to }) =>
    pathname === to || (to !== "/admin" && pathname.startsWith(`${to}/`)),
  )?.id ?? "overview";

  return (
    <div className="admin-workspace">
      <nav aria-label={t("admin.title")} className="admin-navigation">
        {sections.map(({ label, items }) => (
          <div key={label ?? "overview"} className="admin-navigation-group">
            {label && <p>{t(`navigation.categories.${label}`)}</p>}
            {items.map(({ id, to, icon: Icon }) => (
              <Link key={id} to={to} activeOptions={{ exact: id !== "apps", includeSearch: false }}
                className="admin-navigation-link">
                <Icon size={17} aria-hidden="true" />
                {t(`navigation.sections.${id}`)}
              </Link>
            ))}
          </div>
        ))}
      </nav>
      <div className="admin-workspace-content">
        <div className="page-header">
          <h1>{section === "overview" ? t("admin.title") : t(`navigation.sections.${section}`)}</h1>
        </div>
        <AdminSection key={section} section={section} csrf={user.csrf} selfId={user.id} />
      </div>
    </div>
  );
}

function AdminSection({ section, csrf, selfId }: { section: Section; csrf: string; selfId: string }) {
  const [message, report] = useState("");
  const props = { csrf, report };
  const pages = {
    overview: <AdminOverview />,
    users: <UserAdmin {...props} selfId={selfId} />,
    organization: <OrganizationAdmin {...props} />,
    groups: <PermissionGroupsAdmin {...props} />,
    apps: <AdminApps {...props} />,
    launcher: <LauncherCategoriesAdmin csrf={csrf} />,
    audit: <AuditLog report={report} />,
    support: <SupportReports csrf={csrf} />,
  };
  return (
    <>
      {message && <p role="status" className="status-message">{message}</p>}
      {pages[section]}
    </>
  );
}
