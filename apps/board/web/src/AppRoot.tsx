import { Suspense, lazy, useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppProps, NavigationState } from '@miy/sdk';
import { Button, FeedbackProvider, useFeedback } from '@miy/ui';
import './app.css';

import { BoardSidebar } from './app-modules/community/sidebar';
import { NoticesView } from './app-modules/announcements/views/NoticesView';
import { syncLocale } from './platform/i18n';
import { apiErrorMessage } from './platform/api/api-error';
import { bootstrapBoardManager } from './platform/board/board-api';
import { BoardProvider, useBoard } from './platform/board/board-context';
import { resolveAppRoute } from './platform/runtime/app-location';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider, useRuntime } from './platform/runtime/runtime-context';
import {
  syncDateFormatPreference,
  syncTimeZonePreference,
} from './platform/time/time-utils';

// Loaded on demand like AI-DO's community route (app-modules/community/routes.ts):
// the Markdown editor (Milkdown Crepe) is only needed there and in settings.
const CommunityView = lazy(() =>
  import('./app-modules/community/views/CommunityView').then((module) => ({
    default: module.CommunityView,
  })),
);
const BoardAdminView = lazy(() =>
  import('./app-modules/board-admin/views/BoardAdminView').then((module) => ({
    default: module.BoardAdminView,
  })),
);

/**
 * Board (게시판). Routes inside the app frame (platform/runtime/app-location.ts):
 * - `/`, `/community?channel=` — the channel's post list (AI-DO CommunityView);
 * - `/community/posts/<id>?channel=` — one post with its comments;
 * - `/notices?space=`, `/notices/<id>` — space and company notices;
 * - `/admin` — board settings for board managers.
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
        <BoardProvider>
          <AppScreens runtime={runtime} />
        </BoardProvider>
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

function AppScreens({ runtime }: AppProps) {
  const { t } = useTranslation('apps');
  const route = useMemo(
    () => resolveAppRoute(runtime.location.pathname),
    [runtime.location.pathname],
  );
  const { setNavigationState } = runtime;
  const report = useCallback(
    (state: NavigationState) => setNavigationState(state),
    [setNavigationState],
  );
  return (
    <NavigationStateProvider report={report} message={t('board.unsavedChanges')}>
      <div className="flex h-screen min-h-0 bg-app-bg text-app-ink">
        <BoardSidebar route={route} />
        <div className="flex min-w-0 flex-1 flex-col">
          <BootstrapBanner />
          <div className="min-h-0 flex-1">
            <Suspense
              fallback={
                <p className="app-text-caption p-6 text-app-ink/55">
                  {t('board.loading')}
                </p>
              }
            >
              {route.kind === 'notices' ? (
                <NoticesView noticeId={route.noticeId} />
              ) : route.kind === 'admin' ? (
                <BoardAdminView />
              ) : (
                <CommunityView postId={route.postId} />
              )}
            </Suspense>
          </div>
        </div>
      </div>
    </NavigationStateProvider>
  );
}

/** While the board has no manager, anyone may claim the role once (managers.rs). */
function BootstrapBanner() {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const { me, reload } = useBoard();
  const feedback = useFeedback();
  const [busy, setBusy] = useState(false);
  if (!me?.canBootstrap) return null;
  const claim = async () => {
    setBusy(true);
    try {
      await bootstrapBoardManager(api);
      feedback.success(t('board.bootstrap.claimed'));
    } catch (error) {
      feedback.error(apiErrorMessage(error, t('board.bootstrap.failed')));
    } finally {
      setBusy(false);
      reload();
    }
  };
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-app-border bg-app-surface px-4 py-3">
      <div className="min-w-0">
        <p className="app-text-body-sm font-semibold text-app-ink">
          {t('board.bootstrap.title')}
        </p>
        <p className="app-text-caption text-app-ink/60">{t('board.bootstrap.body')}</p>
      </div>
      <Button disabled={busy} size="dense" variant="secondary" onClick={() => void claim()}>
        {t('board.bootstrap.claim')}
      </Button>
    </div>
  );
}
