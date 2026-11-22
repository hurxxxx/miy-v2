// Ported from AI-DO apps/web/src/platform/auth/ProfileSettingsSection.tsx
// (0002af062). miy has one display name, and the sign-in ID and e-mail live in
// the identity provider; the role row shows the platform roles miy knows.
import type { Dispatch, FormEvent } from 'react';
import { Button, FormMessage } from '@miy/ui';

import type { User } from '../../data/session';
import { SettingsFieldRow } from './SettingsFieldRow';
import { SettingsSectionHeader } from './SettingsSectionHeader';
import {
  fieldClassName,
  getUserInitials,
  type ProfilePageAction,
  type ProfilePageState,
  type SettingsTranslator,
} from './settings-page-model';

export function ProfileSettingsSection({
  dispatch,
  onSubmit,
  roles,
  state,
  t,
  user,
}: {
  dispatch: Dispatch<ProfilePageAction>;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  roles: string[];
  state: ProfilePageState;
  t: SettingsTranslator;
  user: User;
}) {
  return (
    <div>
      <SettingsSectionHeader
        title={t('auth:settings.profile')}
        description={t('auth:settings.manageProfile')}
      />

      <form onSubmit={onSubmit}>
        <div className="border-t border-app-border">
          <SettingsFieldRow label={t('auth:settings.avatar')}>
            <div className="flex items-center gap-4">
              <div className="app-text-title-md flex size-12 items-center justify-center rounded-full bg-orange-500 font-bold text-white">
                {getUserInitials(user.name)}
              </div>
              <div className="app-text-body">
                <p className="font-medium text-app-ink">{user.name}</p>
                {user.preferences.jobTitle ? (
                  <p className="app-text-caption text-app-ink/55">
                    {user.preferences.jobTitle}
                  </p>
                ) : null}
              </div>
            </div>
          </SettingsFieldRow>

          <SettingsFieldRow label={t('auth:settings.displayName')}>
            <input
              aria-label={t('auth:settings.displayName')}
              className={fieldClassName}
              maxLength={120}
              onChange={(event) =>
                dispatch({
                  type: 'patch',
                  patch: { displayName: event.target.value },
                })
              }
              required
              value={state.displayName}
            />
          </SettingsFieldRow>

          <SettingsFieldRow label={t('auth:settings.jobTitle')}>
            <input
              aria-label={t('auth:settings.jobTitle')}
              className={fieldClassName}
              maxLength={120}
              onChange={(event) =>
                dispatch({
                  type: 'patch',
                  patch: { jobTitle: event.target.value },
                })
              }
              placeholder={t('auth:settings.jobTitlePlaceholder')}
              value={state.jobTitle}
            />
          </SettingsFieldRow>

          <SettingsFieldRow label={t('auth:settings.role')}>
            <span className="app-text-body text-app-ink">
              {roles.join(', ')}
            </span>
          </SettingsFieldRow>
        </div>

        <div className="flex justify-end gap-3 pt-6">
          <FormMessage
            id="profile-settings-form-error"
            message={state.formError}
            tone="error"
            variant="form"
          />
          <Button
            variant="ghost"
            type="button"
            onClick={() => dispatch({ type: 'resetProfile', user })}
          >
            {t('common:actions.reset')}
          </Button>
          <Button variant="primary" type="submit" disabled={state.submitting}>
            {state.submitting
              ? t('common:actions.saving')
              : t('common:actions.saveChanges')}
          </Button>
        </div>
      </form>
    </div>
  );
}
