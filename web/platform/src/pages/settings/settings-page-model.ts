// Ported from AI-DO apps/web/src/platform/auth/settings-page-model.ts
// (0002af062). miy keeps one display name (AI-DO had display and full names),
// changes passwords on the identity provider's own form, and has no default
// workspace: there is no platform-wide workspace.
import type { DateFormat, Locale, ThemePreference } from '@miy/frame-protocol';

import { normalizeLocale } from '../../platform/i18n';
import {
  normalizeDateFormatPreference,
  normalizeTimeZone,
} from '../../platform/time/time-utils';
import type { User } from '../../data/session';

export type SettingsSection = 'profile' | 'appearance' | 'security' | 'notifications';
export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
  'profile',
  'appearance',
  'security',
  'notifications',
];

export type SettingsTranslator = (
  key: string,
  options?: Record<string, unknown>,
) => string;

/** The body of PATCH /api/v1/me/preferences. */
export interface UpdatePreferencesPayload {
  name?: string;
  jobTitle?: string;
  theme?: ThemePreference;
  locale?: Locale;
  timeZone?: string;
  dateFormat?: DateFormat;
}

export interface ProfilePageState {
  activeSection: SettingsSection;
  displayName: string;
  jobTitle: string;
  themePreference: ThemePreference;
  timeZone: string;
  locale: Locale;
  dateFormat: DateFormat;
  submitting: boolean;
  formError: string | null;
}

export type ProfilePreferenceChange =
  | { type: 'theme'; value: ThemePreference }
  | { type: 'locale'; value: string }
  | { type: 'timeZone'; value: string }
  | { type: 'dateFormat'; value: string };

export type ProfilePreferenceSavePlan = {
  optimisticPatch: Partial<ProfilePageState>;
  payload: UpdatePreferencesPayload;
  rollbackPatch: Partial<ProfilePageState>;
};

export type ProfileDetailsSavePlan = {
  startPatch: Partial<ProfilePageState>;
  payload: UpdatePreferencesPayload;
  failurePatch: (error: unknown, fallback: string) => Partial<ProfilePageState>;
  completePatch: Partial<ProfilePageState>;
};

export type ProfilePageAction =
  | { type: 'patch'; patch: Partial<ProfilePageState> }
  | { type: 'resetProfile'; user: User };

export const fieldClassName =
  'app-text-body w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink transition-colors focus:border-app-accent focus:outline-none';

export function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  return fallback;
}

export function createInitialProfilePageState(
  user: User,
  initialTab: SettingsSection,
): ProfilePageState {
  return {
    activeSection: initialTab,
    displayName: user.name,
    jobTitle: user.preferences.jobTitle ?? '',
    themePreference: user.preferences.theme,
    timeZone: normalizeTimeZone(user.preferences.timeZone),
    locale: normalizeLocale(user.preferences.locale),
    dateFormat: normalizeDateFormatPreference(user.preferences.dateFormat),
    submitting: false,
    formError: null,
  };
}

export function profilePageReducer(
  state: ProfilePageState,
  action: ProfilePageAction,
): ProfilePageState {
  switch (action.type) {
    case 'patch':
      return { ...state, ...action.patch };
    case 'resetProfile':
      return {
        ...state,
        displayName: action.user.name,
        jobTitle: action.user.preferences.jobTitle ?? '',
        formError: null,
      };
  }
}

export function getUserInitials(name: string) {
  return (
    name
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase() ?? '')
      .join('') || 'ID'
  );
}

export function prepareProfilePreferenceSave(
  state: ProfilePageState,
  change: ProfilePreferenceChange,
): ProfilePreferenceSavePlan | null {
  if (change.type === 'theme') {
    if (change.value === state.themePreference) return null;
    return {
      optimisticPatch: {
        themePreference: change.value,
      },
      payload: { theme: change.value },
      rollbackPatch: { themePreference: state.themePreference },
    };
  }

  if (change.type === 'locale') {
    const nextLocale = normalizeLocale(change.value);
    if (nextLocale === state.locale) return null;
    return {
      optimisticPatch: { locale: nextLocale },
      payload: { locale: nextLocale },
      rollbackPatch: { locale: state.locale },
    };
  }

  if (change.type === 'timeZone') {
    const nextTimeZone = normalizeTimeZone(change.value);
    if (nextTimeZone === state.timeZone) return null;
    return {
      optimisticPatch: {
        timeZone: nextTimeZone,
      },
      payload: { timeZone: nextTimeZone },
      rollbackPatch: { timeZone: state.timeZone },
    };
  }

  const nextDateFormat = normalizeDateFormatPreference(change.value);
  if (nextDateFormat === state.dateFormat) return null;
  return {
    optimisticPatch: {
      dateFormat: nextDateFormat,
    },
    payload: { dateFormat: nextDateFormat },
    rollbackPatch: { dateFormat: state.dateFormat },
  };
}

export function prepareProfileDetailsSave(
  state: ProfilePageState,
): ProfileDetailsSavePlan {
  return {
    startPatch: { formError: null, submitting: true },
    payload: {
      name: state.displayName.trim(),
      jobTitle: state.jobTitle.trim(),
    },
    failurePatch: (caughtError, fallback) => ({
      formError: getErrorMessage(caughtError, fallback),
    }),
    completePatch: { submitting: false },
  };
}

/** One entry of GET /api/v1/me/sessions. */
export interface BrowserSession {
  id: string;
  surface: 'portal' | 'dev_portal' | 'console';
  createdAt: string;
  expiresAt: string;
  lastSeenAt: string | null;
  userAgent: string | null;
  current: boolean;
}

export interface SecurityState {
  sessions: BrowserSession[];
  loadingSessions: boolean;
  sessionLoadError: string | null;
}

export interface VisibleAuthSessions {
  hiddenCount: number;
  shownSessions: BrowserSession[];
}

export function selectVisibleAuthSessions(
  sessions: BrowserSession[],
  visibleOtherSessionLimit = 3,
): VisibleAuthSessions {
  const currentSessions = sessions.filter((session) => session.current);
  const otherSessions = sessions
    .filter((session) => !session.current)
    .slice(0, visibleOtherSessionLimit);
  const shownSessions = [...currentSessions, ...otherSessions];

  return {
    hiddenCount: sessions.length - shownSessions.length,
    shownSessions,
  };
}
