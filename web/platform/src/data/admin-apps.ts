import { useCallback, useEffect, useRef, useState } from "react";
import { request } from "@miy/sdk";

type ReadState<T> = { path: string; data?: T; error?: Error; updatedAt?: number };

/** Read only while the page or tab is mounted. Explicit refreshes supersede older reads. */
export function useAdminAppRead<T>(path: string, intervalMs?: number) {
  const [state, setState] = useState<ReadState<T>>({ path });
  const pending = useRef<AbortController | undefined>(undefined);
  const mounted = useRef(false);
  const refresh = useCallback(async () => {
    if (!mounted.current) return;
    pending.current?.abort();
    const read = new AbortController();
    pending.current = read;
    try {
      const data = await request<T>(path, undefined, { signal: read.signal });
      if (!read.signal.aborted) setState({ path, data, updatedAt: Date.now() });
    } catch (error) {
      if (!read.signal.aborted) {
        const reason = error instanceof Error ? error : new Error(String(error));
        setState((current) => ({ ...(current.path === path ? current : { path }), error: reason }));
      }
    }
  }, [path]);

  useEffect(() => {
    mounted.current = true;
    let timer: ReturnType<typeof setTimeout>;
    let active = true;
    async function load() {
      await refresh();
      if (active && intervalMs !== undefined) timer = setTimeout(load, intervalMs);
    }
    void load();
    return () => {
      active = false;
      mounted.current = false;
      clearTimeout(timer);
      pending.current?.abort();
    };
  }, [refresh, intervalMs]);

  return { ...(state.path === path ? state : { path }), refresh };
}
