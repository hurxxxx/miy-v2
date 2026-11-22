// Ported from AI-DO apps/web/src/platform/auth/useProfilePageController.ts
// (0002af062). Saving goes to PATCH /api/v1/me/preferences; display changes
// apply at once through the portal's display settings and roll back when the
// save fails, as AI-DO does with its locale.
import { useEffect, useReducer, useRef, useState } from 'react';
import type { FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useFeedback } from '@miy/ui';
import { ApiError, request } from '@miy/sdk';
import type { ThemePreference } from '@miy/frame-protocol';

import { useDisplaySettings } from '../../data/settings';
import {
  displayOf,
  useSession,
  type Preferences,
  type User,
} from '../../data/session';
import {
  createInitialProfilePageState,
  prepareProfileDetailsSave,
  prepareProfilePreferenceSave,
  profilePageReducer,
  type BrowserSession,
  type ProfilePreferenceChange,
  type SecurityState,
  type SettingsSection,
  type UpdatePreferencesPayload,
} from './settings-page-model';

export function useSettingsPageController({
  initialTab,
  user,
}: {
  initialTab: SettingsSection;
  user: User;
}) {
  const { t, i18n } = useTranslation(['auth', 'common', 'portal']);
  const feedback = useFeedback();
  const { isCurrentSession, updateUser } = useSession();
  const display = useDisplaySettings();
  const [state, dispatch] = useReducer(
    profilePageReducer,
    { user, initialTab },
    ({ user, initialTab }) => createInitialProfilePageState(user, initialTab),
  );
  const [security, setSecurity] = useState<SecurityState>({
    sessions: [],
    loadingSessions: false,
    sessionLoadError: null,
  });
  const pendingPreferences = useRef(new Set<ProfilePreferenceChange['type']>());
  const [savingPreferences, setSavingPreferences] = useState<ReadonlySet<ProfilePreferenceChange['type']>>(
    pendingPreferences.current,
  );

  /** Server errors arrive as codes; known ones have a translation. */
  const errorText = (error: unknown, fallback: string) =>
    error instanceof ApiError && i18n.exists(`portal:errors.${error.code}`)
      ? t(`portal:errors.${error.code}`)
      : fallback;

  const save = async (payload: UpdatePreferencesPayload) => {
    const saved = await request<{ name: string; preferences: Preferences }>(
      '/api/v1/me/preferences',
      user.csrf,
      {
        method: 'PATCH',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload),
      },
    );
    updateUser(user.csrf, saved, Object.keys(payload) as (keyof UpdatePreferencesPayload)[]);
  };

  const loadSessions = async (signal?: AbortSignal) => {
    setSecurity((current) => ({
      ...current,
      loadingSessions: true,
      sessionLoadError: null,
    }));
    try {
      const page = await request<{ items: BrowserSession[] }>(
        '/api/v1/me/sessions',
        undefined,
        { signal },
      );
      if (!signal?.aborted)
        setSecurity({
          sessions: page.items,
          loadingSessions: false,
          sessionLoadError: null,
        });
    } catch (caughtError) {
      if (!signal?.aborted)
        setSecurity((current) => ({
          ...current,
          loadingSessions: false,
          sessionLoadError: errorText(
            caughtError,
            t('auth:settings.sessionsLoadFailed'),
          ),
        }));
    }
  };

  useEffect(() => {
    if (state.activeSection !== 'security') return;
    const controller = new AbortController();
    void loadSessions(controller.signal);
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.activeSection]);

  const handleSectionChange = (section: SettingsSection) => {
    dispatch({
      type: 'patch',
      patch: { activeSection: section, formError: null },
    });
  };

  const handleProfileSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const plan = prepareProfileDetailsSave(state);
    dispatch({ type: 'patch', patch: plan.startPatch });
    try {
      await save(plan.payload);
      feedback.success(t('auth:settings.saved'));
    } catch (caughtError) {
      dispatch({
        type: 'patch',
        patch: {
          formError: errorText(caughtError, t('auth:settings.saveFailed')),
        },
      });
    } finally {
      dispatch({ type: 'patch', patch: plan.completePatch });
    }
  };

  const handleRevoke = async (sessionId: string) => {
    try {
      await request<void>(`/api/v1/me/sessions/${sessionId}`, user.csrf, {
        method: 'DELETE',
      });
      await loadSessions();
      feedback.success(t('auth:settings.sessionRevoked'));
    } catch (caughtError) {
      feedback.error(
        errorText(caughtError, t('auth:settings.sessionRevokeFailed')),
      );
    }
  };

  const saveProfilePreference = async (change: ProfilePreferenceChange) => {
    if (pendingPreferences.current.has(change.type)) return;
    const plan = prepareProfilePreferenceSave(state, change);
    if (!plan) return;
    pendingPreferences.current.add(change.type);
    setSavingPreferences(new Set(pendingPreferences.current));
    const previous = displayOf(user.preferences);
    dispatch({ type: 'patch', patch: plan.optimisticPatch });
    display.update(plan.payload);
    try {
      await save(plan.payload);
    } catch (caughtError) {
      if (isCurrentSession(user.csrf)) {
        display.update({ [change.type]: previous[change.type] });
        dispatch({ type: 'patch', patch: plan.rollbackPatch });
        feedback.error(errorText(caughtError, t('auth:settings.saveFailed')));
      }
    } finally {
      pendingPreferences.current.delete(change.type);
      setSavingPreferences(new Set(pendingPreferences.current));
    }
  };

  return {
    dispatch,
    handleProfileSubmit,
    handleRevoke,
    handleSectionChange,
    i18nLanguage: i18n.language,
    saveDateFormatPreference: (value: string) =>
      saveProfilePreference({ type: 'dateFormat', value }),
    saveLocalePreference: (value: string) =>
      saveProfilePreference({ type: 'locale', value }),
    saveThemePreference: (value: ThemePreference) =>
      saveProfilePreference({ type: 'theme', value }),
    saveTimeZonePreference: (value: string) =>
      saveProfilePreference({ type: 'timeZone', value }),
    savingPreferences,
    security,
    state,
    t,
  };
}
