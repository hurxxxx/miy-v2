import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Calendar, CheckCircle2, Layout, User } from 'lucide-react';

import { cn } from '../../../lib/utils';
import { Link, useLocation } from '../../../platform/router';
import { PMS_WORKSPACE_KEY } from '../../../platform/runtime/app-location';
import { PmsSidebarSpaces } from '../sidebar/PmsSidebarSpaces';

/**
 * The left column of the PMS frame (miy). AI-DO rendered the same pieces in
 * its portal shell's sub-sidebar (components/layout/SubSidebar.tsx with
 * app-modules/pms/sidebar/config.tsx): the app title, the Spaces category
 * (`PmsSidebarSpaces`) and the Personal category with the "My Tasks" group
 * (server nav items `pms-tasks`, `pms-tasks-assigned`, `pms-tasks-today`).
 * In miy the frame owns the whole work area, so the app draws it itself with
 * the same classes. The active item follows AI-DO's `pmsShellNavResolver`
 * (shell-nav.ts) on the app-relative path. The Inbox item (`pms-inbox`) is
 * not ported: the platform notification panel replaces PMS notifications.
 * AI-DO opened the overview from its space switcher ("PMS 전체"); here it is
 * the first link.
 */
export function resolvePmsActiveNavItemId(pathname: string): string {
  const path = pathname.replace(/\/+$/, '') || '/';
  if (path === '/assigned' || path.startsWith('/assigned/')) {
    return 'pms-tasks-assigned';
  }
  if (path === '/today' || path.startsWith('/today/')) {
    return 'pms-tasks-today';
  }
  const listMatch = path.match(/^\/lists\/([^/]+)/);
  if (listMatch?.[1]) return `pms-list-${decodeSegment(listMatch[1])}`;
  const spaceMatch = path.match(/^\/spaces\/([^/]+)/);
  if (spaceMatch?.[1]) return `pms-space-${decodeSegment(spaceMatch[1])}`;
  return path === '/' ? 'pms-overview' : '';
}

function decodeSegment(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

const TASK_CHILDREN = [
  { id: 'pms-tasks-assigned', icon: User, path: '/assigned', title: 'Assigned to me' },
  { id: 'pms-tasks-today', icon: Calendar, path: '/today', title: 'Today & Overdue' },
] as const;

export function PmsAppSidebar() {
  const { t } = useTranslation(['shell', 'apps']);
  const { pathname } = useLocation();
  const activeNavItemId = resolvePmsActiveNavItemId(pathname);
  const [spacesExpanded, setSpacesExpanded] = useState(true);
  const tasksActive =
    activeNavItemId === 'pms-tasks' || activeNavItemId.startsWith('pms-tasks-');
  return (
    <aside
      aria-label={t('apps:pms.sidebarLabel')}
      className="relative hidden h-full w-64 shrink-0 flex-col overflow-hidden border-r border-app-border bg-app-surface-sidebar md:flex"
    >
      <div className="flex items-center justify-between gap-2 border-b border-app-border p-4">
        <h2 className="app-text-overline text-app-ink/70 dark:text-app-ink/80">
          {t('apps:pms.title')}
        </h2>
      </div>
      <div className="custom-scrollbar flex min-h-0 flex-1 flex-col space-y-6 overflow-y-auto px-2 py-4">
        <Link
          to="/"
          aria-current={activeNavItemId === 'pms-overview' ? 'page' : undefined}
          className={cn(
            'sidebar-submenu-item ml-1',
            activeNavItemId === 'pms-overview' && 'sidebar-submenu-item-active',
          )}
        >
          <Layout size={16} className="text-app-ink/55 dark:text-app-ink/65" />
          <span className="sidebar-submenu-label">
            {t('appRuntime.appOverview', { app: t('apps:pms.title') })}
          </span>
        </Link>
        <PmsSidebarSpaces
          activeNavItemId={activeNavItemId}
          currentWorkspaceSlug={PMS_WORKSPACE_KEY}
          isExpanded={spacesExpanded}
          onToggle={() => setSpacesExpanded((current) => !current)}
        />
        <div className="space-y-1">
          <div
            className={cn(
              'sidebar-submenu-group ml-1 cursor-default',
              tasksActive && 'sidebar-submenu-item-active',
            )}
          >
            <CheckCircle2
              size={16}
              className={cn(
                'text-app-ink/55 dark:text-app-ink/65',
                tasksActive && 'text-app-accent',
              )}
            />
            <span className="sidebar-submenu-label">
              {t('nav.pms-tasks', { defaultValue: 'My Tasks' })}
            </span>
          </div>
          <div className="ml-6 space-y-1 border-l border-app-border pl-2">
            {TASK_CHILDREN.map((child) => {
              const ChildIcon = child.icon;
              const active = activeNavItemId === child.id;
              return (
                <Link
                  key={child.id}
                  to={child.path}
                  aria-current={active ? 'page' : undefined}
                  className={cn(
                    'sidebar-submenu-item',
                    active && 'sidebar-submenu-item-active',
                  )}
                >
                  <ChildIcon
                    size={14}
                    className="text-app-ink/55 dark:text-app-ink/65"
                  />
                  <span className="sidebar-submenu-label">
                    {t(`nav.${child.id}`, { defaultValue: child.title })}
                  </span>
                </Link>
              );
            })}
          </div>
        </div>
      </div>
    </aside>
  );
}
