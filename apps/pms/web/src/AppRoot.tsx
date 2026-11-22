import { Suspense, lazy, useCallback, useMemo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { LazyMotion, domAnimation } from 'motion/react';
import type { AppProps, NavigationState } from '@miy/sdk';
import { FeedbackProvider } from '@miy/ui';
import './app.css';

import { PmsAppSidebar } from './app-modules/pms/shell/PmsAppSidebar';
import { PmsCurrentUserProvider } from './platform/auth/current-user';
import { syncLocale } from './platform/i18n';
import { resolveAppRoute, type AppRoute } from './platform/runtime/app-location';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
import {
  syncDateFormatPreference,
  syncTimeZonePreference,
} from './platform/time/time-utils';

// Loaded on demand like AI-DO's PMS routes (app-modules/pms/routes.ts).
const PMSView = lazy(() =>
  import('./app-modules/pms/views/PMSView').then((module) => ({
    default: module.PMSView,
  })),
);
const AssignedToMeView = lazy(() =>
  import('./app-modules/pms/views/AssignedToMeView').then((module) => ({
    default: module.AssignedToMeView,
  })),
);
const TodayOverdueView = lazy(() =>
  import('./app-modules/pms/views/TodayOverdueView').then((module) => ({
    default: module.TodayOverdueView,
  })),
);
const PmsResourceTaskView = lazy(() =>
  import('./app-modules/pms/views/PmsResourceTaskView').then((module) => ({
    default: module.PmsResourceTaskView,
  })),
);
const FloatingPmsWidget = lazy(() =>
  import('./app-modules/pms/views/FloatingPmsWidget').then((module) => ({
    default: module.FloatingPmsWidget,
  })),
);

/**
 * PMS. Routes inside the app frame (app-relative, see
 * platform/runtime/app-location.ts):
 * - `/`, `/spaces/:spaceId`, `/lists/:taskListId` — PMSView (overview, space,
 *   list with AI-DO's query parameters such as `?task=`, `?tab=`);
 * - `/assigned`, `/today` — my tasks views;
 * - `/tasks/:taskId` — notification / home card destination;
 * - `/widgets/my-tasks` — the side panel (AI-DO's floating PMS widget).
 * The wide routes show AI-DO's PMS sidebar as the frame's left column.
 */
export default function AppRoot({ runtime }: AppProps) {
  // Display settings are portal choices delivered on every render (AI-DO
  // synced the same stores from the signed-in user's profile).
  syncLocale(runtime.settings.locale);
  syncDateFormatPreference(runtime.settings.dateFormat);
  syncTimeZonePreference(runtime.settings.timeZone);
  return (
    <RuntimeProvider runtime={runtime}>
      <AppFeedback>
        {/* AI-DO's shell (app/shell/AppContent.tsx) wrapped app screens in the
            same motion features; PMS uses `m.*` components. */}
        <LazyMotion features={domAnimation}>
          <AppScreens runtime={runtime} />
        </LazyMotion>
      </AppFeedback>
    </RuntimeProvider>
  );
}

/** AI-DO's app root provided the feedback toasts with these labels. */
function AppFeedback({ children }: { children: ReactNode }) {
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
  const fallback = (
    <p className="app-text-caption p-6 text-app-ink/55">
      {t('common:feedback.loading')}
    </p>
  );
  return (
    <NavigationStateProvider report={report} message={t('pms.unsavedChanges')}>
      <PmsCurrentUserProvider>
        {route.kind === 'widget' ? (
          <div className="h-screen min-h-0">
            <Suspense fallback={fallback}>
              <FloatingPmsWidget />
            </Suspense>
          </div>
        ) : (
          <div className="flex h-screen min-h-0 bg-app-bg text-app-ink">
            <PmsAppSidebar />
            <div className="min-w-0 flex-1 overflow-hidden">
              <Suspense fallback={fallback}>
                <RouteScreen route={route} />
              </Suspense>
            </div>
          </div>
        )}
      </PmsCurrentUserProvider>
    </NavigationStateProvider>
  );
}

function RouteScreen({ route }: { route: AppRoute }) {
  switch (route.kind) {
    case 'assigned':
      return <AssignedToMeView />;
    case 'today':
      return <TodayOverdueView />;
    case 'task':
      return <PmsResourceTaskView key={route.taskId} />;
    default:
      return <PMSView />;
  }
}
