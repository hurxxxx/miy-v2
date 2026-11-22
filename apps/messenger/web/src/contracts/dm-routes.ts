export interface DmAttachmentUploadFile {
  size: number;
}

// miy: paths are app relative for runtime.api (the gateway adds `/api`).
export const DM_API_PREFIX = '/v1/dm';
export const DM_QUERY_LIMIT_MIN = 1;
export const DM_QUERY_LIMIT_MAX = 100;
export const DEFAULT_DM_MESSAGE_LIMIT = 50;
export const DEFAULT_DM_USER_SEARCH_LIMIT = 30;
// miy: user IDs are platform subjects and may be longer than a UUID.
export const DM_ROUTE_ID_MAX_LENGTH = 256;
export const DM_ROUTE_ID_ALLOWED_PATTERN = '^[A-Za-z0-9_-]+$';
// miy: platform Storage limit (AI-DO 50 MB).
export const DM_MAX_ATTACHMENT_BYTES = 10485760;

const DM_ROUTE_ID_PATTERN = new RegExp(DM_ROUTE_ID_ALLOWED_PATTERN, 'u');

export const dmRoutes = {
  users: (input: { q?: string; limit?: number } = {}) => {
    const params = new URLSearchParams();
    if (input.q != null) {
      params.set('q', input.q);
    }
    params.set(
      'limit',
      String(normalizeDmLimit(input.limit ?? DEFAULT_DM_USER_SEARCH_LIMIT)),
    );
    return `${DM_API_PREFIX}/users?${params.toString()}`;
  },
  conversations: () => `${DM_API_PREFIX}/conversations`,
  conversation: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}`,
  conversationParticipants: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/participants`,
  conversationParticipant: (conversationId: string, userId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/participants/${encodeSegment(userId)}`,
  leaveConversation: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/participants/me`,
  messages: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/messages`,
  listMessages: (
    conversationId: string,
    input: { limit?: number; before?: string | Date | number | null; ids?: readonly string[] } = {},
  ) => {
    const params = new URLSearchParams({
      limit: String(normalizeDmLimit(input.limit ?? DEFAULT_DM_MESSAGE_LIMIT)),
    });
    if (typeof input.before === 'number') {
      params.set('before_sequence', String(input.before));
    } else if (input.before) {
      params.set(
        'before',
        typeof input.before === 'string'
          ? input.before
          : input.before.toISOString(),
      );
    }
    if (input.ids?.length) params.set('ids', input.ids.join(','));
    return `${dmRoutes.messages(conversationId)}?${params.toString()}`;
  },
  messageAttachments: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/attachments`,
  attachmentDownload: (attachmentId: string) =>
    `${DM_API_PREFIX}/attachments/${encodeSegment(attachmentId)}/download`,
  attachmentPreview: (attachmentId: string) =>
    `${DM_API_PREFIX}/attachments/${encodeSegment(attachmentId)}/preview`,
  // miy: author edit/delete, mute and unsent attachment cancel.
  message: (conversationId: string, messageId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/messages/${encodeSegment(messageId)}`,
  // miy: reactions and message search.
  reaction: (conversationId: string, messageId: string, reaction: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/messages/${encodeSegment(messageId)}/reactions/${encodeSegment(reaction)}`,
  search: (input: {
    q: string;
    conversationId?: string | null;
    cursor?: string | null;
    limit?: number;
  }) => {
    const params = new URLSearchParams({ q: input.q });
    if (input.conversationId) params.set('conversation_id', input.conversationId);
    if (input.cursor) params.set('cursor', input.cursor);
    params.set('limit', String(normalizeDmLimit(input.limit ?? 20)));
    return `${DM_API_PREFIX}/search?${params.toString()}`;
  },
  mute: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/mute`,
  attachment: (attachmentId: string) =>
    `${DM_API_PREFIX}/attachments/${encodeSegment(attachmentId)}`,
  // miy: bytes go through the file gateway (`runtime.fileApi`).
  attachmentContent: (attachmentId: string) =>
    `/dm/attachments/${encodeSegment(attachmentId)}/content`,
  markRead: (conversationId: string) =>
    `${DM_API_PREFIX}/conversations/${encodeSegment(conversationId)}/read`,
  reports: () => `${DM_API_PREFIX}/reports`,
  terms: () => `${DM_API_PREFIX}/terms`,
  termsAccept: () => `${DM_API_PREFIX}/terms/accept`,
  blocks: () => `${DM_API_PREFIX}/blocks`,
  block: (userId: string) =>
    `${DM_API_PREFIX}/blocks/${encodeSegment(userId)}`,
} as const;

export function normalizeDmLimit(value: number): number {
  if (
    !Number.isInteger(value) ||
    value < DM_QUERY_LIMIT_MIN ||
    value > DM_QUERY_LIMIT_MAX
  ) {
    throw new RangeError(
      `DM query limit must be an integer from ${DM_QUERY_LIMIT_MIN} to ${DM_QUERY_LIMIT_MAX}.`,
    );
  }
  return value;
}

export function isDmRouteId(value: string): boolean {
  return (
    value.length > 0 &&
    value.length <= DM_ROUTE_ID_MAX_LENGTH &&
    DM_ROUTE_ID_PATTERN.test(value)
  );
}

/**
 * miy: attachment URLs are app file paths (`/dm/attachments/{id}/content`,
 * optionally `?disposition=inline`) read with `runtime.fileApi`; AI-DO
 * resolved same-origin signed URLs against `baseUrl`. Anything else is null.
 */
export function resolveDmAttachmentUrl(
  value: string | null | undefined,
  baseUrl = 'https://miy.invalid',
): string | null {
  if (!value || !value.startsWith('/') || value.startsWith('//')) {
    return null;
  }
  try {
    const base = new URL(baseUrl);
    const url = new URL(value, base);
    if (
      url.origin !== base.origin ||
      url.username ||
      url.password ||
      url.hash ||
      !isDmAttachmentUrl(url) ||
      (url.search && url.search !== '?disposition=inline')
    ) {
      return null;
    }
    return `${url.pathname}${url.search}`;
  } catch {
    return null;
  }
}

export function isDmAttachmentUploadFileAllowed(
  file: DmAttachmentUploadFile,
): boolean {
  return (
    Number.isFinite(file.size) &&
    file.size >= 0 &&
    file.size <= DM_MAX_ATTACHMENT_BYTES
  );
}

function encodeSegment(value: string): string {
  if (!isDmRouteId(value)) {
    throw new Error(
      `DM route id must be 1-${DM_ROUTE_ID_MAX_LENGTH} URL segment-safe characters.`,
    );
  }
  return encodeURIComponent(value);
}

function isDmAttachmentUrl(url: URL): boolean {
  const segments = url.pathname.slice(1).split('/');
  if (segments.length !== 4 || segments[0] !== 'dm' || segments[1] !== 'attachments') {
    return false;
  }
  let attachmentId: string;
  try {
    attachmentId = decodeURIComponent(segments[2] ?? '');
  } catch {
    return false;
  }
  return isDmRouteId(attachmentId) && segments[3] === 'content';
}
