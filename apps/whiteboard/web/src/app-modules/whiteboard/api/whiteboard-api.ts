import type { AppRuntime } from '@miy/sdk';

import type { components } from '../../../api.generated';

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

type Schemas = components['schemas'];

export type WhiteboardPrimaryTarget = Schemas['PrimaryTarget'];
export type WhiteboardTargetItem = Schemas['TargetItem'];
export type WhiteboardHubItem = Schemas['HubItem'];
export type WhiteboardDetail = Schemas['Detail'];
export type WhiteboardHubResponse = Schemas['HubResponse'];
export type ShareableUserItem = Schemas['ShareableUser'];
export type WhiteboardUserShareItem = Schemas['UserShareItem'];
export type WhiteboardSharingResponse = Schemas['SharingResponse'];

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
  payload: Schemas['CreateRequest'],
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
  payload: Schemas['UpdateRequest'],
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
  payload: Schemas['TargetPayload'],
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
): Promise<Schemas['FavoriteResponse']> {
  return request<Schemas['FavoriteResponse']>(
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
  accessLevel: Schemas['UserShareRequest']['access_level'],
  spaceId?: string | null,
): Promise<WhiteboardSharingResponse> {
  const payload: Schemas['UserShareRequest'] = { access_level: accessLevel };
  return request<WhiteboardSharingResponse>(
    whiteboardApiRoutes.userShare(itemId, userId, spaceId),
    api,
    {
      method: 'PUT',
      body: JSON.stringify(payload),
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

export type WhiteboardStoredFile = Schemas['FileResponse'];

/** Reserve (or, with the same request ID, look up) a stored image. */
export function reserveWhiteboardFile(
  api: AppApi | null | undefined,
  itemId: string,
  payload: Schemas['ReserveRequest'],
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
