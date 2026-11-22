// Before anything loads Excalidraw: self-hosted fonts under the frame CSP.
import './platform/excalidraw/self-hosted-assets';
import { Suspense, lazy, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppProps, NavigationState } from '@miy/sdk';
import { FeedbackProvider } from '@miy/ui';
import './app.css';

import { WhiteboardSidebar } from './app-modules/whiteboard/sidebar';
import { syncLocale } from './platform/i18n';
import { resolveAppRoute } from './platform/runtime/app-location';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
import {
  syncDateFormatPreference,
  syncTimeZonePreference,
} from './platform/time/time-utils';

// Loaded on demand like AI-DO's whiteboard route (app-modules/whiteboard/routes.ts).
const WhiteboardView = lazy(() =>
  import('./app-modules/whiteboard/views/WhiteboardView').then((module) => ({
    default: module.WhiteboardView,
  })),
);

/**
 * Whiteboard (화이트보드). Routes inside the app frame
 * (platform/runtime/app-location.ts):
 * - `/?view=&space=` — the hub (AI-DO `/w/:workspaceSlug/whiteboard`);
 * - `/<id>?space=` — one board (AI-DO `/w/:workspaceSlug/whiteboard/:id`).
 */
export default function AppRoot({ runtime }: AppProps) {
  // Display settings are portal choices delivered on every render.
  syncLocale(runtime.settings.locale);
  syncDateFormatPreference(runtime.settings.dateFormat);
  syncTimeZonePreference(runtime.settings.timeZone);
  return (
    <RuntimeProvider runtime={runtime}>
      <Feedback>
        <AppScreens runtime={runtime} />
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
    <NavigationStateProvider report={report} message={t('whiteboardApp.unsavedChanges')}>
      <div className="flex h-screen min-h-0 bg-app-bg text-app-ink">
        <WhiteboardSidebar route={route} />
        <div className="flex min-w-0 flex-1 flex-col">
          <Suspense
            fallback={
              <p className="app-text-caption p-6 text-app-ink/55">
                {t('whiteboardApp.loading')}
              </p>
            }
          >
            <WhiteboardView
              whiteboardId={route.kind === 'board' ? route.whiteboardId : null}
            />
          </Suspense>
        </div>
      </div>
    </NavigationStateProvider>
  );
}
