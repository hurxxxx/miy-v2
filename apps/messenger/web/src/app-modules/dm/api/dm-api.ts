import type { components, paths } from '../../../api.generated';
import {
  DM_MAX_ATTACHMENT_BYTES,
  dmRoutes,
  isDmAttachmentUploadFileAllowed,
  normalizeDmAddParticipantsRequest,
  normalizeDmAttachmentUrlResponse,
  normalizeDmBlockListResponse,
  normalizeDmBlockResult,
  normalizeDmCreateConversationRequest,
  normalizeDmConversation,
  normalizeDmConversationListResponse,
  normalizeDmMessage,
  normalizeDmMessageAttachment,
  normalizeDmMessageListResponse,
  normalizeDmReportResult,
  normalizeDmSendMessageRequest,
  normalizeDmTermsAcceptResult,
  normalizeDmTermsStatus,
  normalizeDmUpdateConversationRequest,
  normalizeDmUser,
  normalizeDmUserListResponse,
  DM_API_PREFIX as DM_API_PREFIX_PATH,
} from '../../../contracts/dm';
import type {
  DmAddParticipantsRequest,
  DmAttachmentUrlResponse,
  DmBlockListResponse,
  DmBlockResult,
  DmConversation,
  DmConversationListResponse,
  DmMessage,
  DmMessageAttachment,
  DmMessageListResponse,
  DmReportRequest,
  DmReportResult,
  DmSendMessageRequest,
  DmTermsAcceptResult,
  DmTermsStatus,
  DmUpdateConversationRequest,
  DmUser,
  DmCreateConversationRequest,
} from '../../../contracts/dm';

import { readJson } from '@miy/sdk';
import { requestId } from '@miy/components/ui/utilities';

import {
  resolveDmMentionRanges,
  type DmMentionDraft,
} from '../views/dm-mention-model';

import {
  ApiRequestError,
  apiFetchFile,
  apiFetchJson,
  toApiRequestError,
} from '../../../platform/api/client';

export type {
  DmAttachmentUrlResponse,
  DmBlockItem,
  DmConversation,
  DmConversationListResponse,
  DmConversationParticipant,
  DmMessage,
  DmMessageAttachment,
  DmMessageListResponse,
  DmRealtimeEvent,
  DmReportCategory,
  DmSafetyCapability,
  DmTermsStatus,
  DmUser,
} from '../../../contracts/dm';

export type DmThread = DmConversation;
export type DmThreadListResponse = DmConversationListResponse;

type SearchDmUsersOptions = {
  includeCurrent?: boolean;
  limit?: number;
  workspaceKey?: string;
};

function dmUsersPath({
  includeCurrent,
  limit,
  q,
  workspaceKey,
}: {
  includeCurrent: boolean;
  limit: number;
  q: string;
  workspaceKey?: string;
}): string {
  let path = dmRoutes.users({ q, limit });
  if (includeCurrent) {
    const separator = path.includes('?') ? '&' : '?';
    path = `${path}${separator}include_current=true`;
  }
  if (workspaceKey) {
    const separator = path.includes('?') ? '&' : '?';
    path = `${path}${separator}workspace_key=${encodeURIComponent(workspaceKey)}`;
  }
  return path;
}

export function searchDmUsers(
  token: string,
  query: string,
  options: SearchDmUsersOptions = {},
): Promise<DmUser[]> {
  const { includeCurrent = false, limit = 30, workspaceKey } = options;
  const path = dmUsersPath({ includeCurrent, limit, q: query, workspaceKey });
  return fetchDmResponse(
    'dm:users',
    path,
    token,
    {},
    normalizeDmUserListResponse,
  );
}

export function listDmConversations(
  token: string,
): Promise<DmConversationListResponse> {
  return fetchDmResponse(
    'dm:conversations',
    dmRoutes.conversations(),
    token,
    {},
    normalizeDmConversationListResponse,
  );
}

export function createDirectDmConversation(
  token: string,
  recipientUserId: string,
): Promise<DmConversation> {
  const body = requireDmRequest(
    'dm:create-conversation',
    { recipient_user_id: recipientUserId },
    normalizeDmCreateConversationRequest,
  );
  return fetchDmResponse(
    'dm:create-conversation',
    dmRoutes.conversations(),
    token,
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
    normalizeDmConversation,
  );
}

export function createGroupDmConversation(
  token: string,
  participantUserIds: string[],
  title: string,
): Promise<DmConversation> {
  const body = requireDmRequest(
    'dm:create-conversation',
    {
      participant_user_ids: participantUserIds,
      title,
    },
    normalizeDmCreateConversationRequest,
  );
  return fetchDmResponse(
    'dm:create-conversation',
    dmRoutes.conversations(),
    token,
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
    normalizeDmConversation,
  );
}

export function updateDmConversationTitle(
  token: string,
  conversationId: string,
  title: string,
): Promise<DmConversation> {
  const body = requireDmRequest(
    'dm:update-conversation',
    { title },
    normalizeDmUpdateConversationRequest,
  );
  return fetchDmResponse(
    'dm:update-conversation',
    dmRoutes.conversation(conversationId),
    token,
    {
      method: 'PATCH',
      body: JSON.stringify(body),
    },
    normalizeDmConversation,
  );
}

export function addDmConversationParticipants(
  token: string,
  conversationId: string,
  userIds: string[],
): Promise<DmConversation> {
  const body = requireDmRequest(
    'dm:add-participants',
    { user_ids: userIds },
    normalizeDmAddParticipantsRequest,
  );
  return fetchDmResponse(
    'dm:add-participants',
    dmRoutes.conversationParticipants(conversationId),
    token,
    {
      method: 'POST',
      body: JSON.stringify(body),
    },
    normalizeDmConversation,
  );
}

export function removeDmConversationParticipant(
  token: string,
  conversationId: string,
  userId: string,
): Promise<DmConversation> {
  return fetchDmResponse(
    'dm:remove-participant',
    dmRoutes.conversationParticipant(conversationId, userId),
    token,
    { method: 'DELETE' },
    normalizeDmConversation,
  );
}

export function leaveDmConversation(
  token: string,
  conversationId: string,
): Promise<void> {
  return apiFetchJson<void>(dmRoutes.leaveConversation(conversationId), token, {
    method: 'DELETE',
  });
}

export function listDmMessages(
  token: string,
  conversationId: string,
  limit = 50,
  // Older pages prefer the sequence cursor, which breaks timestamp ties.
  before: number | string | null = null,
  ids?: readonly string[],
): Promise<DmMessageListResponse> {
  return fetchDmResponse(
    'dm:messages',
    dmRoutes.listMessages(conversationId, { limit, before, ids }),
    token,
    {},
    normalizeDmMessageListResponse,
  );
}

/**
 * miy: AI-DO posted the file as multipart. The messenger reserves platform
 * Storage first (JSON, the server checks the conversation, block, terms and
 * file name) and then streams the bytes through the file gateway
 * (`runtime.fileApi`); the answer is the uploaded attachment.
 */
export function uploadDmAttachment(
  token: string,
  conversationId: string,
  file: File,
): Promise<DmMessageAttachment> {
  const path = dmRoutes.messageAttachments(conversationId);
  assertDmAttachmentUploadFile(file);
  const body = JSON.stringify({
    request_id: requestId(),
    filename: file.name || 'clipboard-file',
    content_type: file.type || null,
    size_bytes: file.size,
  } satisfies components['schemas']['DmAttachmentReserveRequest']);
  return limitUploads(async () => {
    const reserved = await retryUncertainUpload(() => fetchDmResponse(
      'dm:upload-attachment',
      path,
      token,
      {
        method: 'POST',
        body,
      },
      normalizeDmMessageAttachment,
    ));
    const response = await retryUncertainUpload(() => apiFetchFile(
      dmRoutes.attachmentContent(reserved.id),
      token,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: file,
      },
    ));
    let value: unknown;
    try {
      value = await readJson<components['schemas']['DmMessageAttachmentItem']>(response);
    } catch (error) {
      throw toApiRequestError(error);
    }
    const uploaded = normalizeDmMessageAttachment(value);
    if (!uploaded) {
      throw new ApiRequestError(
        502,
        'Invalid response format for dm:upload-attachment.',
        value,
      );
    }
    return uploaded;
  });
}

async function retryUncertainUpload<T>(attempt: () => Promise<T>): Promise<T> {
  try {
    return await attempt();
  } catch (error) {
    if (error instanceof ApiRequestError && (error.status === 0 || error.status >= 500)) {
      return attempt();
    }
    throw error;
  }
}

// The file gateway admits a few transfers at a time; queue the rest.
const MAX_PARALLEL_UPLOADS = 3;
let activeUploads = 0;
const uploadQueue: Array<() => void> = [];

async function limitUploads<T>(task: () => Promise<T>): Promise<T> {
  if (activeUploads >= MAX_PARALLEL_UPLOADS) {
    await new Promise<void>((resolve) => uploadQueue.push(resolve));
  }
  activeUploads += 1;
  try {
    return await task();
  } finally {
    activeUploads -= 1;
    uploadQueue.shift()?.();
  }
}

/** miy: cancel an unsent attachment removed from the composer. */
export function cancelDmAttachment(
  token: string,
  attachmentId: string,
): Promise<void> {
  return apiFetchJson<void>(dmRoutes.attachment(attachmentId), token, {
    method: 'DELETE',
  });
}

/**
 * miy: attachment bytes are read inside the frame with the frame credential
 * and shown through a local object URL (no signed URL). The caller revokes it.
 */
export async function loadDmAttachmentObjectUrl(
  token: string,
  path: string,
): Promise<string> {
  const response = await apiFetchFile(path, token);
  return URL.createObjectURL(await response.blob());
}

export function getDmAttachmentDownloadUrl(
  token: string,
  attachmentId: string,
): Promise<DmAttachmentUrlResponse> {
  return fetchDmResponse(
    'dm:attachment-download',
    dmRoutes.attachmentDownload(attachmentId),
    token,
    {},
    normalizeDmAttachmentUrlResponse,
  );
}

export function getDmAttachmentPreviewUrl(
  token: string,
  attachmentId: string,
): Promise<DmAttachmentUrlResponse> {
  return fetchDmResponse(
    'dm:attachment-preview',
    dmRoutes.attachmentPreview(attachmentId),
    token,
    {},
    normalizeDmAttachmentUrlResponse,
  );
}

export function sendDmMessage(
  token: string,
  conversationId: string,
  body: string,
  attachmentIds: string[] = [],
  replyToMessageId: string | null = null,
  // miy: people picked with "@"; ranges are resolved on the normalized body.
  mentionDrafts: readonly DmMentionDraft[] = [],
): Promise<DmMessage> {
  const payload = requireDmRequest(
    'dm:send-message',
    {
      body,
      attachment_ids: attachmentIds,
      ...(replyToMessageId ? { reply_to_message_id: replyToMessageId } : {}),
    },
    normalizeDmSendMessageRequest,
  );
  const mentions = resolveDmMentionRanges(payload.body, mentionDrafts);
  return fetchDmResponse(
    'dm:send-message',
    dmRoutes.messages(conversationId),
    token,
    {
      method: 'POST',
      body: JSON.stringify(mentions.length ? { ...payload, mentions } : payload),
    },
    normalizeDmMessage,
  );
}

/** 신고(ADR 0025 개정 2026-11-18). 관리자에게는 고른 메시지(와 첨부)·메모만 간다. */
export function reportDm(
  token: string,
  request: DmReportRequest,
): Promise<DmReportResult> {
  return fetchDmResponse(
    'dm:report',
    dmRoutes.reports(),
    token,
    { method: 'POST', body: JSON.stringify(request) },
    normalizeDmReportResult,
  );
}

export function listDmBlocks(token: string): Promise<DmBlockListResponse> {
  return fetchDmResponse(
    'dm:blocks',
    dmRoutes.blocks(),
    token,
    {},
    normalizeDmBlockListResponse,
  );
}

/** 1:1 차단·해제(멱등). 공유 단체 대화는 그대로 보인다. */
export function setDmBlock(
  token: string,
  userId: string,
  blocked: boolean,
): Promise<DmBlockResult> {
  return fetchDmResponse(
    'dm:block',
    dmRoutes.block(userId),
    token,
    { method: blocked ? 'PUT' : 'DELETE' },
    normalizeDmBlockResult,
  );
}

/** 사용자 작성 콘텐츠 이용규칙 — 근태 앱과 같은 원장(ADR 0025 개정 2026-11-18). */
export function getDmTerms(token: string): Promise<DmTermsStatus> {
  return fetchDmResponse('dm:terms', dmRoutes.terms(), token, {}, normalizeDmTermsStatus);
}

export function acceptDmTerms(token: string, version: string): Promise<DmTermsAcceptResult> {
  return fetchDmResponse(
    'dm:terms-accept',
    dmRoutes.termsAccept(),
    token,
    { method: 'POST', body: JSON.stringify({ version } satisfies components['schemas']['UgcTermsAcceptRequest']) },
    normalizeDmTermsAcceptResult,
  );
}

/** miy: the verified caller as the app server sees it (AI-DO: the session user). */
export interface DmMe {
  user: DmUser;
  manager: boolean;
}

type DmMeResponse = paths['/v1/dm/me']['get']['responses'][200]['content']['application/json'];

export async function getDmMe(token: string): Promise<DmMe> {
  const value = await apiFetchJson<DmMeResponse>(
    `${DM_API_PREFIX_PATH}/me`,
    token,
  );
  const user = normalizeDmUser(value?.user);
  if (!user) {
    throw new ApiRequestError(502, 'Invalid response format for dm:me.', value);
  }
  return { user, manager: value.manager === true };
}

/** miy: author edit. */
export function editDmMessage(
  token: string,
  conversationId: string,
  messageId: string,
  body: string,
  // miy: the edited body's mentions replace the old ones.
  mentionDrafts: readonly DmMentionDraft[] = [],
): Promise<DmMessage> {
  const trimmed = body.trim();
  const mentions = resolveDmMentionRanges(trimmed, mentionDrafts);
  return fetchDmResponse(
    'dm:edit-message',
    dmRoutes.message(conversationId, messageId),
    token,
    {
      method: 'PATCH',
      body: JSON.stringify((mentions.length ? { body: trimmed, mentions } : { body: trimmed }) satisfies components['schemas']['DmEditMessageRequest']),
    },
    normalizeDmMessage,
  );
}

/** miy: add or remove the caller's reaction; the answer is the message. */
export function setDmReaction(
  token: string,
  conversationId: string,
  messageId: string,
  reaction: string,
  on: boolean,
): Promise<DmMessage> {
  return fetchDmResponse(
    'dm:reaction',
    dmRoutes.reaction(conversationId, messageId, reaction),
    token,
    { method: on ? 'PUT' : 'DELETE' },
    normalizeDmMessage,
  );
}

/** miy: one message search hit (text only, conversations the caller is in). */
export type DmSearchHit = components['schemas']['SearchItem'];
export type DmSearchPage = components['schemas']['SearchResponse'];

function normalizeDmSearchPage(value: unknown): DmSearchPage | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const record = value as { items?: unknown; next_cursor?: unknown };
  if (!Array.isArray(record.items)) return null;
  const items: DmSearchHit[] = [];
  for (const item of record.items) {
    if (!item || typeof item !== 'object') return null;
    const hit = item as Record<string, unknown>;
    if (
      typeof hit.message_id !== 'string' ||
      typeof hit.conversation_id !== 'string' ||
      typeof hit.conversation_name !== 'string' ||
      (hit.conversation_type !== 'direct' && hit.conversation_type !== 'group') ||
      typeof hit.sequence !== 'number' ||
      typeof hit.sender_id !== 'string' ||
      typeof hit.sender_name !== 'string' ||
      typeof hit.snippet !== 'string' ||
      typeof hit.created_at !== 'string'
    ) {
      return null;
    }
    items.push(hit as unknown as DmSearchHit);
  }
  const cursor = record.next_cursor;
  if (cursor !== null && cursor !== undefined && typeof cursor !== 'string') return null;
  return { items, next_cursor: typeof cursor === 'string' ? cursor : null };
}

/** miy: search message text in one conversation or in all of the caller's. */
export function searchDmMessages(
  token: string,
  input: { q: string; conversationId?: string | null; cursor?: string | null; limit?: number },
): Promise<DmSearchPage> {
  return fetchDmResponse(
    'dm:search',
    dmRoutes.search(input),
    token,
    {},
    normalizeDmSearchPage,
  );
}

/** miy: author delete (the answer is the placeholder message). */
export function deleteDmMessage(
  token: string,
  conversationId: string,
  messageId: string,
): Promise<DmMessage> {
  return fetchDmResponse(
    'dm:delete-message',
    dmRoutes.message(conversationId, messageId),
    token,
    { method: 'DELETE' },
    normalizeDmMessage,
  );
}

/** miy: mute this conversation's platform notifications for me. */
export function setDmConversationMuted(
  token: string,
  conversationId: string,
  muted: boolean,
): Promise<DmConversation> {
  return fetchDmResponse(
    'dm:mute',
    dmRoutes.mute(conversationId),
    token,
    { method: 'PUT', body: JSON.stringify({ muted } satisfies components['schemas']['DmMuteRequest']) },
    normalizeDmConversation,
  );
}

function markDmConversationRead(
  token: string,
  conversationId: string,
): Promise<DmConversation> {
  return fetchDmResponse(
    'dm:mark-read',
    dmRoutes.markRead(conversationId),
    token,
    { method: 'PATCH' },
    normalizeDmConversation,
  );
}

export const listDmThreads = listDmConversations;
export const createDmThread = createDirectDmConversation;
export const createGroupDmThread = createGroupDmConversation;
export const markDmThreadRead = markDmConversationRead;

function fetchDmResponse<T>(
  label: string,
  path: string,
  token: string,
  init: RequestInit,
  normalize: (value: unknown) => T | null,
): Promise<T> {
  return apiFetchJson<unknown>(path, token, init).then((value) => {
    const normalized = normalize(value);
    if (!normalized) {
      throw new ApiRequestError(
        502,
        `Invalid response format for ${label}.`,
        value,
      );
    }
    return normalized;
  });
}

function assertDmAttachmentUploadFile(file: Pick<File, 'size'>): void {
  if (!isDmAttachmentUploadFileAllowed(file)) {
    throw new ApiRequestError(
      400,
      `Attachments must be ${DM_MAX_ATTACHMENT_BYTES / (1024 * 1024)}MB or smaller.`,
      { size: file.size },
    );
  }
}

function requireDmRequest<
  T extends
    | DmAddParticipantsRequest
    | DmCreateConversationRequest
    | DmSendMessageRequest
    | DmUpdateConversationRequest,
>(label: string, value: unknown, normalize: (value: unknown) => T | null): T {
  const normalized = normalize(value);
  if (!normalized) {
    throw new ApiRequestError(
      400,
      `Invalid request format for ${label}.`,
      value,
    );
  }
  return normalized;
}
