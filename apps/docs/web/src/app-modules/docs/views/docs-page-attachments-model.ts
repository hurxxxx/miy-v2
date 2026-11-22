import type { DocsPageAttachment } from '../api/docs-api';

/**
 * List bookkeeping for the attachment panel, kept away from React so the
 * ordering and out-of-order-response rules can be tested directly.
 */

/** Oldest first, matching the order the server lists them in. */
export function sortDocPageAttachments(
  attachments: readonly DocsPageAttachment[],
): DocsPageAttachment[] {
  return [...attachments].sort((left, right) => {
    const byDate = left.created_at.localeCompare(right.created_at);
    return byDate !== 0 ? byDate : left.id.localeCompare(right.id);
  });
}

/**
 * Add freshly uploaded files. Re-uploading the same file yields a new id, so
 * entries are matched on id and a repeated response never doubles a row.
 */
export function withUploadedAttachments(
  current: readonly DocsPageAttachment[],
  uploaded: readonly DocsPageAttachment[],
): DocsPageAttachment[] {
  const byId = new Map(current.map((item) => [item.id, item]));
  for (const item of uploaded) {
    byId.set(item.id, item);
  }
  return sortDocPageAttachments([...byId.values()]);
}

export function withoutAttachment(
  current: readonly DocsPageAttachment[],
  attachmentId: string,
): DocsPageAttachment[] {
  return current.filter((item) => item.id !== attachmentId);
}

export function docPageAttachmentErrorMessage(
  error: unknown,
  fallback: string,
): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return fallback;
}
