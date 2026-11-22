import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import type { FrameSettings } from "@miy/frame-protocol";
import { syncLocale } from "../platform/i18n";
import { normalizeSettings, readStoredSettings, setCurrentSettings, writeStoredSettings } from "./display-settings-store";

export { currentSettings, readStoredSettings } from "./display-settings-store";

/**
 * Display settings applied to the portal document and passed to app frames.
 * `update` changes what this page shows; saving a person's choice is the
 * settings page's job (PATCH /api/v1/me/preferences), and the saved values
 * arrive here again after sign-in (data/session.tsx).
 */
const DARK = "(prefers-color-scheme: dark)";

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
  useEffect(() => {
    setCurrentSettings(settings);
    syncLocale(settings.locale);
    applyDocumentSettings(settings, systemDark);
  }, [settings, systemDark]);
  const update = useCallback((change: Partial<FrameSettings>) => {
    setSettings((current) => {
      const next = normalizeSettings({ ...current, ...change });
      if (
        next.theme === current.theme &&
        next.locale === current.locale &&
        next.timeZone === current.timeZone &&
        next.dateFormat === current.dateFormat
      )
        return current;
      writeStoredSettings(next);
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
