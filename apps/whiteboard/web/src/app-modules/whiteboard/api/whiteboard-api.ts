import type { AppRuntime } from '@miy/sdk';

import { ApiRequestError, apiFetchJson } from '../../../platform/api/client';
import type { AppApi } from '../../../platform/runtime/runtime-context';

import {
  normalizeWhiteboardScene as normalizeWhiteboardSceneValue,
  type WhiteboardScene,
} from './whiteboard-scene-codec';
import {
  whiteboardApiRoutes,
  type WhiteboardHubRouteParams,
} from './whiteboard-routes';

/**
 * AI-DO app-modules/whiteboard/api/whiteboard-api.ts. Every call takes the
 * app's own API client (`runtime.api`) and the current space instead of the
 * bearer token and workspace slug. Removed with their features: the
 * collaboration session and snapshot, link shares and shared-link routes,
 * and the context slots other apps embedded. Added (miy): `base_revision` on
 * scene saves and the stored-image calls.
 */
export type { WhiteboardScene } from './whiteboard-scene-codec';
export type WhiteboardVisibility = 'personal' | 'workspace';
export type FileApi = AppRuntime['fileApi'];

export class WhiteboardApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    public readonly code: string | null = null,
  ) {
    super(message);
  }
}

async function request<T>(
  path: string,
  api: AppApi | null | undefined,
  init: RequestInit = {},
): Promise<T> {
  try {
    return await apiFetchJson<T>(path, api, init);
  } catch (error) {
    if (error instanceof ApiRequestError) {
      const payload = error.payload as { code?: unknown } | null;
      throw new WhiteboardApiError(
        error.status,
        error.message,
        typeof payload?.code === 'string' ? payload.code : null,
      );
    }
    throw error;
  }
}

export interface WhiteboardPrimaryTarget {
  app: string;
  type: string;
  id: string;
  sort_order: number;
}

export interface WhiteboardTargetItem extends WhiteboardPrimaryTarget {
  is_primary: boolean;
}

export interface WhiteboardHubItem {
  id: string;
  workspace_id: string | null;
  workspace_key: string | null;
  source_app: string;
  source_type: 'whiteboard';
  source_id: string;
  source_kind: string;
  source_ref: string | null;
  generation_kind: string;
  location_label: string;
  target_label: string;
  primary_target: WhiteboardPrimaryTarget | null;
  targets: WhiteboardTargetItem[];
  source_badge: string;
  source_deeplink: string | null;
  title: string;
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
  /** miy: the stored scene's revision (single-editor saving). */
  revision: number;
}

export type WhiteboardDetail = WhiteboardHubItem & {
  scene: WhiteboardScene;
};

export interface WhiteboardHubResponse {
  items: WhiteboardHubItem[];
  total: number;
  page: number;
  page_size: number;
}

export interface ShareableUserItem {
  id: string;
  email: string;
  full_name: string;
  /** miy: the directory's group name (it exposes no e-mail). */
  primary_group?: string | null;
}

export interface WhiteboardUserShareItem {
  user_id: string;
  email: string;
  full_name: string;
  access_level: 'read' | 'edit';
}

export interface WhiteboardSharingResponse {
  whiteboard_id: string;
  owner_id: string;
  users: WhiteboardUserShareItem[];
  link_share: null;
}

export function normalizeWhiteboardScene(value: unknown): WhiteboardScene {
  return normalizeWhiteboardSceneValue(value);
}

export function listWhiteboardHub(
  api: AppApi | null | undefined,
  params: WhiteboardHubRouteParams = {},
  spaceId?: string | null,
): Promise<WhiteboardHubResponse> {
  return request<WhiteboardHubResponse>(
    whiteboardApiRoutes.hub(params, spaceId),
    api,
  );
}

export function createWhiteboard(
  api: AppApi | null | undefined,
  payload: {
    title: string;
    scene?: WhiteboardScene | null;
    source_app?: string;
    source_kind?: string;
    source_ref?: string | null;
    generation_kind?: string;
    primary_target?: {
      app: string;
      type: string;
      id: string;
      sort_order?: number;
    } | null;
  },
  spaceId?: string | null,
): Promise<WhiteboardDetail> {
  return request<WhiteboardDetail>(whiteboardApiRoutes.items(spaceId), api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function getWhiteboard(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<WhiteboardDetail> {
  return request<WhiteboardDetail>(
    whiteboardApiRoutes.item(itemId, spaceId),
    api,
  );
}

/**
 * A title change, a scene save, or both. A scene save names the revision it
 * started from (`base_revision`); the server answers 409
 * `whiteboard_revision_conflict` when someone saved a newer scene.
 */
export function updateWhiteboard(
  api: AppApi | null | undefined,
  itemId: string,
  payload: {
    title?: string;
    scene?: WhiteboardScene | null;
    base_revision?: number;
  },
  spaceId?: string | null,
): Promise<WhiteboardDetail> {
  return request<WhiteboardDetail>(
    whiteboardApiRoutes.item(itemId, spaceId),
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
  );
}

export function deleteWhiteboard(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<void> {
  return request<void>(whiteboardApiRoutes.item(itemId, spaceId), api, {
    method: 'DELETE',
  });
}

export function restoreWhiteboard(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<WhiteboardDetail> {
  return request<WhiteboardDetail>(
    whiteboardApiRoutes.itemRestore(itemId, spaceId),
    api,
    { method: 'POST' },
  );
}

export function updateWhiteboardTarget(
  api: AppApi | null | undefined,
  itemId: string,
  payload: {
    app: string;
    type: string;
    id: string;
    sort_order?: number;
  },
  spaceId?: string | null,
): Promise<WhiteboardDetail> {
  return request<WhiteboardDetail>(
    whiteboardApiRoutes.itemTarget(itemId, spaceId),
    api,
    {
      method: 'PUT',
      body: JSON.stringify(payload),
    },
  );
}

export function deleteWhiteboardTarget(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<WhiteboardDetail> {
  return request<WhiteboardDetail>(
    whiteboardApiRoutes.itemTarget(itemId, spaceId),
    api,
    { method: 'DELETE' },
  );
}

export function permanentlyDeleteWhiteboard(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<void> {
  return request<void>(
    whiteboardApiRoutes.itemPermanent(itemId, spaceId),
    api,
    { method: 'DELETE' },
  );
}

export function toggleWhiteboardFavorite(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<{ is_favorite: boolean }> {
  return request<{ is_favorite: boolean }>(
    whiteboardApiRoutes.itemFavorite(itemId, spaceId),
    api,
    { method: 'PATCH' },
  );
}

export function recordWhiteboardView(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<void> {
  return request<void>(whiteboardApiRoutes.itemView(itemId, spaceId), api, {
    method: 'POST',
  });
}

export function listWhiteboardShareableUsers(
  api: AppApi | null | undefined,
  q = '',
  spaceId?: string | null,
): Promise<ShareableUserItem[]> {
  return request<ShareableUserItem[]>(
    whiteboardApiRoutes.shareableUsers(q, spaceId),
    api,
  );
}

export function getWhiteboardSharing(
  api: AppApi | null | undefined,
  itemId: string,
  spaceId?: string | null,
): Promise<WhiteboardSharingResponse> {
  return request<WhiteboardSharingResponse>(
    whiteboardApiRoutes.sharing(itemId, spaceId),
    api,
  );
}

export function upsertWhiteboardUserShare(
  api: AppApi | null | undefined,
  itemId: string,
  userId: string,
  accessLevel: 'read' | 'edit',
  spaceId?: string | null,
): Promise<WhiteboardSharingResponse> {
  return request<WhiteboardSharingResponse>(
    whiteboardApiRoutes.userShare(itemId, userId, spaceId),
    api,
    {
      method: 'PUT',
      body: JSON.stringify({ access_level: accessLevel }),
    },
  );
}

export function deleteWhiteboardUserShare(
  api: AppApi | null | undefined,
  itemId: string,
  userId: string,
  spaceId?: string | null,
): Promise<WhiteboardSharingResponse> {
  return request<WhiteboardSharingResponse>(
    whiteboardApiRoutes.userShare(itemId, userId, spaceId),
    api,
    { method: 'DELETE' },
  );
}

// ---------------------------------------------------------------- miy: stored images

export interface WhiteboardStoredFile {
  /** The storage row; the scene's file entry names it as `storage`. */
  id: string;
  /** Excalidraw's file ID. */
  file_id: string;
  status: 'pending' | 'ready' | 'attached';
}

/** Reserve (or, with the same request ID, look up) a stored image. */
export function reserveWhiteboardFile(
  api: AppApi | null | undefined,
  itemId: string,
  payload: {
    request_id: string;
    file_id: string;
    name: string;
    mime: string;
    size: number;
  },
  spaceId?: string | null,
): Promise<WhiteboardStoredFile> {
  return request<WhiteboardStoredFile>(
    whiteboardApiRoutes.itemFiles(itemId, spaceId),
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
}

/** Stream the reserved bytes (repeating after a lost answer is safe). */
export async function uploadWhiteboardFileContent(
  fileApi: FileApi,
  itemId: string,
  fileId: string,
  body: Blob,
): Promise<WhiteboardStoredFile> {
  const response = await fileApi(whiteboardApiRoutes.fileContent(itemId, fileId), {
    method: 'PUT',
    body,
    headers: { 'Content-Type': 'application/octet-stream' },
  });
  if (!response.ok) {
    throw new WhiteboardApiError(response.status, `upload failed (${response.status})`);
  }
  return (await response.json()) as WhiteboardStoredFile;
}

export async function downloadWhiteboardFile(
  fileApi: FileApi,
  itemId: string,
  fileId: string,
  signal?: AbortSignal,
): Promise<Blob> {
  const response = await fileApi(whiteboardApiRoutes.fileContent(itemId, fileId), {
    signal,
  });
  if (!response.ok) {
    throw new WhiteboardApiError(response.status, `download failed (${response.status})`);
  }
  return response.blob();
}
