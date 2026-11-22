import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';

/**
 * AI-DO pushed DM changes over its realtime WebSocket (`useRealtimeEvent`)
 * and bumped `reconnectSeq` after a reconnect so screens refetched. The miy
 * messenger refreshes by periodic fetch (COMMON-FEATURES-PLAN M3; instant
 * updates and typing indicators come with M4): `reconnectSeq` advances every
 * POLL_INTERVAL_MS while the frame is visible and at once when it becomes
 * visible again, and the ported screens refetch exactly as after AI-DO's
 * reconnects. No events are delivered, so `useRealtimeEvent` does nothing.
 */
export type RealtimeStatus = 'live' | 'offline';

export interface RealtimeEvent {
  type: string;
  data?: unknown;
}

export const POLL_INTERVAL_MS = 5000;

interface RealtimeState {
  status: RealtimeStatus;
  reconnectSeq: number;
}

const RealtimeContext = createContext<RealtimeState>({
  status: 'offline',
  reconnectSeq: 0,
});

export function PollingProvider({
  children,
  intervalMs = POLL_INTERVAL_MS,
}: {
  children: ReactNode;
  intervalMs?: number;
}) {
  const [reconnectSeq, setReconnectSeq] = useState(0);
  useEffect(() => {
    let timer: number | null = null;
    const visible = () => document.visibilityState === 'visible';
    const tick = () => setReconnectSeq((value) => value + 1);
    const schedule = () => {
      if (timer !== null) window.clearInterval(timer);
      timer = visible() ? window.setInterval(tick, intervalMs) : null;
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
  }, [intervalMs]);
  return (
    <RealtimeContext.Provider value={{ status: 'live', reconnectSeq }}>
      {children}
    </RealtimeContext.Provider>
  );
}

export function useRealtime(): RealtimeState {
  return useContext(RealtimeContext);
}

export function useRealtimeEvent(
  _type: string,
  _handler: (event: RealtimeEvent) => void,
): void {
  // Realtime events arrive with M4; polling refetches instead.
}
