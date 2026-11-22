import { apiFetchJsonWithMappedError } from '../../../platform/api/client';
import type { AppApi } from '../../../platform/runtime/runtime-context';

/**
 * AI-DO app-modules/announcements/api/announcements-api.ts. AI-DO sent the
 * session token and rewrote the path into the current workspace
 * (`rewriteWorkspaceApiPath`). miy calls `runtime.api` (`api`) and puts the
 * board space in the path: `/v1/spaces/<id>/announcements`; company notices
 * are also reachable without a space (`/v1/announcements`).
 */
export type AnnouncementScope = 'workspace' | 'company';

/** AI-DO `ApiSchema<'AnnouncementOut'>`; workspaceId is the board space (null for company notices). */
interface AnnouncementOut {
  id: string;
  workspaceId: string | null;
  authorId: string;
  authorName: string;
  scope: AnnouncementScope;
  title: string;
  body: string;
  isPinned: boolean;
  canManage?: boolean;
  createdAt: string;
  updatedAt: string;
}

export type Announcement = Omit<AnnouncementOut, 'canManage'> & {
  canManage: boolean;
};

function toAnnouncement(value: AnnouncementOut): Announcement {
  return {
    id: value.id,
    workspaceId: value.workspaceId,
    authorId: value.authorId,
    authorName: value.authorName,
    scope: value.scope,
    title: value.title,
    body: value.body,
    isPinned: value.isPinned,
    createdAt: value.createdAt,
    updatedAt: value.updatedAt,
    canManage: 'canManage' in value && value.canManage === true,
  };
}

export interface AnnouncementsResponse {
  items: Announcement[];
  hasMore: boolean;
}

export interface AnnouncementCreateInput {
  title: string;
  body?: string;
  scope?: AnnouncementScope;
  isPinned?: boolean;
}

export interface AnnouncementUpdateInput {
  title?: string;
  body?: string;
  isPinned?: boolean;
}

export class AnnouncementApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = 'AnnouncementApiError';
  }
}

function announcementsPath(spaceId: string | null | undefined): string {
  return spaceId
    ? `/v1/spaces/${encodeURIComponent(spaceId)}/announcements`
    : '/v1/announcements';
}

async function request<T>(
  path: string,
  api: AppApi | null | undefined,
  init: RequestInit = {},
): Promise<T> {
  return apiFetchJsonWithMappedError<T>(
    path,
    api,
    init,
    (error) =>
      new AnnouncementApiError(
        error.status,
        error.message || `Announcement request failed with ${error.status}.`,
      ),
  );
}

export async function listAnnouncements(
  api: AppApi | null | undefined,
  spaceId: string | null | undefined,
  options: { scope?: AnnouncementScope; limit?: number; offset?: number } = {},
): Promise<AnnouncementsResponse> {
  const params = new URLSearchParams();
  if (options.scope) params.set('scope', options.scope);
  if (options.limit) params.set('limit', String(options.limit));
  if (options.offset) params.set('offset', String(options.offset));
  const query = params.toString();
  const response = await request<{ items: AnnouncementOut[]; hasMore: boolean }>(
    `${announcementsPath(spaceId)}${query ? `?${query}` : ''}`,
    api,
  );
  return { items: response.items.map(toAnnouncement), hasMore: response.hasMore === true };
}

/** miy: one notice, for a notification link (`/notices/<id>`). */
export async function getAnnouncement(
  api: AppApi | null | undefined,
  spaceId: string | null | undefined,
  announcementId: string,
): Promise<Announcement> {
  const response = await request<AnnouncementOut>(
    `${announcementsPath(spaceId)}/${encodeURIComponent(announcementId)}`,
    api,
  );
  return toAnnouncement(response);
}

export async function createAnnouncement(
  api: AppApi | null | undefined,
  spaceId: string | null | undefined,
  payload: AnnouncementCreateInput,
): Promise<Announcement> {
  const response = await request<AnnouncementOut>(
    announcementsPath(spaceId),
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
  return toAnnouncement(response);
}

export async function updateAnnouncement(
  api: AppApi | null | undefined,
  spaceId: string | null | undefined,
  announcementId: string,
  payload: AnnouncementUpdateInput,
): Promise<Announcement> {
  const response = await request<AnnouncementOut>(
    `${announcementsPath(spaceId)}/${encodeURIComponent(announcementId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
  );
  return toAnnouncement(response);
}

export function deleteAnnouncement(
  api: AppApi | null | undefined,
  spaceId: string | null | undefined,
  announcementId: string,
): Promise<void> {
  return request<void>(
    `${announcementsPath(spaceId)}/${encodeURIComponent(announcementId)}`,
    api,
    { method: 'DELETE' },
  );
}
