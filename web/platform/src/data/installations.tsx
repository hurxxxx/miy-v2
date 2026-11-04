import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { request, type Installation } from "@miy/sdk";

/**
 * Installations visible to the signed-in user, as decided by the server.
 * `items` stays null until the first successful read so a failure is never
 * presented as an empty list; a later failure keeps the last list as stale.
 */
export type InstallationsState<T = Installation> = {
  status: "loading" | "ready" | "error";
  items: T[] | null;
  error?: string;
  updatedAt?: number;
};

/**
 * Administrator runtime view. The company switch blocks use, not runtime control, so a
 * disabled app stays here; `canOpen` is the server's current decision about opening it.
 */
export type ManagedInstallation = Installation & { appDisabled: boolean; canOpen: boolean };

type InstallationsContextValue = InstallationsState & { refresh: () => Promise<void> };

const InstallationsContext = createContext<InstallationsContextValue | null>(null);

// Latest request wins: an older response never replaces a newer one.
function useServerList<T>(path: string) {
  const [state, setState] = useState<InstallationsState<T>>({ status: "loading", items: null });
  const sequence = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const refresh = useCallback(async () => {
    const id = ++sequence.current;
    try {
      const items = await request<T[]>(path);
      if (!mounted.current || id !== sequence.current) return;
      if (!Array.isArray(items)) throw new Error("invalid_installations");
      setState({ status: "ready", items, updatedAt: Date.now() });
    } catch (error) {
      if (!mounted.current || id !== sequence.current) return;
      setState((previous) => ({
        ...previous,
        status: "error",
        error: error instanceof Error ? error.message : "network_error",
      }));
    }
  }, [path]);
  return useMemo(() => ({ ...state, refresh }), [state, refresh]);
}

// Keep polls sequential: a slow response must be allowed to settle before another
// poll supersedes it. Explicit refreshes still use latest-wins.
function usePolling(refresh: () => Promise<void>, intervalMs: number) {
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      await refresh();
      if (active) timer = setTimeout(poll, intervalMs);
    }
    timer = setTimeout(poll, intervalMs);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [refresh, intervalMs]);
}

export function InstallationsProvider({ children }: { children: ReactNode }) {
  const value = useServerList<Installation>("/api/v1/installations");
  return <InstallationsContext.Provider value={value}>{children}</InstallationsContext.Provider>;
}

export function useInstallations() {
  const value = useContext(InstallationsContext);
  if (!value) throw new Error("useInstallations outside InstallationsProvider");
  return value;
}

/** Keep the shared list fresh while a status screen is mounted. */
export function useInstallationPolling(intervalMs = 3000) {
  const { refresh } = useInstallations();
  usePolling(refresh, intervalMs);
}

/** Administrator runtime list, loaded on mount and kept fresh while mounted. */
export function useManagedInstallations(intervalMs = 3000) {
  const value = useServerList<ManagedInstallation>("/api/v1/runtime/installations");
  const { refresh } = value;
  useEffect(() => {
    void refresh();
  }, [refresh]);
  usePolling(refresh, intervalMs);
  return value;
}

export function installationsIn(items: Installation[] | null, environment: string) {
  return (items ?? []).filter((app) => app.environment === environment);
}
