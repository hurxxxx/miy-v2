import { Loader2 } from 'lucide-react';
import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppProps, NavigationState } from '@miy/sdk';
import { FeedbackProvider } from '@miy/components/ui/lib/feedback/feedback-provider';
import './app.css';

import { getDmMe } from './app-modules/dm/api/dm-api';
import type { DmThreadScrollSnapshots } from './app-modules/dm/views/dm-conversation-session-model';
import { bindApiRuntime } from './platform/api/client';
import { AuthProvider, RUNTIME_TOKEN, type AuthUser } from './platform/auth/auth-provider';
import { syncLocale } from './platform/i18n';
import { LiveProvider } from './platform/realtime/realtime-provider';
import { conversationPath, resolveAppRoute } from './platform/runtime/app-location';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
import { syncDateFormatPreference, syncTimeZonePreference } from './platform/time/time-utils';

// Loaded on demand like AI-DO's floating DM panel (FloatingDmWidget lazily
// imported DMView); the report review screen is separate.
const DMEmbeddedView = lazy(() =>
  import('./app-modules/dm/views/DMView').then((module) => ({
    default: module.DMEmbeddedView,
  })),
);
const DmSafetyPage = lazy(() =>
  import('./platform/admin/DmSafetyPage').then((module) => ({
    default: module.DmSafetyPage,
  })),
);

/**
 * Messenger (메신저). Routes inside the app frame:
 * - `/` and `/conversations/{id}`: the conversation list and a conversation
 *   (notifications open the conversation path);
 * - `/widgets/conversations[/{id}]`: the same screens for the portal's narrow
 *   side panel (manifest `panels`);
 * - `/safety` and `/safety/reports/{id}`: report review for messenger managers.
 */
export default function AppRoot({ runtime }: AppProps) {
  // Display settings are portal choices delivered on every render (AI-DO
  // synced the same stores from the signed-in user's profile).
  syncLocale(runtime.settings.locale);
  syncDateFormatPreference(runtime.settings.dateFormat);
  syncTimeZonePreference(runtime.settings.timeZone);
  bindApiRuntime(runtime);
  return (
    <RuntimeProvider runtime={runtime}>
      <Feedback>
        <LiveProvider live={runtime.live} activity={runtime.activity}>
          <Session runtime={runtime} />
        </LiveProvider>
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

function Loading() {
  return (
    <div className="flex h-screen items-center justify-center bg-app-bg text-app-ink/45">
      <Loader2 aria-hidden="true" className="animate-spin" size={20} />
    </div>
  );
}

function Session({ runtime }: AppProps) {
  const { t } = useTranslation('apps');
  const [me, setMe] = useState<{ id: string; full_name: string } | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const { api, settings } = runtime;
  useEffect(() => {
    const controller = new AbortController();
    setFailed(false);
    getDmMe(RUNTIME_TOKEN, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) setMe({ id: value.user.id, full_name: value.user.full_name });
      })
      .catch(() => {
        if (!controller.signal.aborted) setFailed(true);
      });
    return () => controller.abort();
  }, [api, attempt]);
  const auth = useMemo(() => {
    const user: AuthUser | null = me
      ? {
          ...me,
          time_zone: settings.timeZone,
          date_format: settings.dateFormat,
          locale: settings.locale,
        }
      : null;
    return { token: user ? RUNTIME_TOKEN : null, user };
  }, [me, settings.dateFormat, settings.locale, settings.timeZone]);
  if (!me) {
    return failed ? (
      <div className="flex h-screen flex-col items-center justify-center gap-3 bg-app-bg text-app-ink">
        <p className="app-text-body text-app-ink/65">{t('dm.errors.threadsLoadFailed')}</p>
        <button
          className="app-text-body-sm rounded-md border border-app-border px-3 py-1.5"
          onClick={() => setAttempt((current) => current + 1)}
          type="button"
        >
          {t('common:actions.retry', { defaultValue: 'Retry' })}
        </button>
      </div>
    ) : (
      <Loading />
    );
  }
  return (
    <AuthProvider value={auth}>
      <Screens runtime={runtime} />
    </AuthProvider>
  );
}

function Screens({ runtime }: AppProps) {
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
  // AI-DO kept per-thread scroll positions in the shell across panel opens.
  const scrollSnapshots = useRef<DmThreadScrollSnapshots>({});
  const kind = route.kind === 'panel' ? 'panel' : 'messenger';
  const onThreadIdChange = useCallback(
    (threadId: string) => navigate(conversationPath(kind, threadId)),
    [kind, navigate],
  );
  return (
    <NavigationStateProvider message={t('dm.unsavedChanges')} report={report}>
      <div className="h-screen min-h-0">
        <Suspense fallback={<Loading />}>
          {route.kind === 'safety' ? (
            <DmSafetyPage onBack={() => navigate('/')} reportId={route.reportId} />
          ) : (
            <DMEmbeddedView
              embeddedPersistentList={route.kind === 'messenger'}
              focusMessageId={new URLSearchParams(runtime.location.search).get('message')}
              onOpenSafety={route.kind === 'messenger' ? () => navigate('/safety') : undefined}
              onThreadIdChange={onThreadIdChange}
              threadScrollSnapshotsRef={scrollSnapshots}
              threadId={route.threadId}
            />
          )}
        </Suspense>
      </div>
    </NavigationStateProvider>
  );
}
