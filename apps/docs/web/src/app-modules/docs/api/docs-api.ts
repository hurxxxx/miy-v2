import type { components, paths } from '../../../api.generated';
import { checkedResponse } from '@miy/sdk';
import { requestId } from '@miy/ui';

import {
  ApiRequestError,
  apiFetchJsonWithMappedError,
  toApiRequestError,
} from '../../../platform/api/client';
import type { AppApi, AppFileApi } from '../../../platform/runtime/runtime-context';

/**
 * AI-DO apps/web/src/app-modules/docs/api/docs-api.ts for miy.
 *
 * - Calls go through `runtime.api` with app-relative paths (`/v1/docs/...`
 *   reaches `/api/v1/docs/...`, AI-DO's path). AI-DO took a bearer token and
 *   rewrote paths to `/api/v1/workspaces/{slug}/docs/...`; the space-scoped
 *   calls here add `?space=<id>` instead (`spaceId` in place of
 *   `workspaceSlug`).
 * - Link shares (`share_token`, `/docs/shared/...`), the Yjs collaboration
 *   session/snapshot calls and PMS task links are not ported.
 * - Page content saves send `expected_revision`; the server answers 409
 *   `docs_page_revision_conflict` for a save against an older revision.
 * - Attachments and body images go through the platform file gateway
 *   (`runtime.fileApi`).
 */
export class DocsApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    public readonly code: string | null = null,
  ) {
    super(message);
  }
}

function errorCode(error: ApiRequestError): string | null {
  const payload = error.payload as { code?: unknown } | null;
  return typeof payload?.code === 'string' ? payload.code : null;
}

function request<T>(
  path: string,
  api: AppApi | null | undefined,
  init: RequestInit = {},
): Promise<T> {
  return apiFetchJsonWithMappedError<T, DocsApiError>(
    path,
    api,
    init,
    (error) => new DocsApiError(error.status, error.message, errorCode(error)),
  );
}

function withSpace(path: string, spaceId?: string | null): string {
  if (!spaceId) return path;
  const separator = path.includes('?') ? '&' : '?';
  return `${path}${separator}space=${encodeURIComponent(spaceId)}`;
}

const segment = encodeURIComponent;
const attachmentContentRoute = '/files/docs/attachments/{attachment_id}/content';
type Models = components['schemas'];
type AttachmentUpload = paths[typeof attachmentContentRoute]['put'];
type HubQuery = NonNullable<paths['/v1/docs/hub']['get']['parameters']['query']>;

export const DOCS_PAGE_REVISION_CONFLICT = 'docs_page_revision_conflict';

export function isDocsRevisionConflict(error: unknown): boolean {
  return (
    error instanceof DocsApiError &&
    error.status === 409 &&
    error.code === DOCS_PAGE_REVISION_CONFLICT
  );
}

export type DocsDocType = Models['DocType'];
export type DocsContentFormat = Models['ContentFormat'] | 'html';
export type DocsHubContentFormat = DocsContentFormat | 'mixed';

export type DocsShareSummary = Omit<Models['ShareSummary'], 'link_access_level'> & {
  link_access_level?: Models['DocAccessLevel'] | null;
};
export type DocsPrimaryTarget = Models['PrimaryTarget'];
export interface DocsCollectionSummary {
  id: string;
  name: string;
  scope: 'workspace' | 'private';
}

// The screen still understands legacy formats and projections in fixtures.
export type DocsHubItem = Omit<Models['HubItem'], 'content_format' | 'collection' | 'source_deeplink' | 'sharing_summary'> & {
  content_format: DocsHubContentFormat;
  collection: DocsCollectionSummary | null;
  source_deeplink: string | null;
  sharing_summary: DocsShareSummary | null;
};
export type DocsHubResponse = Omit<Models['HubResponse'], 'items'> & { items: DocsHubItem[] };

export function getDocsItemPrimaryTargetId(
  item: DocsHubItem,
  app?: string,
  type?: string,
): string | null {
  const target = item.primary_target;
  if (!target) {
    return null;
  }
  if (app && target.app !== app) {
    return null;
  }
  if (type && target.type !== type) {
    return null;
  }
  return target.id;
}

export function getDocsItemPrimaryTargetSortOrder(item: DocsHubItem): number {
  return item.primary_target?.sort_order ?? 0;
}

export function withDocsItemPrimaryTargetSortOrder(
  item: DocsHubItem,
  sortOrder: number,
): DocsHubItem {
  if (!item.primary_target) {
    return item;
  }
  return {
    ...item,
    primary_target: {
      ...item.primary_target,
      sort_order: sortOrder,
    },
  };
}

export type DocsPageItem = Omit<Models['PageItem'], 'content_format'> & {
  content_format: DocsContentFormat;
};
export type DocsPageListResponse = Omit<Models['PageListResponse'], 'items'> & { items: DocsPageItem[] };
export type DocsPageAttachment = Omit<Models['AttachmentItem'], 'status'> & { status?: Models['AttachmentItem']['status'] };
export type DocsPageAttachmentListResponse = Omit<Models['AttachmentListResponse'], 'items'> & { items: DocsPageAttachment[] };

export function mediaResourceTypeForDocsPage(): 'docs_native_page' {
  return 'docs_native_page';
}

/** The Docs-owned space holding a doc, for links into another space. */
export type DocsSpaceLocation = Models['SpaceLocation'];
export type FavoriteDocItem = Omit<Models['FavoriteDoc'], 'space'> & { space?: DocsSpaceLocation | null };
export type RecentPageItem = Omit<Models['RecentPage'], 'space'> & { space?: DocsSpaceLocation | null };
export type ShareableUserItem = Models['ShareableUser'];
export type NativeUserShareItem = Models['DocUserShare'];
export type NativeDocSharingResponse = Models['DocSharing'];

export const DOC_CONTENT_FORMAT_VALUES = ['block', 'html'] as const;
export const DOC_HUB_CONTENT_FORMAT_VALUES = [
  'block',
  'html',
  'mixed',
] as const;

export function isDocsContentFormat(
  value: unknown,
): value is DocsContentFormat {
  return DOC_CONTENT_FORMAT_VALUES.includes(value as DocsContentFormat);
}

export function resolveDocsContentFormat(
  page?: { content_format?: DocsContentFormat | null } | null,
): DocsContentFormat {
  if (isDocsContentFormat(page?.content_format)) {
    return page.content_format;
  }
  return 'block';
}

export function isDocsHubContentFormat(
  value: unknown,
): value is DocsHubContentFormat {
  return DOC_HUB_CONTENT_FORMAT_VALUES.includes(value as DocsHubContentFormat);
}

export function resolveDocsHubContentFormat(
  doc?: { content_format?: DocsHubContentFormat | null } | null,
): DocsHubContentFormat {
  if (isDocsHubContentFormat(doc?.content_format)) {
    return doc.content_format;
  }
  return 'block';
}

export function listDocsHub(
  api: AppApi | null | undefined,
  params: Omit<HubQuery, 'space'> = {},
  spaceId?: string | null,
  init: RequestInit = {},
): Promise<DocsHubResponse> {
  const qs = new URLSearchParams();
  if (params.view) qs.set('view', params.view);
  if (params.q) qs.set('q', params.q);
  if (params.sort_by) qs.set('sort_by', params.sort_by);
  if (params.sort_dir) qs.set('sort_dir', params.sort_dir);
  if (params.page) qs.set('page', String(params.page));
  if (params.page_size) qs.set('page_size', String(params.page_size));
  if (params.source_app) qs.set('source_app', params.source_app);
  if (params.source_kind) qs.set('source_kind', params.source_kind);
  if (params.doc_type) qs.set('doc_type', params.doc_type);
  if (params.space_id) qs.set('space_id', params.space_id);
  return request<Models['HubResponse']>(
    withSpace(`/v1/docs/hub?${qs}`, spaceId),
    api,
    init,
  );
}

export function createNativeDoc(
  api: AppApi | null | undefined,
  payload: Models['CreateItemRequest'],
  spaceId?: string | null,
): Promise<DocsHubItem> {
  return request<Models['HubItem']>(withSpace('/v1/docs/items', spaceId), api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function getDocsItem(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsHubItem> {
  return request<Models['HubItem']>(`/v1/docs/items/${segment(itemId)}`, api);
}

export function updateDocsItem(
  api: AppApi | null | undefined,
  itemId: string,
  payload: Models['UpdateItemRequest'],
): Promise<DocsHubItem> {
  return request<Models['HubItem']>(`/v1/docs/items/${segment(itemId)}`, api, {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
}

export function deleteDocsItem(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<void> {
  return request<void>(`/v1/docs/items/${segment(itemId)}`, api, {
    method: 'DELETE',
  });
}

export function duplicateDocsItem(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsHubItem> {
  return request<Models['HubItem']>(
    `/v1/docs/items/${segment(itemId)}/duplicate`,
    api,
    { method: 'POST' },
  );
}

export function listDocPages(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsPageListResponse> {
  return request<Models['PageListResponse']>(
    `/v1/docs/items/${segment(itemId)}/pages?metadata_only=true`,
    api,
  );
}

export function getDocPage(
  api: AppApi | null | undefined,
  pageId: string,
  init?: RequestInit,
): Promise<DocsPageItem> {
  return request<Models['PageItem']>(`/v1/docs/pages/${segment(pageId)}`, api, init);
}

export function createDocPage(
  api: AppApi | null | undefined,
  itemId: string,
  payload: Models['CreatePageRequest'],
): Promise<DocsPageItem> {
  return request<Models['PageItem']>(
    `/v1/docs/items/${segment(itemId)}/pages`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
}

export function updateDocPage(
  api: AppApi | null | undefined,
  pageId: string,
  payload: Models['UpdatePageRequest'],
): Promise<DocsPageItem> {
  return request<Models['PageItem']>(`/v1/docs/pages/${segment(pageId)}`, api, {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
}

export function makeDocsPageRef(
  sourceType: 'native_doc_page',
  sourcePageId: string,
): string {
  return `${sourceType}__${sourcePageId}`;
}

// ── Page attachments ────────────────────────────────────────────────
// Files kept beside a page. Reading a page is enough to download them;
// editing it is required to add or remove one.

export function listDocPageAttachments(
  api: AppApi | null | undefined,
  pageId: string,
): Promise<DocsPageAttachmentListResponse> {
  return request<Models['AttachmentListResponse']>(
    `/v1/docs/pages/${segment(pageId)}/attachments`,
    api,
  );
}

/** miy: one attachment per page, 10 MiB (platform small files). */
export const DOCS_ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;

export function attachmentContentPath(attachmentId: string): string {
  return attachmentContentRoute.replace('{attachment_id}', segment(attachmentId)).slice('/files'.length);
}

/** Network failures, 409 and 5xx may be repeated with the same request ID. */
export function canRetryAttachmentUpload(error: unknown): boolean {
  if (error instanceof DocsApiError) {
    return error.status === 0 || error.status === 409 || error.status >= 500;
  }
  return error instanceof TypeError || error instanceof SyntaxError;
}

async function recoverAttachmentStep<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    if (!canRetryAttachmentUpload(error)) throw error;
    return run();
  }
}

/**
 * miy: AI-DO posted the file as multipart form data. Here the server first
 * records the attachment (`POST .../attachments` with name, type, size and a
 * request ID), then the bytes go through the platform's bounded file gateway
 * (`runtime.fileApi`, `PUT /docs/attachments/<id>/content`), which answers
 * the finished attachment. An uncertain answer is retried with the same
 * request ID; a failed upload is never deleted automatically.
 */
export async function uploadDocPageAttachment(
  api: AppApi | null | undefined,
  fileApi: AppFileApi,
  pageId: string,
  file: File,
  reservationId: string = requestId(),
): Promise<DocsPageAttachment> {
  const input = {
    request_id: reservationId,
    filename: file.name,
    content_type: file.type || 'application/octet-stream',
    size_bytes: file.size,
  } satisfies Models['ReserveRequest'];
  const reserved = await recoverAttachmentStep(() =>
    request<Models['AttachmentItem']>(
      `/v1/docs/pages/${segment(pageId)}/attachments`,
      api,
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
    ),
  );
  if (reserved.status === 'attached') return reserved;
  return recoverAttachmentStep(async () => {
    const init: RequestInit = {
      method: 'PUT',
      body: file,
      headers: { 'Content-Type': 'application/octet-stream' },
    };
    try {
      const response = await checkedResponse(
        await fileApi(attachmentContentPath(reserved.id), init),
        init,
      );
      return (await response.json()) as AttachmentUpload['responses'][200]['content']['application/json'];
    } catch (error) {
      const mapped = toApiRequestError(error);
      if (mapped instanceof ApiRequestError) {
        throw new DocsApiError(
          mapped.status,
          mapped.message,
          errorCode(mapped),
        );
      }
      throw mapped;
    }
  });
}

export function deleteDocPageAttachment(
  api: AppApi | null | undefined,
  attachmentId: string,
): Promise<void> {
  return request<void>(`/v1/docs/attachments/${segment(attachmentId)}`, api, {
    method: 'DELETE',
  });
}

/**
 * Fetch the file itself through the file gateway. It reaches the user as a
 * blob rather than a URL the browser could open on its own.
 */
export async function fetchDocPageAttachmentBlob(
  fileApi: AppFileApi,
  attachmentId: string,
): Promise<Blob> {
  try {
    const response = await checkedResponse(
      await fileApi(attachmentContentPath(attachmentId)),
    );
    return await response.blob();
  } catch (error) {
    const mapped = toApiRequestError(error);
    if (mapped instanceof ApiRequestError) {
      throw new DocsApiError(mapped.status, mapped.message, errorCode(mapped));
    }
    throw mapped;
  }
}

export function updateDocTarget(
  api: AppApi | null | undefined,
  itemId: string,
  payload: Models['TargetPayload'],
): Promise<DocsHubItem> {
  return request<Models['HubItem']>(`/v1/docs/items/${segment(itemId)}/target`, api, {
    method: 'PUT',
    body: JSON.stringify(payload),
  });
}

export function deleteDocTarget(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsHubItem> {
  return request<Models['HubItem']>(`/v1/docs/items/${segment(itemId)}/target`, api, {
    method: 'DELETE',
  });
}

export function deleteDocPage(
  api: AppApi | null | undefined,
  pageId: string,
): Promise<void> {
  return request<void>(`/v1/docs/pages/${segment(pageId)}`, api, {
    method: 'DELETE',
  });
}

export function toggleDocFavorite(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<Models['FavoriteResponse']> {
  return request<Models['FavoriteResponse']>(
    `/v1/docs/items/${segment(itemId)}/favorite`,
    api,
    { method: 'PATCH' },
  );
}

export function recordDocView(
  api: AppApi | null | undefined,
  itemId: string,
  pageId?: string | null,
): Promise<void> {
  return request<void>(`/v1/docs/items/${segment(itemId)}/view`, api, {
    method: 'POST',
    body: JSON.stringify({ page_id: pageId ?? null } satisfies Models['RecordViewRequest']),
  });
}

/**
 * `space` keeps the space the request is bound to; `docs_spaces` spans every
 * Docs space the user can enter and reports each item's space.
 */
export type DocsPrefScope = 'space' | 'docs_spaces';

export function listFavoriteDocs(
  api: AppApi | null | undefined,
  spaceId?: string | null,
  scope: DocsPrefScope = 'space',
): Promise<FavoriteDocItem[]> {
  const query = scope === 'space' ? '' : `?scope=${scope}`;
  return request<Models['FavoriteDoc'][]>(
    withSpace(`/v1/docs/favorites${query}`, scope === 'space' ? spaceId : null),
    api,
  );
}

export function listRecentPages(
  api: AppApi | null | undefined,
  limit = 10,
  spaceId?: string | null,
  scope: DocsPrefScope = 'space',
): Promise<RecentPageItem[]> {
  const query = scope === 'space' ? '' : `&scope=${scope}`;
  return request<Models['RecentPage'][]>(
    withSpace(
      `/v1/docs/recent-pages?limit=${limit}${query}`,
      scope === 'space' ? spaceId : null,
    ),
    api,
  );
}

export function listShareableUsers(
  api: AppApi | null | undefined,
  q?: string,
  init: RequestInit = {},
): Promise<ShareableUserItem[]> {
  const suffix = q ? `?q=${encodeURIComponent(q)}` : '';
  return request<ShareableUserItem[]>(
    `/v1/docs/shareable-users${suffix}`,
    api,
    init,
  );
}

export function getDocSharing(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<NativeDocSharingResponse> {
  return request<NativeDocSharingResponse>(
    `/v1/docs/items/${segment(itemId)}/sharing`,
    api,
  );
}

export function upsertDocUserShare(
  api: AppApi | null | undefined,
  itemId: string,
  userId: string,
  accessLevel: 'read' | 'edit',
): Promise<NativeDocSharingResponse> {
  return request<NativeDocSharingResponse>(
    `/v1/docs/items/${segment(itemId)}/sharing/users/${segment(userId)}`,
    api,
    {
      method: 'PUT',
      body: JSON.stringify({ access_level: accessLevel } satisfies Models['UpsertShareRequest']),
    },
  );
}

export function deleteDocUserShare(
  api: AppApi | null | undefined,
  itemId: string,
  userId: string,
): Promise<NativeDocSharingResponse> {
  return request<NativeDocSharingResponse>(
    `/v1/docs/items/${segment(itemId)}/sharing/users/${segment(userId)}`,
    api,
    { method: 'DELETE' },
  );
}

// ── 보강: trash, version history, comments, templates (miy) ────────────

export type DocsTrashEntry = Models['TrashEntry'];

export function listDocsTrash(
  api: AppApi | null | undefined,
  spaceId?: string | null,
): Promise<Models['TrashList']> {
  return request(withSpace('/v1/docs/trash', spaceId), api);
}

function trashPath(entry: Pick<DocsTrashEntry, 'kind' | 'id'>): string {
  return `/v1/docs/trash/${entry.kind === 'doc' ? 'items' : 'pages'}/${segment(entry.id)}`;
}

export function restoreDocsTrashEntry(
  api: AppApi | null | undefined,
  entry: Pick<DocsTrashEntry, 'kind' | 'id'>,
): Promise<Models['HubItem'] | Models['RestoredPage']> {
  return request(`${trashPath(entry)}/restore`, api, { method: 'POST' });
}

export function purgeDocsTrashEntry(
  api: AppApi | null | undefined,
  entry: Pick<DocsTrashEntry, 'kind' | 'id'>,
): Promise<void> {
  return request<void>(trashPath(entry), api, { method: 'DELETE' });
}

export type DocsPageVersion = Models['PageVersion'] & Partial<Pick<Models['PageVersionDetail'], 'content_blocks'>>;

export function listDocPageVersions(
  api: AppApi | null | undefined,
  pageId: string,
): Promise<Models['PageVersionList']> {
  return request(`/v1/docs/pages/${segment(pageId)}/versions`, api);
}

export function getDocPageVersion(
  api: AppApi | null | undefined,
  pageId: string,
  versionId: string,
): Promise<DocsPageVersion> {
  return request(
    `/v1/docs/pages/${segment(pageId)}/versions/${segment(versionId)}`,
    api,
  );
}

export function restoreDocPageVersion(
  api: AppApi | null | undefined,
  pageId: string,
  versionId: string,
  expectedRevision: number,
): Promise<DocsPageItem> {
  return request(
    `/v1/docs/pages/${segment(pageId)}/versions/${segment(versionId)}/restore`,
    api,
    {
      method: 'POST',
      body: JSON.stringify({ expected_revision: expectedRevision } satisfies Models['RestoreRequest']),
    },
  );
}

export type DocsCommentMention = Models['CommentMention'];

export type DocsComment = Models['CommentItem'] & { replies?: DocsComment[] };

export function listDocPageComments(
  api: AppApi | null | undefined,
  pageId: string,
  init: RequestInit = {},
): Promise<{ items: DocsComment[]; can_comment: boolean }> {
  return request<Models['CommentListResponse']>(`/v1/docs/pages/${segment(pageId)}/comments`, api, init);
}

export function createDocPageComment(
  api: AppApi | null | undefined,
  pageId: string,
  payload: Models['CommentCreateRequest'],
): Promise<Models['CommentCreated']> {
  return request(`/v1/docs/pages/${segment(pageId)}/comments`, api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function updateDocPageComment(
  api: AppApi | null | undefined,
  commentId: string,
  payload: Models['CommentUpdateRequest'],
): Promise<void> {
  return request<void>(`/v1/docs/comments/${segment(commentId)}`, api, {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
}

export function deleteDocPageComment(
  api: AppApi | null | undefined,
  commentId: string,
): Promise<void> {
  return request<void>(`/v1/docs/comments/${segment(commentId)}`, api, {
    method: 'DELETE',
  });
}

export function setDocPageCommentResolved(
  api: AppApi | null | undefined,
  commentId: string,
  resolved: boolean,
): Promise<void> {
  return request<void>(
    `/v1/docs/comments/${segment(commentId)}/${resolved ? 'resolve' : 'reopen'}`,
    api,
    { method: 'POST' },
  );
}

export type DocsTemplate = Models['TemplateItem'];

export function listDocsTemplates(
  api: AppApi | null | undefined,
  spaceId?: string | null,
): Promise<Models['TemplateList']> {
  return request(withSpace('/v1/docs/templates', spaceId), api);
}

export function createDocsTemplate(
  api: AppApi | null | undefined,
  payload: Models['TemplateCreateRequest'],
): Promise<DocsTemplate> {
  return request('/v1/docs/templates', api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function deleteDocsTemplate(
  api: AppApi | null | undefined,
  templateId: string,
): Promise<void> {
  return request<void>(`/v1/docs/templates/${segment(templateId)}`, api, {
    method: 'DELETE',
  });
}
