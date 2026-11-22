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

export const DOCS_PAGE_REVISION_CONFLICT = 'docs_page_revision_conflict';

export function isDocsRevisionConflict(error: unknown): boolean {
  return (
    error instanceof DocsApiError &&
    error.status === 409 &&
    error.code === DOCS_PAGE_REVISION_CONFLICT
  );
}

export type DocsDocType =
  | 'general'
  | 'meeting_notes'
  | 'project_brief'
  | 'spec'
  | 'policy'
  | 'guide'
  | 'memo';
export type DocsContentFormat = 'block' | 'html';
export type DocsHubContentFormat = 'block' | 'html' | 'mixed';

export interface DocsShareSummary {
  visibility: 'private' | 'shared';
  user_share_count: number;
  link_active: boolean;
  link_access_level?: 'read' | 'edit' | null;
}

export interface DocsPrimaryTarget {
  app: string;
  type: string;
  id: string;
  sort_order: number;
}

export interface DocsCollectionSummary {
  id: string;
  name: string;
  scope: 'workspace' | 'private';
}

export interface DocsHubItem {
  id: string;
  /** miy: the docs space holding the doc. */
  space_id: string;
  source_app: string;
  source_type: 'native_doc';
  source_id: string;
  source_kind: string;
  source_ref: string | null;
  generation_kind: string;
  doc_type: DocsDocType;
  content_format: DocsHubContentFormat;
  structure_kind: 'page_tree';
  location_label: string;
  target_label: string;
  collection: DocsCollectionSummary | null;
  primary_target: DocsPrimaryTarget | null;
  source_badge: string;
  source_deeplink: string | null;
  title: string;
  page_count: number;
  created_by_id: string;
  created_by_name: string;
  created_at: string;
  updated_at: string;
  trashed_at: string | null;
  is_favorite: boolean;
  is_private: boolean;
  last_viewed_at: string | null;
  can_view: boolean;
  can_edit: boolean;
  can_share: boolean;
  can_manage: boolean;
  sharing_summary: DocsShareSummary | null;
}

export interface DocsHubResponse {
  items: DocsHubItem[];
  total: number;
  page: number;
  page_size: number;
}

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

export interface DocsPageItem {
  id: string;
  doc_id: string;
  source_type: 'native_doc_page';
  source_page_id: string;
  parent_id: string | null;
  title: string;
  content_blocks: Record<string, unknown>[] | null;
  content_text: string | null;
  content_format: DocsContentFormat;
  sort_order: number;
  created_by_id: string;
  created_by_name: string;
  created_at: string;
  updated_at: string;
  trashed_at: string | null;
  can_edit: boolean;
  realtime_collab: boolean;
  /** miy: the content revision (single-editor saving). */
  revision: number;
}

export interface DocsPageListResponse {
  items: DocsPageItem[];
}

export interface DocsPageAttachment {
  id: string;
  page_id: string;
  filename: string;
  content_type: string;
  size_bytes: number;
  uploaded_by_id: string;
  uploaded_by_name: string;
  created_at: string;
  /** miy: `pending` until the bytes are stored, then `attached`. */
  status?: string;
}

export interface DocsPageAttachmentListResponse {
  items: DocsPageAttachment[];
}

export function mediaResourceTypeForDocsPage(): 'docs_native_page' {
  return 'docs_native_page';
}

/** The Docs-owned space holding a doc, for links into another space. */
export interface DocsSpaceLocation {
  key: string;
  name: string;
  kind: 'app_global' | 'app_space';
}

export interface FavoriteDocItem {
  id: string;
  title: string;
  source_type: 'native_doc';
  space?: DocsSpaceLocation | null;
}

export interface RecentPageItem {
  page_id: string;
  page_title: string;
  doc_id: string;
  doc_title: string;
  source_type: 'native_doc';
  location_label: string;
  last_viewed_at: string;
  space?: DocsSpaceLocation | null;
}

export interface ShareableUserItem {
  id: string;
  email: string;
  full_name: string;
}

export interface NativeUserShareItem {
  user_id: string;
  email: string;
  full_name: string;
  access_level: 'read' | 'edit';
}

export interface NativeDocSharingResponse {
  doc_id: string;
  owner_id: string;
  users: NativeUserShareItem[];
  link_share: null;
}

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
  params: {
    view?: string;
    q?: string;
    sort_by?: string;
    sort_dir?: string;
    page?: number;
    page_size?: number;
    source_app?: string;
    source_kind?: string;
    doc_type?: DocsDocType;
    space_id?: string;
  } = {},
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
  return request<DocsHubResponse>(
    withSpace(`/v1/docs/hub?${qs}`, spaceId),
    api,
    init,
  );
}

export function createNativeDoc(
  api: AppApi | null | undefined,
  payload: {
    title: string;
    first_page_title?: string;
    source_app?: string;
    source_kind?: string;
    source_ref?: string | null;
    generation_kind?: string;
    doc_type?: DocsDocType;
    content_format?: DocsContentFormat;
    first_page_content_text?: string | null;
    primary_target?: {
      app: string;
      type: string;
      id: string;
      sort_order?: number;
    } | null;
  },
  spaceId?: string | null,
): Promise<DocsHubItem> {
  return request<DocsHubItem>(withSpace('/v1/docs/items', spaceId), api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function getDocsItem(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsHubItem> {
  return request<DocsHubItem>(`/v1/docs/items/${segment(itemId)}`, api);
}

export function updateDocsItem(
  api: AppApi | null | undefined,
  itemId: string,
  payload: {
    title?: string;
    doc_type?: DocsDocType;
  },
): Promise<DocsHubItem> {
  return request<DocsHubItem>(`/v1/docs/items/${segment(itemId)}`, api, {
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
  return request<DocsHubItem>(
    `/v1/docs/items/${segment(itemId)}/duplicate`,
    api,
    { method: 'POST' },
  );
}

export function listDocPages(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsPageListResponse> {
  return request<DocsPageListResponse>(
    `/v1/docs/items/${segment(itemId)}/pages`,
    api,
  );
}

export function getDocPage(
  api: AppApi | null | undefined,
  pageId: string,
): Promise<DocsPageItem> {
  return request<DocsPageItem>(`/v1/docs/pages/${segment(pageId)}`, api);
}

export function createDocPage(
  api: AppApi | null | undefined,
  itemId: string,
  payload: {
    title: string;
    parent_id?: string | null;
    content_format?: DocsContentFormat;
    content_blocks?: Record<string, unknown>[] | null;
    sort_order?: number | null;
  },
): Promise<DocsPageItem> {
  return request<DocsPageItem>(
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
  payload: {
    title?: string;
    parent_id?: string | null;
    content_blocks?: Record<string, unknown>[] | null;
    sort_order?: number | null;
    expected_revision?: number;
  },
): Promise<DocsPageItem> {
  return request<DocsPageItem>(`/v1/docs/pages/${segment(pageId)}`, api, {
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
  return request<DocsPageAttachmentListResponse>(
    `/v1/docs/pages/${segment(pageId)}/attachments`,
    api,
  );
}

/** miy: one attachment per page, 10 MiB (platform small files). */
export const DOCS_ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;

export function attachmentContentPath(attachmentId: string): string {
  return `/docs/attachments/${segment(attachmentId)}/content`;
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
  const reserved = await recoverAttachmentStep(() =>
    request<DocsPageAttachment>(
      `/v1/docs/pages/${segment(pageId)}/attachments`,
      api,
      {
        method: 'POST',
        body: JSON.stringify({
          request_id: reservationId,
          filename: file.name,
          content_type: file.type || 'application/octet-stream',
          size_bytes: file.size,
        }),
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
      return (await response.json()) as DocsPageAttachment;
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
  payload: {
    app: string;
    type: string;
    id: string;
    sort_order?: number;
  },
): Promise<DocsHubItem> {
  return request<DocsHubItem>(`/v1/docs/items/${segment(itemId)}/target`, api, {
    method: 'PUT',
    body: JSON.stringify(payload),
  });
}

export function deleteDocTarget(
  api: AppApi | null | undefined,
  itemId: string,
): Promise<DocsHubItem> {
  return request<DocsHubItem>(`/v1/docs/items/${segment(itemId)}/target`, api, {
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
): Promise<{ is_favorite: boolean }> {
  return request<{ is_favorite: boolean }>(
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
    body: JSON.stringify({ page_id: pageId ?? null }),
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
  return request<FavoriteDocItem[]>(
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
  return request<RecentPageItem[]>(
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
      body: JSON.stringify({ access_level: accessLevel }),
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
