import { describe, expect, it } from 'vitest';

import {
  activeDmMentionQuery,
  dmMentionCandidates,
  dmMentionDraftsFromMessage,
  dmMentionParts,
  insertDmMention,
  resolveDmMentionRanges,
} from './dm-mention-model';

describe('miy mentions', () => {
  it('finds the "@query" before the caret', () => {
    expect(activeDmMentionQuery('안녕 @김', 5)).toEqual({ start: 3, query: '김' });
    expect(activeDmMentionQuery('@', 1)).toEqual({ start: 0, query: '' });
    expect(activeDmMentionQuery('mail@example', 12)).toBeNull();
    expect(activeDmMentionQuery('@김 하나', 5)).toBeNull();
  });

  it('filters participants without the current person', () => {
    const people = [
      { id: 'me', name: '나' },
      { id: 'a', name: '김하나' },
      { id: 'b', name: 'Bee' },
    ];
    expect(dmMentionCandidates(people, 'me', '').map((p) => p.id)).toEqual(['a', 'b']);
    expect(dmMentionCandidates(people, 'me', 'bE').map((p) => p.id)).toEqual(['b']);
  });

  it('inserts a label and resolves ranges in UTF-16 units', () => {
    const typed = '😀 @김';
    const active = activeDmMentionQuery(typed, typed.length)!;
    const inserted = insertDmMention(typed, active, typed.length, '김하나');
    expect(inserted.text).toBe('😀 @김하나 ');
    expect(inserted.caret).toBe(inserted.text.length);
    const body = `${inserted.text}와 @Bee 그리고 @김하나`;
    expect(
      resolveDmMentionRanges(body, [
        { userId: 'a', label: '@김하나' },
        { userId: 'b', label: '@Bee' },
        { userId: 'a', label: '@김하나' },
        { userId: 'x', label: '@없음' },
      ]),
    ).toEqual([
      { user_id: 'a', start: 3, length: 4 },
      { user_id: 'b', start: 10, length: 4 },
      { user_id: 'a', start: 19, length: 4 },
    ]);
    expect(resolveDmMentionRanges('mail@Bee', [{ userId: 'b', label: '@Bee' }])).toEqual([]);
  });

  it('splits bodies for rendering and reads drafts back', () => {
    const body = '안녕 @Bee 님';
    const mentions = [{ user_id: 'b', start: 3, length: 4 }];
    expect(dmMentionParts(body, mentions)).toEqual([
      { kind: 'text', text: '안녕 ' },
      { kind: 'mention', text: '@Bee', userId: 'b' },
      { kind: 'text', text: ' 님' },
    ]);
    expect(dmMentionParts(body, [{ user_id: 'b', start: 2, length: 4 }])).toEqual([
      { kind: 'text', text: body },
    ]);
    expect(dmMentionDraftsFromMessage(body, mentions)).toEqual([{ userId: 'b', label: '@Bee' }]);
  });
});
