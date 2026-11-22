import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import { DEFAULT_LOCALE, normalizeLocale, type AppLocale } from './locales';
import { resources } from './resources';

// Same options as AI-DO apps/web/src/platform/i18n/i18n.ts. The initial
// language is the default; AppRoot syncs it to runtime.settings.locale.
void i18n.use(initReactI18next).init({
  defaultNS: 'common',
  fallbackLng: DEFAULT_LOCALE,
  interpolation: {
    escapeValue: false,
  },
  lng: DEFAULT_LOCALE,
  ns: Object.keys(resources[DEFAULT_LOCALE]),
  react: {
    useSuspense: false,
  },
  resources,
  supportedLngs: Object.keys(resources),
});

/** Follows the portal language; unsupported values fall back to ko-KR. */
export function syncLocale(locale: string | null | undefined): AppLocale {
  const normalized = normalizeLocale(locale);
  if (i18n.language !== normalized) {
    void i18n.changeLanguage(normalized);
  }
  return normalized;
}

export { i18n };
