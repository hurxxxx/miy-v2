import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  DATE_FORMATS,
  DEFAULT_SETTINGS,
  LOCALES,
  THEME_PREFERENCES,
  timeZoneName,
  type FrameSettings,
} from "@miy/frame-protocol";

/**
 * Display settings of this browser: theme, language, time zone and date format,
 * modelled as in AI-DO. They are applied to the portal document and passed to
 * app frames. Saved per-person preferences arrive with the personal settings
 * work; until then the values live in this origin's local storage.
 */
const KEY = "miy:display-settings";
const DARK = "(prefers-color-scheme: dark)";

function pick<T extends string>(values: readonly T[], value: unknown, fallback: T): T {
  return typeof value === "string" && (values as readonly string[]).includes(value) ? (value as T) : fallback;
}
function zone(value: unknown): string {
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
export function readStoredSettings(): FrameSettings {
  let stored: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(KEY) ?? "{}");
    if (typeof parsed === "object" && parsed !== null) stored = parsed as Record<string, unknown>;
  } catch {
    /* unreadable storage: fall back to defaults */
  }
  return {
    theme: pick(THEME_PREFERENCES, stored.theme, DEFAULT_SETTINGS.theme),
    locale: pick(LOCALES, stored.locale, DEFAULT_SETTINGS.locale),
    timeZone: zone(stored.timeZone),
    dateFormat: pick(DATE_FORMATS, stored.dateFormat, DEFAULT_SETTINGS.dateFormat),
  };
}
export function resolveTheme(theme: FrameSettings["theme"], systemDark: boolean): "light" | "dark" {
  return theme === "system" ? (systemDark ? "dark" : "light") : theme;
}
/** Both markers are set: `.dark` is what AI-DO styles key on, the attribute is what common UI 1.1.0 keys on. */
export function applyDocumentSettings(settings: FrameSettings, systemDark: boolean) {
  const root = document.documentElement;
  const theme = resolveTheme(settings.theme, systemDark);
  root.classList.toggle("dark", theme === "dark");
  root.dataset.miyTheme = theme;
  root.lang = settings.locale;
}

interface DisplaySettings {
  settings: FrameSettings;
  resolvedTheme: "light" | "dark";
  update(change: Partial<FrameSettings>): void;
}
const Context = createContext<DisplaySettings | null>(null);

export function DisplaySettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState(readStoredSettings);
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(DARK).matches);
  useEffect(() => {
    const query = window.matchMedia(DARK);
    const changed = () => setSystemDark(query.matches);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  }, []);
  useEffect(() => applyDocumentSettings(settings, systemDark), [settings, systemDark]);
  const update = useCallback((change: Partial<FrameSettings>) => {
    setSettings((current) => {
      const next = { ...current, ...change };
      try {
        window.localStorage.setItem(KEY, JSON.stringify(next));
      } catch {
        /* the choice still applies to this page */
      }
      return next;
    });
  }, []);
  const value = useMemo(
    () => ({ settings, resolvedTheme: resolveTheme(settings.theme, systemDark), update }),
    [settings, systemDark, update],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export function useDisplaySettings(): DisplaySettings {
  const value = useContext(Context);
  if (!value) throw new Error("DisplaySettingsProvider is missing");
  return value;
}
