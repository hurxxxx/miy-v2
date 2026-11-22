import { useSyncExternalStore } from 'react';
import { requestId } from '@miy/ui';

import type { DocsCommentMention } from '../api/docs-api';

/**
 * Unsent comment drafts by composer (`<docId>:<pageId>:new`,
 * `…:reply:<commentId>`, `…:edit:<commentId>`). They live for the app
 * session so that switching pages or a failed send never loses what the
 * person typed; the app root reports them to the portal as unsaved work.
 * Trashing a page or doc drops its drafts (`clearCommentDrafts`).
 */
export type CommentDraft = { text: string; mentions: DocsCommentMention[] };

const drafts = new Map<string, CommentDraft>();
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function getCommentDraft(key: string): CommentDraft | undefined {
  return drafts.get(key);
}

/** Store a draft; an empty text (or `null`) removes it. */
export function setCommentDraft(key: string, draft: CommentDraft | null): void {
  const had = drafts.has(key);
  if (!draft || !draft.text.trim()) {
    if (!had) return;
    drafts.delete(key);
  } else {
    drafts.set(key, draft);
    if (had) return;
  }
  emit();
}

// The request ID of a send whose answer was not seen, by composer: sending
// the same comment again reuses it (the server answers the comment already
// made instead of making a second one); a changed comment gets a new ID.
const attempts = new Map<string, { payload: string; id: string }>();

export function commentRequestId(key: string, payload: string): string {
  const attempt = attempts.get(key);
  if (attempt && attempt.payload === payload) return attempt.id;
  const id = requestId();
  attempts.set(key, { payload, id });
  return id;
}

/** After a confirmed send: the composer starts over. */
export function finishCommentRequest(key: string): void {
  attempts.delete(key);
  setCommentDraft(key, null);
}

/** Drop every draft under `scope` (`<docId>` or `<docId>:<pageId>`). */
export function clearCommentDrafts(scope: string): void {
  const prefix = `${scope}:`;
  let changed = false;
  for (const key of [...drafts.keys()]) {
    if (key.startsWith(prefix)) {
      drafts.delete(key);
      changed = true;
    }
  }
  for (const key of [...attempts.keys()]) {
    if (key.startsWith(prefix)) attempts.delete(key);
  }
  if (changed) emit();
}

export function hasCommentDrafts(): boolean {
  return drafts.size > 0;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Whether any composer holds unsent text (for the navigation guard). */
export function useHasCommentDrafts(): boolean {
  return useSyncExternalStore(subscribe, hasCommentDrafts, hasCommentDrafts);
}
