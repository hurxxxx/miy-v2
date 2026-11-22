// Ported from AI-DO apps/web/src/platform/i18n/i18n.ts (0002af062): the same
// i18next options. The initial language is the stored display setting.
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { readStoredSettings } from "../../data/display-settings-store";
import { DEFAULT_LOCALE, normalizeLocale, type AppLocale } from "./locales";
import { resources } from "./resources";

void i18n.use(initReactI18next).init({
  defaultNS: "common",
  fallbackLng: DEFAULT_LOCALE,
  interpolation: {
    escapeValue: false,
  },
  lng: readStoredSettings().locale,
  ns: Object.keys(resources[DEFAULT_LOCALE]),
  react: {
    useSuspense: false,
  },
  resources,
  supportedLngs: Object.keys(resources),
});

export function syncLocale(locale: string | null | undefined): AppLocale {
  const next = normalizeLocale(locale);
  if (i18n.language !== next) void i18n.changeLanguage(next);
  return next;
}

export { i18n };
