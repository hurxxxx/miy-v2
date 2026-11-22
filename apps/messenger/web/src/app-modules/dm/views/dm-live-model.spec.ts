import { describe, expect, it } from 'vitest';

import {
  applyDmTypingHint,
  liveHintConversationId,
  liveTypingHint,
  pruneDmTyping,
} from './dm-live-model';

describe('miy live hints (M4)', () => {
  it('reads conversation and typing hints and rejects malformed ones', () => {
    const changed = { type: 'conversation.changed', data: { type: 'conversation.changed', conversationId: 'c1' } };
    expect(liveHintConversationId(changed)).toBe('c1');
    expect(liveHintConversationId({ type: 'x', data: { conversationId: 'c/1' } })).toBeNull();
    expect(liveHintConversationId({ type: 'x', data: [] })).toBeNull();
    expect(
      liveTypingHint({ type: 'typing', data: { conversationId: 'c1', userId: 'u1' } }),
    ).toEqual({ conversationId: 'c1', userId: 'u1', active: true });
    expect(
      liveTypingHint({ type: 'typing', data: { conversationId: 'c1', userId: 'u1', active: false } }),
    ).toEqual({ conversationId: 'c1', userId: 'u1', active: false });
    expect(liveTypingHint({ type: 'typing', data: { conversationId: 'c1' } })).toBeNull();
  });

  it('adds, ends and expires typing people', () => {
    const one = applyDmTypingHint({}, { conversationId: 'c1', userId: 'u1', active: true }, 100);
    expect(one).toEqual({ c1: { u1: 100 } });
    const two = applyDmTypingHint(one, { conversationId: 'c1', userId: 'u2', active: true }, 200);
    expect(two).toEqual({ c1: { u1: 100, u2: 200 } });
    expect(applyDmTypingHint(two, { conversationId: 'c1', userId: 'u1', active: false }, 0)).toEqual({
      c1: { u2: 200 },
    });
    const unchanged = applyDmTypingHint(one, { conversationId: 'c2', userId: 'u9', active: false }, 0);
    expect(unchanged).toBe(one);
    expect(pruneDmTyping(two, 150)).toEqual({ c1: { u2: 200 } });
    expect(pruneDmTyping(two, 250)).toEqual({});
    expect(pruneDmTyping(two, 50)).toBe(two);
  });
});
