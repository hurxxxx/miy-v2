import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import type { LiveConnection, LiveHandlers } from '@miy/sdk';

/**
 * AI-DO pushed DM changes over its realtime WebSocket (`useRealtimeEvent`)
 * and bumped `reconnectSeq` after a reconnect so screens refetched.
 *
 * The miy messenger (M4) opens one platform live connection per frame
 * (`runtime.live`). The app server sends small hints only
 * (`conversation.changed`, `typing`); the screens refetch through
 * `runtime.api`, so a hint never carries conversation data.
 *
 * - While the connection is open, hints are dispatched to `useRealtimeEvent`
 *   subscribers, `reconnectSeq` advances once on every (re)open so missed
 *   changes are refetched, and a slow safety refresh (LIVE_SAFETY_INTERVAL_MS)
 *   covers hints lost to a full queue or an app restart.
 * - While it is down and the runtime retries, the M3 polling resumes:
 *   `reconnectSeq` every POLL_INTERVAL_MS while the frame is visible.
 * - After a final close (session ended, access revoked, release changed,
 *   refused) the runtime stops reconnecting and so does polling; the platform
 *   relaunches or blocks the frame.
 * - Without `runtime.live` (a host before M4) the frame polls.
 */
export type RealtimeStatus = 'live' | 'offline';

export interface RealtimeEvent {
  type: string;
  data?: unknown;
}

export const POLL_INTERVAL_MS = 5000;
export const LIVE_SAFETY_INTERVAL_MS = 60_000;

type Listener = (event: RealtimeEvent) => void;
type ConnectionState = 'connecting' | 'live' | 'retrying' | 'ended';

interface RealtimeState {
  status: RealtimeStatus;
  reconnectSeq: number;
  /** Send a client message on the open connection; false when not open. */
  send: (message: Record<string, unknown>) => boolean;
  subscribe: (type: string, listener: Listener) => () => void;
}

const RealtimeContext = createContext<RealtimeState>({
  status: 'offline',
  reconnectSeq: 0,
  send: () => false,
  subscribe: () => () => undefined,
});

export type LiveOpener = (handlers: LiveHandlers) => LiveConnection;

/** Parse one live message; heartbeats and malformed text are dropped. */
export function parseLiveHint(text: string): RealtimeEvent | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const type = (value as { type?: unknown }).type;
  if (typeof type !== 'string' || type === 'heartbeat') return null;
  return { type, data: value };
}

export function LiveProvider({
  children,
  intervalMs = POLL_INTERVAL_MS,
  live,
  safetyIntervalMs = LIVE_SAFETY_INTERVAL_MS,
}: {
  children: ReactNode;
  intervalMs?: number;
  live?: LiveOpener;
  safetyIntervalMs?: number;
}) {
  // One connection per frame: the opener is taken once (a new runtime object
  // arrives on every settings change).
  const liveRef = useRef(live);
  const [reconnectSeq, setReconnectSeq] = useState(0);
  const [state, setState] = useState<ConnectionState>(
    live ? 'connecting' : 'retrying',
  );
  const listeners = useRef(new Map<string, Set<Listener>>());
  const connection = useRef<LiveConnection | null>(null);
  const openRef = useRef(false);

  useEffect(() => {
    const opener = liveRef.current;
    if (!opener) return undefined;
    let disposed = false;
    const current = opener({
      onOpen: () => {
        if (disposed) return;
        openRef.current = true;
        setState('live');
        setReconnectSeq((value) => value + 1);
      },
      onMessage: (data) => {
        if (disposed) return;
        const event = parseLiveHint(data);
        if (!event) return;
        listeners.current.get(event.type)?.forEach((listener) => listener(event));
      },
      onClose: ({ retrying }) => {
        if (disposed) return;
        openRef.current = false;
        setState(retrying ? 'retrying' : 'ended');
      },
    });
    connection.current = current;
    return () => {
      disposed = true;
      openRef.current = false;
      connection.current = null;
      current.close();
    };
  }, []);

  useEffect(() => {
    if (state === 'ended') return undefined;
    const every = state === 'live' ? safetyIntervalMs : intervalMs;
    let timer: number | null = null;
    const visible = () => document.visibilityState === 'visible';
    const tick = () => setReconnectSeq((value) => value + 1);
    const schedule = () => {
      if (timer !== null) window.clearInterval(timer);
      timer = visible() ? window.setInterval(tick, every) : null;
    };
    const onVisibility = () => {
      if (visible()) tick();
      schedule();
    };
    schedule();
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      if (timer !== null) window.clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [intervalMs, safetyIntervalMs, state]);

  const send = useCallback((message: Record<string, unknown>) => {
    if (!openRef.current || !connection.current) return false;
    try {
      connection.current.send(JSON.stringify(message));
      return true;
    } catch {
      return false;
    }
  }, []);

  const subscribe = useCallback((type: string, listener: Listener) => {
    const set = listeners.current.get(type) ?? new Set<Listener>();
    set.add(listener);
    listeners.current.set(type, set);
    return () => {
      set.delete(listener);
    };
  }, []);

  const value = useMemo<RealtimeState>(
    () => ({
      status: state === 'live' ? 'live' : 'offline',
      reconnectSeq,
      send,
      subscribe,
    }),
    [reconnectSeq, send, state, subscribe],
  );
  return (
    <RealtimeContext.Provider value={value}>{children}</RealtimeContext.Provider>
  );
}

export function useRealtime(): RealtimeState {
  return useContext(RealtimeContext);
}

/** Subscribe to one hint type of the live connection. */
export function useRealtimeEvent(
  type: string,
  handler: (event: RealtimeEvent) => void,
): void {
  const { subscribe } = useContext(RealtimeContext);
  const handlerRef = useRef(handler);
  handlerRef.current = handler;
  useEffect(
    () => subscribe(type, (event) => handlerRef.current(event)),
    [subscribe, type],
  );
}
