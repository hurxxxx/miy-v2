/**
 * miy: mentions (COMMON-FEATURES-PLAN §2.2 "보강"; AI-DO 0002af062 had none).
 *
 * The composer keeps the people picked with "@" as drafts (`@이름` labels).
 * When the message is sent the labels still present in the body become
 * ranges `{user_id, start, length}` in UTF-16 code units (JavaScript string
 * indices), which the server checks against the body and the current
 * participants. Deleting the label text drops the mention.
 */
export interface DmMentionDraft {
  userId: string;
  label: string;
}

export interface DmMentionRange {
  user_id: string;
  start: number;
  length: number;
}

export interface DmMentionCandidate {
  id: string;
  name: string;
}

export interface DmActiveMentionQuery {
  /** Index of the "@". */
  start: number;
  query: string;
}

const MENTION_QUERY = /(^|\s)@([^\s@]{0,30})$/u;
export const DM_MENTION_PICKER_LIMIT = 8;

/** The "@query" being typed right before the caret, if any. */
export function activeDmMentionQuery(
  text: string,
  caret: number,
): DmActiveMentionQuery | null {
  const before = text.slice(0, Math.max(0, caret));
  const match = MENTION_QUERY.exec(before);
  if (!match) return null;
  return { start: before.length - match[2].length - 1, query: match[2] };
}

export function dmMentionCandidates(
  people: readonly DmMentionCandidate[],
  currentUserId: string | null | undefined,
  query: string,
): DmMentionCandidate[] {
  const needle = query.trim().toLocaleLowerCase();
  return people
    .filter((person) => person.id !== currentUserId)
    .filter((person) => !needle || person.name.toLocaleLowerCase().includes(needle))
    .slice(0, DM_MENTION_PICKER_LIMIT);
}

/** Replace the "@query" with "@이름 " and answer the new text and caret. */
export function insertDmMention(
  text: string,
  active: DmActiveMentionQuery,
  caret: number,
  name: string,
): { text: string; caret: number; label: string } {
  const label = `@${name}`;
  const next = `${text.slice(0, active.start)}${label} ${text.slice(caret)}`;
  return { text: next, caret: active.start + label.length + 1, label };
}

/** Ranges of the drafts whose labels remain in `body`, first free occurrence each. */
export function resolveDmMentionRanges(
  body: string,
  drafts: readonly DmMentionDraft[],
): DmMentionRange[] {
  const taken: Array<[number, number]> = [];
  const ranges: DmMentionRange[] = [];
  for (const draft of drafts) {
    if (draft.label.length < 2) continue;
    let from = 0;
    while (from <= body.length) {
      const start = body.indexOf(draft.label, from);
      if (start < 0) break;
      const end = start + draft.label.length;
      const free = taken.every(([s, e]) => end <= s || start >= e);
      const boundaryBefore = start === 0 || /\s/u.test(body[start - 1]);
      if (free && boundaryBefore) {
        taken.push([start, end]);
        ranges.push({ user_id: draft.userId, start, length: draft.label.length });
        break;
      }
      from = start + 1;
    }
  }
  return ranges.sort((left, right) => left.start - right.start);
}

/** Drafts for editing a message: its ranges read back as labels. */
export function dmMentionDraftsFromMessage(
  body: string,
  mentions: readonly DmMentionRange[] | undefined,
): DmMentionDraft[] {
  return (mentions ?? [])
    .filter((mention) => mention.start >= 0 && mention.start + mention.length <= body.length)
    .map((mention) => ({
      userId: mention.user_id,
      label: body.slice(mention.start, mention.start + mention.length),
    }));
}

export type DmBodyPart =
  | { kind: 'text'; text: string }
  | { kind: 'mention'; text: string; userId: string };

/** Split a body at its mention ranges for rendering. Bad ranges are ignored. */
export function dmMentionParts(
  body: string,
  mentions: readonly DmMentionRange[] | undefined,
): DmBodyPart[] {
  const parts: DmBodyPart[] = [];
  let at = 0;
  const sorted = [...(mentions ?? [])].sort((left, right) => left.start - right.start);
  for (const mention of sorted) {
    const end = mention.start + mention.length;
    if (mention.start < at || end > body.length || body[mention.start] !== '@') continue;
    if (mention.start > at) parts.push({ kind: 'text', text: body.slice(at, mention.start) });
    parts.push({ kind: 'mention', text: body.slice(mention.start, end), userId: mention.user_id });
    at = end;
  }
  if (at < body.length) parts.push({ kind: 'text', text: body.slice(at) });
  return parts;
}
