import { useCallback, useEffect, useMemo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppProps, NavigationState } from '@miy/sdk';
import { Button, ContentState, FeedbackProvider } from '@miy/ui';
import { HardDrive, Loader2 } from 'lucide-react';
import './app.css';

import { FileUploadProvider } from './app-modules/files/file-upload-provider';
import { DriveSidebar } from './app-modules/files/sidebar';
import { FileManagerView } from './app-modules/files/views/FileManagerView';
import { FileSearchView } from './app-modules/files/views/FileSearchView';
import { TrashView } from './app-modules/files/views/TrashView';
import { syncLocale } from './platform/i18n';
import { apiErrorMessage } from './platform/api/api-error';
import {
  DriveProvider,
  readLastSpace,
  rememberLastSpace,
  useDrive,
} from './platform/drive/drive-context';
import {
  buildSpacePath,
  resolveAppRoute,
  useAppNavigate,
} from './platform/runtime/app-location';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
import {
  syncDateFormatPreference,
  syncTimeZonePreference,
} from './platform/time/time-utils';

/**
 * Drive (드라이브). Routes inside the app frame (platform/runtime/app-location.ts):
 * - `/` — opens the last or first space (or offers to create one);
 * - `/spaces/<id>?folder=` — the file manager (AI-DO FileManagerView);
 * - `/spaces/<id>?view=search&q=` — name and metadata search (FileSearchView);
 * - `/spaces/<id>/trash` — the trash (보강).
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
        <Navigation runtime={runtime}>
          <DriveProvider>
            <FileUploadProvider>
              <AppScreens runtime={runtime} />
            </FileUploadProvider>
          </DriveProvider>
        </Navigation>
      </Feedback>
    </RuntimeProvider>
  );
}

function Feedback({ children }: { children: ReactNode }) {
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

function Navigation({ runtime, children }: AppProps & { children: ReactNode }) {
  const { t } = useTranslation('apps');
  const { setNavigationState } = runtime;
  const report = useCallback(
    (state: NavigationState) => setNavigationState(state),
    [setNavigationState],
  );
  return (
    <NavigationStateProvider report={report} message={t('drive.unsavedChanges')}>
      {children}
    </NavigationStateProvider>
  );
}

function AppScreens({ runtime }: AppProps) {
  const route = useMemo(
    () => resolveAppRoute(runtime.location.pathname),
    [runtime.location.pathname],
  );
  const params = useMemo(
    () => new URLSearchParams(runtime.location.search),
    [runtime.location.search],
  );
  const searchSelected = params.get('view') === 'search';
  const activeFolderId = params.get('folder');
  useEffect(() => {
    if (route.kind !== 'home') rememberLastSpace(route.spaceId);
  }, [route]);
  return (
    <div className="flex h-screen min-h-0 bg-app-bg text-app-ink">
      <DriveSidebar
        activeFolderId={activeFolderId}
        route={route}
        searchSelected={searchSelected}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        {route.kind === 'home' ? (
          <SpacePicker />
        ) : route.kind === 'trash' ? (
          <TrashView key={route.spaceId} spaceId={route.spaceId} />
        ) : searchSelected ? (
          <FileSearchView key={`search:${route.spaceId}`} spaceId={route.spaceId} />
        ) : (
          <FileManagerView key={`manager:${route.spaceId}`} spaceId={route.spaceId} />
        )}
      </div>
    </div>
  );
}

/** `/`: AI-DO opened the current workspace; drive opens the last space. */
function SpacePicker() {
  const { t } = useTranslation(['apps', 'common']);
  const navigate = useAppNavigate();
  const { spaces, loading, error, reload } = useDrive();
  useEffect(() => {
    if (loading || spaces.length === 0) return;
    const last = readLastSpace();
    const target = spaces.find((space) => space.id === last) ?? spaces[0];
    navigate(buildSpacePath(target.id), { replace: true });
  }, [loading, navigate, spaces]);
  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-app-ink/55">
        <Loader2 size={24} className="animate-spin" />
      </div>
    );
  }
  if (error) {
    return (
      <div className="p-6">
        <ContentState
          kind="error"
          size="fill"
          title={apiErrorMessage(error, t('drive.spaces.loadFailed'))}
        />
        <div className="mt-3 flex justify-center">
          <Button onClick={() => void reload()}>{t('drive.retry')}</Button>
        </div>
      </div>
    );
  }
  if (spaces.length > 0) return null;
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <span className="flex size-12 items-center justify-center rounded-md bg-app-accent/12 text-app-accent">
        <HardDrive size={24} />
      </span>
      <p className="app-text-body text-app-ink/70">{t('drive.spaces.none')}</p>
    </div>
  );
}
