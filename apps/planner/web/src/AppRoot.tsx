import { Suspense, lazy, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppProps, NavigationState } from '@miy/sdk';
import './app.css';

import { syncLocale } from './platform/i18n';
import { PersonalWidgetPanelRoute, PersonalWidgetHost } from './platform/personal-widgets/PersonalWidgetHost';
import { resolveAppRoute, WIDGET_ROUTE_PATHS } from './platform/runtime/app-location';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
import { syncDateFormatPreference, syncTimeZonePreference } from './platform/time/time-utils';

// Loaded on demand like AI-DO's planner route (app-modules/planner/routes.ts),
// so the narrow widget routes do not load FullCalendar.
const PlannerView = lazy(() =>
  import('./app-modules/planner/views/PlannerView').then((module) => ({
    default: module.PlannerView,
  })),
);

/**
 * Planner (플래너). Routes inside the app frame:
 * - `/` (any other path): the planner, with the to-do/memo dock at the right
 *   edge as in AI-DO's shell;
 * - `/widgets/todos`, `/widgets/memos`: the to-do or memo panel alone, for a
 *   narrow secondary frame.
 */
export default function AppRoot({ runtime }: AppProps) {
  // Display settings are portal choices delivered on every render (AI-DO
  // synced the same stores from the signed-in user's profile).
  syncLocale(runtime.settings.locale);
  syncDateFormatPreference(runtime.settings.dateFormat);
  syncTimeZonePreference(runtime.settings.timeZone);
  return (
    <RuntimeProvider runtime={runtime}>
      <AppScreens runtime={runtime} />
    </RuntimeProvider>
  );
}

function AppScreens({ runtime }: AppProps) {
  const { t } = useTranslation('apps');
  const route = useMemo(
    () => resolveAppRoute(runtime.location.pathname),
    [runtime.location.pathname],
  );
  const { navigate, setNavigationState } = runtime;
  const report = useCallback(
    (state: NavigationState) => setNavigationState(state),
    [setNavigationState],
  );
  const openPanelRoute = useCallback(
    (widget: 'todo' | 'memo') => navigate(WIDGET_ROUTE_PATHS[widget]),
    [navigate],
  );
  return (
    <NavigationStateProvider
      report={report}
      message={t('planner.unsavedChanges')}
    >
      {route.kind === 'widget' ? (
        <div className="h-screen min-h-0">
          <PersonalWidgetPanelRoute key={route.widget} widget={route.widget} />
        </div>
      ) : (
        <div className="flex h-screen min-h-0">
          <div className="min-w-0 flex-1">
            <Suspense
              fallback={
                <p className="app-text-caption p-6 text-app-ink/55">
                  {t('common:feedback.loading')}
                </p>
              }
            >
              <PlannerView />
            </Suspense>
          </div>
          <PersonalWidgetHost onOpenPanelRoute={openPanelRoute} />
        </div>
      )}
    </NavigationStateProvider>
  );
}
