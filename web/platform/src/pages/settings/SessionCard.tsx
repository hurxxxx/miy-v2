// Ported from AI-DO apps/web/src/platform/auth/SessionCard.tsx (0002af062).
// miy shows which surface the session signed in to, and no IP address: the
// Core does not take a client address from the reverse proxy.
import { Button } from '@miy/ui';
import { formatDateTime } from '../../platform/time/time-utils';

import type { BrowserSession, SettingsTranslator } from './settings-page-model';

export function SessionCard({
  session,
  timeZone,
  locale,
  onRevoke,
  t,
}: {
  session: BrowserSession;
  timeZone: string;
  locale: string;
  onRevoke: (sessionId: string) => Promise<void>;
  t: SettingsTranslator;
}) {
  return (
    <div className="flex items-start justify-between gap-4 rounded-md border border-app-border bg-app-bg px-4 py-3">
      <div className="app-text-body grid gap-1">
        <span className="font-medium text-app-ink">
          {session.current
            ? t('auth:settings.sessionCurrent')
            : t('auth:settings.sessionStored')}
          {' · '}
          {t(`portal:surfaces.${session.surface}`)}
        </span>
        <span className="app-text-caption text-app-ink/55">
          {t('auth:settings.sessionCreated')}:{' '}
          {formatDateTime(session.createdAt, {
            dateStyle: 'medium',
            fallback: t('common:empty.none'),
            locale,
            timeStyle: 'short',
            timeZone,
          })}
        </span>
        <span className="app-text-caption text-app-ink/55">
          {t('auth:settings.sessionExpires')}:{' '}
          {formatDateTime(session.expiresAt, {
            dateStyle: 'medium',
            fallback: t('common:empty.none'),
            locale,
            timeStyle: 'short',
            timeZone,
          })}
        </span>
        <span className="app-text-caption text-app-ink/55">
          {t('auth:settings.sessionLastSeen')}:{' '}
          {formatDateTime(session.lastSeenAt, {
            dateStyle: 'medium',
            fallback: t('common:empty.none'),
            locale,
            timeStyle: 'short',
            timeZone,
          })}
        </span>
        <span className="app-text-caption max-w-[360px] truncate text-app-ink/55">
          {session.userAgent ?? t('common:feedback.unknown')}
        </span>
      </div>
      {!session.current ? (
        <Button
          onClick={() => {
            void onRevoke(session.id);
          }}
          size="dense"
          variant="secondary"
        >
          {t('auth:settings.sessionRevoke')}
        </Button>
      ) : null}
    </div>
  );
}
