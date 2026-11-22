// AI-DO apps/web/src/platform/i18n/locales.ts, without the stored-locale
// session: in miy the language is the portal display setting
// (`runtime.settings.locale`), delivered on every render.
export const DEFAULT_LOCALE = 'ko-KR';
export const SUPPORTED_LOCALES = ['ko-KR', 'en-US'] as const;

export type AppLocale = (typeof SUPPORTED_LOCALES)[number];

export function isAppLocale(
  value: string | null | undefined,
): value is AppLocale {
  return SUPPORTED_LOCALES.includes(value as AppLocale);
}

export function normalizeLocale(value: string | null | undefined): AppLocale {
  return isAppLocale(value) ? value : DEFAULT_LOCALE;
}
