import type { RealtimeEvent } from '../../../platform/realtime/realtime-provider';
import { isDmRouteId } from '../../../contracts/dm';

/**
 * miy (M4): the messenger's live hints. The app server sends
 * `{"type":"conversation.changed","conversationId"}` after committed writes
 * and `{"type":"typing","conversationId","userId","active"}` while someone
 * types; neither carries conversation content.
 */
export interface DmTypingHint {
  conversationId: string;
  userId: string;
  active: boolean;
}

/** Typing people per conversation: user ID → expiry time (ms). */
export type DmTypingState = Record<string, Record<string, number>>;

function record(event: RealtimeEvent): Record<string, unknown> | null {
  const data = event.data;
  return data && typeof data === 'object' && !Array.isArray(data)
    ? (data as Record<string, unknown>)
    : null;
}

export function liveHintConversationId(event: RealtimeEvent): string | null {
  const id = record(event)?.conversationId;
  return typeof id === 'string' && isDmRouteId(id) ? id : null;
}

export function liveTypingHint(event: RealtimeEvent): DmTypingHint | null {
  const data = record(event);
  const conversationId = liveHintConversationId(event);
  const userId = data?.userId;
  if (!data || !conversationId || typeof userId !== 'string' || !userId) return null;
  return { conversationId, userId, active: data.active !== false };
}

export function applyDmTypingHint(
  state: DmTypingState,
  hint: DmTypingHint,
  expiresAt: number,
): DmTypingState {
  const thread = { ...(state[hint.conversationId] ?? {}) };
  if (hint.active) {
    thread[hint.userId] = expiresAt;
  } else if (hint.userId in thread) {
    delete thread[hint.userId];
  } else {
    return state;
  }
  const next = { ...state };
  if (Object.keys(thread).length) next[hint.conversationId] = thread;
  else delete next[hint.conversationId];
  return next;
}

export function pruneDmTyping(state: DmTypingState, now: number): DmTypingState {
  let changed = false;
  const next: DmTypingState = {};
  for (const [conversationId, thread] of Object.entries(state)) {
    const kept = Object.fromEntries(
      Object.entries(thread).filter(([, expiresAt]) => expiresAt > now),
    );
    if (Object.keys(kept).length !== Object.keys(thread).length) changed = true;
    if (Object.keys(kept).length) next[conversationId] = kept;
  }
  return changed ? next : state;
}
