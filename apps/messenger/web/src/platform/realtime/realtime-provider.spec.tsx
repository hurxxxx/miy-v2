import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LiveHandlers } from '@miy/sdk';

import {
  LiveProvider,
  parseLiveHint,
  useRealtime,
  useRealtimeEvent,
  type RealtimeEvent,
} from './realtime-provider';

function fakeLive() {
  const sent: string[] = [];
  let handlers: LiveHandlers | null = null;
  const close = vi.fn();
  const open = vi.fn((next: LiveHandlers) => {
    handlers = next;
    return { send: (data: string) => sent.push(data), close };
  });
  return {
    open,
    close,
    sent,
    handlers: () => handlers as LiveHandlers,
  };
}

let state: ReturnType<typeof useRealtime> | null = null;
const events: RealtimeEvent[] = [];

function Probe() {
  state = useRealtime();
  useRealtimeEvent('conversation.changed', (event) => events.push(event));
  return null;
}

describe('miy live connection provider (M4)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    events.length = 0;
    state = null;
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('parses hints and drops heartbeats', () => {
    expect(parseLiveHint('{"type":"heartbeat"}')).toBeNull();
    expect(parseLiveHint('nope')).toBeNull();
    expect(parseLiveHint('{"type":"typing","conversationId":"c1"}')).toEqual({
      type: 'typing',
      data: { type: 'typing', conversationId: 'c1' },
    });
  });

  it('opens one connection, dispatches hints and refetches after each open', () => {
    const live = fakeLive();
    const view = render(
      <LiveProvider intervalMs={5000} live={live.open} safetyIntervalMs={60_000}>
        <Probe />
      </LiveProvider>,
    );
    expect(live.open).toHaveBeenCalledTimes(1);
    expect(state?.status).toBe('offline');
    expect(state?.send({ type: 'open' })).toBe(false);
    act(() => live.handlers().onOpen?.());
    expect(state?.status).toBe('live');
    expect(state?.reconnectSeq).toBe(1);
    expect(state?.send({ type: 'open', conversationId: 'c1' })).toBe(true);
    expect(live.sent).toEqual(['{"type":"open","conversationId":"c1"}']);
    act(() => live.handlers().onMessage('{"type":"conversation.changed","conversationId":"c1"}'));
    expect(events).toHaveLength(1);
    // While live only the slow safety refresh runs.
    act(() => vi.advanceTimersByTime(10_000));
    expect(state?.reconnectSeq).toBe(1);
    act(() => vi.advanceTimersByTime(60_000));
    expect(state?.reconnectSeq).toBe(2);
    view.rerender(
      <LiveProvider intervalMs={5000} live={live.open} safetyIntervalMs={60_000}>
        <Probe />
      </LiveProvider>,
    );
    expect(live.open).toHaveBeenCalledTimes(1);
    view.unmount();
    expect(live.close).toHaveBeenCalled();
  });

  it('polls while the connection retries and stops after a final close', () => {
    const live = fakeLive();
    render(
      <LiveProvider intervalMs={5000} live={live.open} safetyIntervalMs={60_000}>
        <Probe />
      </LiveProvider>,
    );
    act(() => live.handlers().onOpen?.());
    act(() => live.handlers().onClose?.({ code: 4502, retrying: true }));
    expect(state?.status).toBe('offline');
    const before = state?.reconnectSeq ?? 0;
    act(() => vi.advanceTimersByTime(10_000));
    expect(state?.reconnectSeq).toBe(before + 2);
    act(() => live.handlers().onClose?.({ code: 4401, retrying: false }));
    const ended = state?.reconnectSeq ?? 0;
    act(() => vi.advanceTimersByTime(60_000));
    expect(state?.reconnectSeq).toBe(ended);
  });

  it('polls when the host has no live connection', () => {
    render(
      <LiveProvider intervalMs={5000}>
        <Probe />
      </LiveProvider>,
    );
    act(() => vi.advanceTimersByTime(5000));
    expect(state?.reconnectSeq).toBe(1);
    expect(state?.send({ type: 'typing' })).toBe(false);
  });
});
