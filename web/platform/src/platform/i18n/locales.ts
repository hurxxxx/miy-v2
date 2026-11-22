// Ported from AI-DO apps/web/src/platform/i18n/locales.ts (0002af062). The
// chosen language is part of the display settings (data/settings.tsx), so the
// separate locale storage key of AI-DO is not used here.
import { DEFAULT_SETTINGS, LOCALES } from "@miy/frame-protocol";

export const DEFAULT_LOCALE = DEFAULT_SETTINGS.locale;
export const SUPPORTED_LOCALES = LOCALES;

export type AppLocale = (typeof SUPPORTED_LOCALES)[number];

export const LOCALE_OPTIONS: Array<{ value: AppLocale; label: string }> = [
  { value: "ko-KR", label: "한국어" }, // i18n-allow: a language names itself
  { value: "en-US", label: "English" },
];

export function isAppLocale(value: string | null | undefined): value is AppLocale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value ?? "");
}

export function normalizeLocale(value: string | null | undefined): AppLocale {
  return isAppLocale(value) ? value : DEFAULT_LOCALE;
}
