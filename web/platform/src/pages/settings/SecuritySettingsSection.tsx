// Ported from AI-DO apps/web/src/platform/auth/SecuritySettingsSection.tsx
// (0002af062). The password changes on the identity provider's own form, so
// this section links to it instead of holding password fields; the CLI
// sessions keep their existing page.
import { ExternalLink, KeyRound, Terminal } from 'lucide-react';
import { Button, StatusSlot } from '@miy/ui';

import { SettingsFieldRow } from './SettingsFieldRow';
import { SettingsSectionHeader } from './SettingsSectionHeader';
import { SessionCard } from './SessionCard';
import {
  selectVisibleAuthSessions,
  type SecurityState,
  type SettingsTranslator,
} from './settings-page-model';

export function SecuritySettingsSection({
  cliSessionsHref,
  i18nLanguage,
  onChangePassword,
  onRevoke,
  passwordUrl,
  state,
  t,
  timeZone,
}: {
  cliSessionsHref: string;
  i18nLanguage: string;
  onChangePassword: () => void;
  onRevoke: (sessionId: string) => Promise<void>;
  passwordUrl?: string;
  state: SecurityState;
  t: SettingsTranslator;
  timeZone: string;
}) {
  const { hiddenCount, shownSessions } = selectVisibleAuthSessions(
    state.sessions,
  );

  return (
    <div>
      <SettingsSectionHeader
        title={t('auth:settings.security')}
        description={t(
          passwordUrl
            ? 'auth:settings.managePassword'
            : 'auth:settings.reviewSessions',
        )}
      />

      <div className="border-t border-app-border">
        {passwordUrl ? (
          <SettingsFieldRow
            label={t('auth:settings.updatePassword')}
            description={t('portal:settings.passwordOnIdentityProvider')}
          >
            <Button variant="secondary" onClick={onChangePassword}>
              <KeyRound size={15} />
              {t('auth:settings.updatePassword')}
            </Button>
          </SettingsFieldRow>
        ) : null}
        <SettingsFieldRow
          label={t('portal:settings.cliSessions')}
          description={t('portal:settings.cliSessionsDescription')}
        >
          <a
            className="app-control inline-flex"
            href={cliSessionsHref}
            rel="noopener"
            target="_blank"
          >
            <Terminal size={14} />
            {t('portal:settings.openCliSessions')}
            <ExternalLink size={12} />
          </a>
        </SettingsFieldRow>
      </div>

      <div className="mt-8">
        <h3 className="app-text-title-md mb-1 text-app-ink">
          {t('auth:settings.activeSessions')}
        </h3>
        <p className="app-text-caption mb-4 text-app-ink/55">
          {t('auth:settings.reviewSessions')}
        </p>
        <StatusSlot
          message={state.sessionLoadError}
          tone="error"
          announce="assertive"
          size="regular"
        />
        {state.loadingSessions && state.sessions.length === 0 ? (
          <p className="app-text-body text-app-ink/55">
            {t('common:feedback.loading')}
          </p>
        ) : state.sessionLoadError && state.sessions.length === 0 ? null : (
          <div className="grid gap-2">
            {shownSessions.map((session) => (
              <SessionCard
                key={session.id}
                onRevoke={onRevoke}
                session={session}
                locale={i18nLanguage}
                timeZone={timeZone}
                t={t}
              />
            ))}
            {hiddenCount > 0 ? (
              <p className="app-text-caption py-2 text-center text-app-ink/55">
                {t('auth:settings.sessionsMore', {
                  count: hiddenCount,
                })}
              </p>
            ) : null}
          </div>
        )}
      </div>
    </div>
  );
}
