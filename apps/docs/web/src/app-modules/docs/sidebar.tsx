import { useCallback, useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import {
  ChevronDown,
  ChevronRight,
  FileText,
  Files,
  History,
  Lock,
  Plus,
  Settings,
  Share2,
  Trash2,
  User,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { AnimatePresence, LazyMotion, domAnimation, m } from 'motion/react';

import { cn } from '../../lib/utils';
import { useDocsApp } from '../../platform/docs/docs-context';
import {
  SETTINGS_PATH,
  useAppNavigate,
  type AppRoute,
} from '../../platform/runtime/app-location';
import { useAppApi } from '../../platform/runtime/runtime-context';
import {
  listFavoriteDocs,
  listRecentPages,
  type FavoriteDocItem,
  type RecentPageItem,
} from './api/docs-api';
import {
  docsSidebarRowSpaceLabel,
  docsTrashPath,
  getInitialDocsSidebarExpandedSections,
  isDocsSidebarSectionExpanded,
  resolveDocsSidebarRowPath,
  toggleDocsSidebarSection,
  type DocsSidebarSectionId,
} from './docs-sidebar-model';

/** A window event the docs screens fire after favorites or views change. */
export const DOCS_SIDEBAR_REFRESH_EVENT = 'docs:sidebar-refresh';

export function refreshDocsSidebar(): void {
  window.dispatchEvent(new CustomEvent(DOCS_SIDEBAR_REFRESH_EVENT));
}

interface DocsSidebarData {
  favorites: FavoriteDocItem[];
  recentPages: RecentPageItem[];
}

function DocsSidebarSection({
  children,
  expanded,
  id,
  title,
  onToggle,
}: {
  children: ReactNode;
  expanded: boolean;
  id: DocsSidebarSectionId;
  title: string;
  onToggle: (id: DocsSidebarSectionId) => void;
}) {
  const contentId = `docs-sidebar-section-${id}`;

  return (
    <div className="space-y-1 border-t border-app-border pt-2">
      <button
        type="button"
        aria-controls={contentId}
        aria-expanded={expanded}
        onClick={() => onToggle(id)}
        className="sidebar-section-label sidebar-section-header group/section flex w-full items-center gap-1 px-3 py-1"
      >
        {expanded ? (
          <ChevronDown
            size={11}
            className="text-app-ink/55 dark:text-app-ink/65 transition-colors group-hover/section:text-app-ink dark:group-hover/section:text-white"
          />
        ) : (
          <ChevronRight
            size={11}
            className="text-app-ink/55 dark:text-app-ink/65 transition-colors group-hover/section:text-app-ink dark:group-hover/section:text-white"
          />
        )}
        <span>{title}</span>
      </button>

      <LazyMotion features={domAnimation}>
        <AnimatePresence initial={false}>
          {expanded ? (
            <m.div
              id={contentId}
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: 'auto', opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              className="overflow-hidden"
            >
              {children}
            </m.div>
          ) : null}
        </AnimatePresence>
      </LazyMotion>
    </div>
  );
}

function DocsSidebarSpaceLabel({ label }: { label: string | null }) {
  if (!label) return null;
  return (
    <span className="app-text-micro ml-auto max-w-[45%] shrink-0 truncate text-app-ink/55">
      {label}
    </span>
  );
}

/**
 * AI-DO app-modules/docs/sidebar.tsx (DocsSidebarExtras): favorites and
 * recent pages across every docs space the person may enter
 * (`scope=docs_spaces`). Links become `runtime.navigate`.
 */
export function DocsSidebarExtras({
  currentSpaceId,
  route,
  locationKey,
}: {
  currentSpaceId: string | null;
  route: AppRoute;
  locationKey: string;
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const navigate = useAppNavigate();
  const [revision, setRevision] = useState(0);
  const [sidebarData, setSidebarData] = useState<DocsSidebarData>({
    favorites: [],
    recentPages: [],
  });
  const [expandedSections, setExpandedSections] = useState<
    DocsSidebarSectionId[]
  >(getInitialDocsSidebarExpandedSections);
  const { favorites, recentPages } = sidebarData;

  useEffect(() => {
    const refresh = () => setRevision((value) => value + 1);
    window.addEventListener(DOCS_SIDEBAR_REFRESH_EVENT, refresh);
    return () => window.removeEventListener(DOCS_SIDEBAR_REFRESH_EVENT, refresh);
  }, []);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      // Rows span every Docs space the user can enter.
      listFavoriteDocs(api, null, 'docs_spaces').catch(() => null),
      listRecentPages(api, 5, null, 'docs_spaces').catch(() => null),
    ]).then(([nextFavorites, nextRecentPages]) => {
      if (cancelled) return;
      // A failed refresh keeps the rows already shown.
      setSidebarData((current) => ({
        favorites: nextFavorites ?? current.favorites,
        recentPages: nextRecentPages ?? current.recentPages,
      }));
    });
    return () => {
      cancelled = true;
    };
  }, [api, revision, locationKey]);

  const toggleSection = useCallback((sectionId: DocsSidebarSectionId) => {
    setExpandedSections((current) =>
      toggleDocsSidebarSection(current, sectionId),
    );
  }, []);

  const activeDocId = route.kind === 'doc' ? route.docId : null;

  return (
    <div className="space-y-2">
      <DocsSidebarSection
        id="favorites"
        title={t('docs.sidebar.favorites')}
        expanded={isDocsSidebarSectionExpanded(expandedSections, 'favorites')}
        onToggle={toggleSection}
      >
        {favorites.length > 0 ? (
          favorites.map((favorite) => {
            const docPath = resolveDocsSidebarRowPath({
              docId: favorite.id,
              space: favorite.space,
              currentSpaceId,
            });
            const spaceLabel = docsSidebarRowSpaceLabel(
              favorite.space,
              currentSpaceId,
            );
            return (
              <button
                key={favorite.id}
                type="button"
                onClick={() => navigate(docPath)}
                className={cn(
                  'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
                  activeDocId === favorite.id && 'sidebar-submenu-item-active',
                )}
              >
                <FileText size={14} className="text-yellow-500" />
                <span className="sidebar-submenu-label truncate">
                  {favorite.title}
                </span>
                <DocsSidebarSpaceLabel label={spaceLabel} />
              </button>
            );
          })
        ) : (
          <div className="px-3 py-2 text-center">
            <span className="app-text-micro text-app-ink/70">
              {t('docs.sidebar.noFavorites')}
            </span>
          </div>
        )}
      </DocsSidebarSection>

      <DocsSidebarSection
        id="recentPages"
        title={t('docs.sidebar.recentPages')}
        expanded={isDocsSidebarSectionExpanded(expandedSections, 'recentPages')}
        onToggle={toggleSection}
      >
        {recentPages.length > 0 ? (
          recentPages.map((recentPage) => {
            const docPath = resolveDocsSidebarRowPath({
              docId: recentPage.doc_id,
              pageId: recentPage.page_id,
              space: recentPage.space,
              currentSpaceId,
            });
            const spaceLabel = docsSidebarRowSpaceLabel(
              recentPage.space,
              currentSpaceId,
            );
            return (
              <button
                key={recentPage.page_id}
                type="button"
                onClick={() => navigate(docPath)}
                className="sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left"
              >
                <FileText size={14} className="text-app-ink/55" />
                <span className="sidebar-submenu-label truncate">
                  {recentPage.page_title}
                </span>
                <DocsSidebarSpaceLabel label={spaceLabel} />
              </button>
            );
          })
        ) : (
          <div className="px-3 py-2 text-center">
            <span className="app-text-micro text-app-ink/70">
              {t('docs.sidebar.noRecentPages')}
            </span>
          </div>
        )}
      </DocsSidebarSection>
    </div>
  );
}

/**
 * AI-DO docs manifest navItems (the docs-notes item needs meetings: left
 * out). AI-DO's "보관됨" view listed trashed docs without a way back; miy
 * replaces it with the space's trash (`/trash`, restore and delete for good).
 */
const NAV_ITEMS = [
  { view: 'all', labelKey: 'docs.category.all', icon: Files },
  { view: 'mine', labelKey: 'docs.category.mine', icon: User },
  { view: 'shared', labelKey: 'docs.category.shared', icon: Share2 },
  { view: 'private', labelKey: 'docs.category.private', icon: Lock },
  { view: 'recent', labelKey: 'docs.category.recent', icon: History },
] as const;

export function docsListPath(spaceId: string | null, view?: string | null): string {
  const params = new URLSearchParams();
  if (spaceId && spaceId !== 'global') params.set('space', spaceId);
  if (view && view !== 'all') params.set('view', view);
  const query = params.toString();
  return query ? `/?${query}` : '/';
}

/**
 * The docs app's left column (miy). AI-DO drew the space chooser, the
 * library navigation and the create action in the portal shell
 * (manifest navItems, sidebar-config.ts `docs-create-doc`); the frame draws
 * them at its left edge. Settings (docs managers, spaces) is a miy screen.
 */
export function DocsSidebar({
  route,
  spaceId,
  activeView,
  locationKey,
}: {
  route: AppRoute;
  spaceId: string;
  activeView: string;
  locationKey: string;
}) {
  const { t } = useTranslation('apps');
  const navigate = useAppNavigate();
  const { me, spaces } = useDocsApp();
  const showSettings =
    Boolean(me?.isManager || me?.canBootstrap) ||
    spaces.some((space) => space.canManage);

  return (
    <nav
      aria-label={t('docs.docs')}
      className="flex h-full w-60 shrink-0 flex-col gap-4 overflow-y-auto border-r border-app-border bg-app-surface-sidebar px-2 py-3"
    >
      <div className="space-y-2 px-1">
        <label className="block">
          <span className="sidebar-section-label px-2 text-app-ink/45">
            {t('docsApp.spaces.label')}
          </span>
          <select
            aria-label={t('docsApp.spaces.label')}
            className="app-field-input-sm mt-1 w-full"
            value={spaces.some((space) => space.id === spaceId) ? spaceId : ''}
            onChange={(event) =>
              navigate(docsListPath(event.target.value || null, null))
            }
          >
            {spaces.some((space) => space.id === spaceId) ? null : (
              <option value="">{t('docsApp.spaces.unavailable')}</option>
            )}
            {spaces.map((space) => (
              <option key={space.id} value={space.id}>
                {space.kind === 'app_global'
                  ? t('docsApp.spaces.global')
                  : space.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          onClick={() => {
            const params = new URLSearchParams();
            if (spaceId !== 'global') params.set('space', spaceId);
            params.set('create', '1');
            navigate(`/?${params.toString()}`);
          }}
          className="app-text-control flex w-full items-center justify-center gap-2 rounded-md bg-app-accent px-3 py-2 text-app-accent-fg hover:opacity-90"
        >
          <Plus size={15} />
          <span>{t('home.actionNewDoc')}</span>
        </button>
      </div>

      <section className="space-y-1">
        <div className="sidebar-section-label px-3 py-1 text-app-ink/45">
          {t('docsApp.library')}
        </div>
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.view}
              type="button"
              onClick={() => navigate(docsListPath(spaceId, item.view))}
              className={cn(
                'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
                route.kind === 'list' &&
                  activeView === item.view &&
                  'sidebar-submenu-item-active',
              )}
            >
              <Icon size={16} className="text-app-ink/55 dark:text-app-ink/65" />
              <span className="sidebar-submenu-label">{t(item.labelKey)}</span>
            </button>
          );
        })}
        <button
          type="button"
          onClick={() => navigate(docsTrashPath(spaceId))}
          className={cn(
            'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
            route.kind === 'trash' && 'sidebar-submenu-item-active',
          )}
        >
          <Trash2 size={16} className="text-app-ink/55 dark:text-app-ink/65" />
          <span className="sidebar-submenu-label">{t('docsApp.trash.title')}</span>
        </button>
      </section>

      <DocsSidebarExtras
        currentSpaceId={spaceId}
        route={route}
        locationKey={locationKey}
      />

      {showSettings ? (
        <section className="mt-auto border-t border-app-border pt-2">
          <button
            type="button"
            onClick={() => navigate(SETTINGS_PATH)}
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              route.kind === 'settings' && 'sidebar-submenu-item-active',
            )}
          >
            <Settings size={16} className="text-app-ink/55 dark:text-app-ink/65" />
            <span className="sidebar-submenu-label">{t('docsApp.settings.title')}</span>
          </button>
        </section>
      ) : null}
    </nav>
  );
}
