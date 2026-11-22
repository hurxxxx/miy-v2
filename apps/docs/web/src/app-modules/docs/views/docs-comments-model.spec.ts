import { describe, expect, it } from 'vitest';

import type { DocsComment } from '../api/docs-api';
import {
  activeMentionQuery,
  countVisibleComments,
  insertMention,
  mentionIdsInBody,
  partitionThreads,
  splitCommentBody,
} from './docs-comments-model';
import {
  commentRequestId,
  finishCommentRequest,
  getCommentDraft,
  hasCommentDrafts,
  setCommentDraft,
} from './docs-comment-drafts';

describe('comment mentions', () => {
  it('finds the @query typed before the caret', () => {
    expect(activeMentionQuery('hi @al', 6)).toEqual({ start: 3, query: 'al' });
    expect(activeMentionQuery('@', 1)).toEqual({ start: 0, query: '' });
    expect(activeMentionQuery('mail@host', 9)).toBeNull();
    expect(activeMentionQuery('@al done', 8)).toBeNull();
    expect(activeMentionQuery('no mention', 10)).toBeNull();
  });

  it('inserts the picked name and moves the caret after it', () => {
    expect(insertMention('hi @al!', 3, 6, '홍길동')).toEqual({
      text: 'hi @홍길동 !',
      caret: 8,
    });
  });

  it('keeps only the mentions whose @name is still in the body', () => {
    const picked = [
      { user_id: 'u1', name: 'Alice' },
      { user_id: 'u2', name: 'Bob' },
      { user_id: 'u1', name: 'Alice' },
    ];
    expect(mentionIdsInBody('@Alice please check', picked)).toEqual(['u1']);
    expect(mentionIdsInBody('@Bob and @Alice', picked)).toEqual(['u1', 'u2']);
  });

  it('splits the body into text and mention runs (longest name first)', () => {
    expect(
      splitCommentBody('cc @Al Lee and @Al', [
        { user_id: 'u1', name: 'Al' },
        { user_id: 'u2', name: 'Al Lee' },
      ]),
    ).toEqual([
      { text: 'cc ', mention: false },
      { text: '@Al Lee', mention: true },
      { text: ' and ', mention: false },
      { text: '@Al', mention: true },
    ]);
    expect(splitCommentBody('plain', [])).toEqual([{ text: 'plain', mention: false }]);
  });
});

describe('comment threads', () => {
  const thread = (id: string, extra: Partial<DocsComment> = {}): DocsComment =>
    ({ id, is_deleted: false, resolved_at: null, replies: [], ...extra }) as DocsComment;

  it('puts open threads before resolved ones and counts live comments', () => {
    const threads = [
      thread('a', { resolved_at: '2026-11-22T00:00:00Z' }),
      thread('b', {
        is_deleted: true,
        replies: [thread('b1'), thread('b2', { is_deleted: true })],
      }),
    ];
    const { open, resolved } = partitionThreads(threads);
    expect(open.map((item) => item.id)).toEqual(['b']);
    expect(resolved.map((item) => item.id)).toEqual(['a']);
    expect(countVisibleComments(threads)).toBe(2);
  });
});

describe('comment drafts', () => {
  it('keeps drafts until they are emptied or sent', () => {
    setCommentDraft('spec:new', { text: 'hello', mentions: [] });
    expect(getCommentDraft('spec:new')?.text).toBe('hello');
    expect(hasCommentDrafts()).toBe(true);
    setCommentDraft('spec:new', { text: '   ', mentions: [] });
    expect(getCommentDraft('spec:new')).toBeUndefined();
    setCommentDraft('spec:new', { text: 'again', mentions: [] });
    finishCommentRequest('spec:new');
    expect(getCommentDraft('spec:new')).toBeUndefined();
  });

  it('reuses the request ID for the same comment and renews it for a changed one', () => {
    const first = commentRequestId('spec:reply', 'payload-1');
    expect(commentRequestId('spec:reply', 'payload-1')).toBe(first);
    const changed = commentRequestId('spec:reply', 'payload-2');
    expect(changed).not.toBe(first);
    finishCommentRequest('spec:reply');
    expect(commentRequestId('spec:reply', 'payload-2')).not.toBe(changed);
  });
});
