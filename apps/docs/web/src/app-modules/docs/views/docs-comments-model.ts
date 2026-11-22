import type { DocsComment, DocsCommentMention } from '../api/docs-api';

/**
 * Page comments (miy, 보강; AI-DO Docs had none). Pure helpers for the
 * comment composer's `@` mentions and the thread list.
 */

export const DOCS_COMMENT_MAX_CHARS = 10_000;
export const DOCS_COMMENT_MAX_MENTIONS = 20;

/** The `@query` being typed right before the caret, if any. */
export function activeMentionQuery(
  text: string,
  caret: number,
): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const at = before.lastIndexOf('@');
  if (at < 0) return null;
  if (at > 0 && !/\s/.test(before[at - 1] ?? '')) return null;
  const query = before.slice(at + 1);
  if (/\s/.test(query) || query.length > 40) return null;
  return { start: at, query };
}

/** Replace the `@query` at `start..caret` with `@name ` and move the caret. */
export function insertMention(
  text: string,
  start: number,
  caret: number,
  name: string,
): { text: string; caret: number } {
  const inserted = `@${name} `;
  return {
    text: `${text.slice(0, start)}${inserted}${text.slice(caret)}`,
    caret: start + inserted.length,
  };
}

/**
 * The people picked in the composer whose `@name` is still in the body, in
 * order, without duplicates (removing the text removes the mention).
 */
export function mentionIdsInBody(
  body: string,
  picked: DocsCommentMention[],
): string[] {
  const ids: string[] = [];
  for (const person of picked) {
    if (ids.includes(person.user_id)) continue;
    if (person.name && body.includes(`@${person.name}`)) ids.push(person.user_id);
  }
  return ids.slice(0, DOCS_COMMENT_MAX_MENTIONS);
}

export type CommentSegment = { text: string; mention: boolean };

/** The body split into plain text and `@name` runs of the comment's mentions. */
export function splitCommentBody(
  body: string,
  mentions: DocsCommentMention[],
): CommentSegment[] {
  const names = [...new Set(mentions.map((person) => person.name).filter(Boolean))].sort(
    (a, b) => b.length - a.length,
  );
  if (!names.length) return body ? [{ text: body, mention: false }] : [];
  const segments: CommentSegment[] = [];
  let plain = '';
  let index = 0;
  while (index < body.length) {
    const name =
      body[index] === '@'
        ? names.find((candidate) => body.startsWith(`@${candidate}`, index))
        : undefined;
    if (name) {
      if (plain) segments.push({ text: plain, mention: false });
      plain = '';
      segments.push({ text: `@${name}`, mention: true });
      index += name.length + 1;
    } else {
      plain += body[index];
      index += 1;
    }
  }
  if (plain) segments.push({ text: plain, mention: false });
  return segments;
}

/** Threads still open first (newest activity order kept within each group). */
export function partitionThreads(threads: DocsComment[]): {
  open: DocsComment[];
  resolved: DocsComment[];
} {
  return {
    open: threads.filter((thread) => !thread.resolved_at),
    resolved: threads.filter((thread) => Boolean(thread.resolved_at)),
  };
}

/** Comments shown (roots and replies, deleted placeholders excluded). */
export function countVisibleComments(threads: DocsComment[]): number {
  let count = 0;
  for (const thread of threads) {
    if (!thread.is_deleted) count += 1;
    count += (thread.replies ?? []).filter((reply) => !reply.is_deleted).length;
  }
  return count;
}
