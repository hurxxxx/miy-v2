import { Suspense, lazy, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppProps, NavigationState } from '@miy/sdk';
import { FeedbackProvider } from '@miy/ui';
import './app.css';

import { DocsSidebar } from './app-modules/docs/sidebar';
import { useHasCommentDrafts } from './app-modules/docs/views/docs-comment-drafts';
import { syncLocale } from './platform/i18n';
import { DocsAppProvider } from './platform/docs/docs-context';
import { resolveAppRoute } from './platform/runtime/app-location';
import {
  NavigationStateProvider,
  useNavigationDirty,
} from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
import {
  syncDateFormatPreference,
  syncTimeZonePreference,
} from './platform/time/time-utils';

// Loaded on demand like AI-DO's docs route (app-modules/docs/routes.ts): the
// block editor (BlockNote with Mantine) is only needed there.
const DocsView = lazy(() =>
  import('./app-modules/docs/views/DocsView').then((module) => ({
    default: module.DocsView,
  })),
);
const DocsTrashView = lazy(() =>
  import('./app-modules/docs-trash/DocsTrashView').then((module) => ({
    default: module.DocsTrashView,
  })),
);
const DocsSettingsView = lazy(() =>
  import('./app-modules/docs-settings/DocsSettingsView').then((module) => ({
    default: module.DocsSettingsView,
  })),
);

/**
 * Docs (문서). Routes inside the app frame (platform/runtime/app-location.ts):
 * - `/?space=&view=` — the docs of a space (AI-DO docs hub);
 * - `/<docId>?space=&page=` — one doc with its page tree and editor;
 * - `/trash?space=` — the space's trash: restore or delete for good (miy);
 * - `/settings` — docs managers and spaces (miy).
 */
export default function AppRoot({ runtime }: AppProps) {
  // Display settings are portal choices delivered on every render (AI-DO
  // synced the same stores from the signed-in user's profile).
  syncLocale(runtime.settings.locale);
  syncDateFormatPreference(runtime.settings.dateFormat);
  syncTimeZonePreference(runtime.settings.timeZone);
  return (
    <RuntimeProvider runtime={runtime}>
      <Feedback>
        <DocsAppProvider>
          <AppScreens runtime={runtime} />
        </DocsAppProvider>
      </Feedback>
    </RuntimeProvider>
  );
}

function Feedback({ children }: { children: React.ReactNode }) {
  const { t } = useTranslation('common');
  return (
    <FeedbackProvider
      labels={{
        close: t('actions.close'),
        item: t('feedback.itemLabel'),
        region: t('feedback.regionLabel'),
      }}
    >
      {children}
    </FeedbackProvider>
  );
}

/** Unsent comments of any page count as unsaved work (miy 보강). */
function CommentDraftsGuard() {
  useNavigationDirty(useHasCommentDrafts());
  return null;
}

function AppScreens({ runtime }: AppProps) {
  const { t } = useTranslation('apps');
  const { pathname, search } = runtime.location;
  const route = useMemo(() => resolveAppRoute(pathname), [pathname]);
  const params = useMemo(() => new URLSearchParams(search), [search]);
  const spaceId = params.get('space')?.trim() || 'global';
  const view = params.get('view') || 'all';
  const { setNavigationState } = runtime;
  const report = useCallback(
    (state: NavigationState) => setNavigationState(state),
    [setNavigationState],
  );
  return (
    <NavigationStateProvider report={report} message={t('docsApp.unsavedChanges')}>
      <CommentDraftsGuard />
      <div className="flex h-screen min-h-0 bg-app-bg text-app-ink">
        <DocsSidebar
          route={route}
          spaceId={spaceId}
          activeView={view}
        />
        <div className="flex min-w-0 flex-1 flex-col">
          <Suspense
            fallback={
              <p className="app-text-caption p-6 text-app-ink/55">
                {t('docsApp.loading')}
              </p>
            }
          >
            {route.kind === 'settings' ? (
              <DocsSettingsView />
            ) : route.kind === 'trash' ? (
              <DocsTrashView />
            ) : (
              <DocsView docId={route.kind === 'doc' ? route.docId : null} />
            )}
          </Suspense>
        </div>
      </div>
    </NavigationStateProvider>
  );
}
