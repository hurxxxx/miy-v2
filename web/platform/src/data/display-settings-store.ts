import {
  DATE_FORMATS,
  DEFAULT_SETTINGS,
  LOCALES,
  THEME_PREFERENCES,
  timeZoneName,
  type FrameSettings,
} from "@miy/frame-protocol";

/**
 * This origin's copy of the display settings: theme, language, time zone and
 * date format, modelled as in AI-DO. The signed-in person's saved preferences
 * replace it after sign-in, so the next page load paints with them at once,
 * as AI-DO keeps its locale, time zone and date format keys in local storage.
 */
const KEY = "miy:display-settings";

function pick<T extends string>(values: readonly T[], value: unknown, fallback: T): T {
  return typeof value === "string" && (values as readonly string[]).includes(value) ? (value as T) : fallback;
}
/** A zone name this browser can format with, else the browser's own zone, else Seoul. */
export function knownTimeZone(value: unknown): string {
  for (const candidate of [value, Intl.DateTimeFormat().resolvedOptions().timeZone]) {
    if (!timeZoneName(candidate)) continue;
    try {
      new Intl.DateTimeFormat("en-US", { timeZone: candidate });
      return candidate;
    } catch {
      /* not a zone this browser knows */
    }
  }
  return DEFAULT_SETTINGS.timeZone;
}
export function normalizeSettings(value: Record<string, unknown>): FrameSettings {
  return {
    theme: pick(THEME_PREFERENCES, value.theme, DEFAULT_SETTINGS.theme),
    locale: pick(LOCALES, value.locale, DEFAULT_SETTINGS.locale),
    timeZone: knownTimeZone(value.timeZone),
    dateFormat: pick(DATE_FORMATS, value.dateFormat, DEFAULT_SETTINGS.dateFormat),
  };
}
export function readStoredSettings(): FrameSettings {
  let stored: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(KEY) ?? "{}");
    if (typeof parsed === "object" && parsed !== null) stored = parsed as Record<string, unknown>;
  } catch {
    /* unreadable storage: fall back to defaults */
  }
  return normalizeSettings(stored);
}
export function writeStoredSettings(settings: FrameSettings) {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    /* the choice still applies to this page */
  }
}

let current = readStoredSettings();
/** The settings in effect on this page, for formatters outside React. */
export function currentSettings(): FrameSettings {
  return current;
}
export function setCurrentSettings(settings: FrameSettings) {
  current = settings;
}
