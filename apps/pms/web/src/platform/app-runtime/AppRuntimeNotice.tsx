import { useTranslation } from 'react-i18next';
import { Link } from '../router';
import { ContentState } from '@miy/ui';

export type AppRuntimeNoticeStatus =
  | 'loading'
  | 'bootstrap_error'
  | 'app_denied'
  | 'space_denied'
  | 'space_archived'
  | 'coming_soon';

const messageKeys = {
  loading: 'appRuntime.loading',
  bootstrap_error: 'appRuntime.loadFailed',
  app_denied: 'appRuntime.appDenied',
  space_denied: 'appRuntime.spaceDenied',
  space_archived: 'appRuntime.spaceArchived',
  coming_soon: 'appRuntime.comingSoon',
} as const;

const kinds = {
  loading: 'loading',
  bootstrap_error: 'error',
  app_denied: 'error',
  space_denied: 'error',
  space_archived: 'unavailable',
  coming_soon: 'unavailable',
} as const;

/** Retrying only helps a failed load; a denial instead offers a way out. */
export function AppRuntimeNotice({
  status,
  onRetry,
  backHref = '/',
  action,
}: {
  status: AppRuntimeNoticeStatus;
  onRetry?: () => void;
  /** Where a denied user can continue; `/` opens the first accessible app. */
  backHref?: string | null;
  action?: { label: string; onClick: () => void };
}) {
  const { t } = useTranslation('shell');
  const back =
    backHref !== null &&
    (status === 'app_denied' ||
      status === 'space_denied' ||
      status === 'space_archived')
      ? backHref
      : null;
  const hint =
    status === 'space_archived' ? t('appRuntime.spaceArchivedHint') : null;
  return (
    <ContentState
      className="m-4"
      kind={kinds[status]}
      title={t(messageKeys[status])}
      description={
        hint || back ? (
          <>
            {hint ? <span className="block">{hint}</span> : null}
            {back ? (
              <span className="mt-3 block">
                <Link
                  className="inline-flex rounded-lg border border-app-border px-3 py-1.5 font-medium text-app-ink hover:bg-app-surface-hover"
                  to={back}
                >
                  {t(
                    back === '/'
                      ? 'appRuntime.goToStart'
                      : 'appRuntime.backToApp',
                  )}
                </Link>
              </span>
            ) : null}
          </>
        ) : undefined
      }
      action={
        status === 'bootstrap_error' && onRetry
          ? { label: t('appRuntime.retry'), onClick: onRetry }
          : action
      }
    />
  );
}
