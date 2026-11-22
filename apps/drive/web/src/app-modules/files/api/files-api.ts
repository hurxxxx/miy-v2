import type { components, paths } from '../../../api.generated';
import { isApiError, type AppRuntime } from '@miy/sdk';
import type {
  DirectoryOption,
  DirectoryProvider,
  ShareProvider,
  ShareSnapshot as UiShareSnapshot,
} from '@miy/ui';

import {
  ApiRequestError,
  apiFetchJsonWithMappedError,
  toApiRequestError,
} from '../../../platform/api/client';

/**
 * AI-DO apps/web/src/app-modules/files/api/files-api.ts. AI-DO passed a
 * bearer `token` and the workspace slug; miy passes the frame runtime
 * (`runtime.api` for JSON, `runtime.fileApi` for bytes) and the drive space
 * ID, which replaces the workspace in the path
 * (`/v1/spaces/<id>/files/...`, reached as `/api/v1/...`). RAG corpus,
 * ingress-grant and chat calls are not ported (docs/PORTING.md §1).
 */
export type DriveClient = Pick<AppRuntime, 'api' | 'fileApi'>;

type Schemas = components['schemas'];

export type FileVisibility = Schemas['FileVisibility'];
export type FileIngressBlockReason = Schemas['IngestBlockReason'];
export type SpaceRole = Schemas['Role'];
export type FileFolderItem = Schemas['FileFolderItem'];
export type FileItem = Schemas['FileItem'];
export type FileSpaceSummary = Schemas['SpaceSummary'];
export type FileBrowseResponse = Schemas['FileBrowseResponse'];
export type FileDownloadResponse = Schemas['FileDownloadResponse'];
export type FileSearchStrategy = Schemas['FileSearchStrategy'];
export type FileSearchHighlight = Schemas['Highlight'];
export type FileSearchSnippet = Schemas['Snippet'];
export type FileSearchHit = Schemas['FileSearchHit'];
export type FileSearchPayload = Schemas['FileSearchRequest'];
export type FileSearchResponse = Schemas['FileSearchResponse'];
export type FileArchiveEntry = Schemas['ArchiveEntryOut'];
export type FileArchivePlan = Schemas['ArchivePlanResponse'];
export type FileVersionItem = Schemas['VersionOut'];
export type FileVersionsResponse = Schemas['VersionsResponse'];
export type TrashItem = Schemas['TrashItem'];
export type TrashResponse = Schemas['TrashResponse'];

type SpaceOperations = paths['/v1/spaces'];
export type DriveSpacesResponse = SpaceOperations['get']['responses'][200]['content']['application/json'];
export type DriveSpace = SpaceOperations['post']['responses'][201]['content']['application/json'];
type CreateSpaceInput = SpaceOperations['post']['requestBody']['content']['application/json'];

export interface FileUploadProgress {
  loaded: number;
  total: number;
  percent: number;
}

export interface FileBulkPayload {
  fileIds: string[];
  folderIds: string[];
}

export class FilesApiError extends Error {
  status: number;
  code: string | null;

  constructor(status: number, message: string, code: string | null = null) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

function errorCode(error: ApiRequestError): string | null {
  const payload = error.payload as { code?: unknown } | null;
  return typeof payload?.code === 'string' ? payload.code : null;
}

export function toFilesApiError(error: ApiRequestError): FilesApiError {
  return new FilesApiError(error.status, error.message, errorCode(error));
}

function request<T>(
  client: DriveClient,
  path: string,
  init: RequestInit = {},
): Promise<T> {
  return apiFetchJsonWithMappedError<T, FilesApiError>(
    path,
    client.api,
    init,
    toFilesApiError,
  );
}

function spacePath(spaceId: string, tail = ''): string {
  return `/v1/spaces/${encodeURIComponent(spaceId)}${tail}`;
}

function toBulkRequest(payload: FileBulkPayload): Schemas['FileBulkRequest'] {
  return {
    file_ids: payload.fileIds,
    folder_ids: payload.folderIds,
  };
}

export function browseFiles(
  client: DriveClient,
  spaceId: string,
  folderId?: string | null,
  options: { signal?: AbortSignal } = {},
): Promise<FileBrowseResponse> {
  const params = new URLSearchParams();
  if (folderId) {
    params.set('folder_id', folderId);
  }
  const query = params.toString();
  return request<FileBrowseResponse>(
    client,
    spacePath(spaceId, `/files${query ? `?${query}` : ''}`),
    { signal: options.signal },
  );
}

export function searchFiles(
  client: DriveClient,
  spaceId: string,
  payload: FileSearchPayload,
  options: { signal?: AbortSignal } = {},
): Promise<FileSearchResponse> {
  return request<FileSearchResponse>(
    client,
    spacePath(spaceId, '/files/search'),
    {
      method: 'POST',
      body: JSON.stringify(payload),
      signal: options.signal,
    },
  );
}

export function createFileFolder(
  client: DriveClient,
  spaceId: string,
  payload: Schemas['FolderCreateRequest'],
): Promise<FileFolderItem> {
  return request<FileFolderItem>(client, spacePath(spaceId, '/files/folders'), {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function updateFileFolder(
  client: DriveClient,
  spaceId: string,
  folderId: string,
  payload: Schemas['FolderUpdateRequest'],
): Promise<FileFolderItem> {
  return request<FileFolderItem>(
    client,
    spacePath(spaceId, `/files/folders/${encodeURIComponent(folderId)}`),
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
  );
}

export function deleteFileFolder(
  client: DriveClient,
  spaceId: string,
  folderId: string,
): Promise<void> {
  return request<void>(
    client,
    spacePath(spaceId, `/files/folders/${encodeURIComponent(folderId)}`),
    { method: 'DELETE' },
  );
}

export function bulkDeleteFileItems(
  client: DriveClient,
  spaceId: string,
  payload: FileBulkPayload,
): Promise<void> {
  return request<void>(client, spacePath(spaceId, '/files/bulk-delete'), {
    method: 'POST',
    body: JSON.stringify(toBulkRequest(payload)),
  });
}

/** AI-DO `downloadFileArchive` posted the selection and got a ZIP. */
export function planFileArchive(
  client: DriveClient,
  spaceId: string,
  payload: FileBulkPayload,
): Promise<FileArchivePlan> {
  return request<FileArchivePlan>(client, spacePath(spaceId, '/files/archive'), {
    method: 'POST',
    body: JSON.stringify(toBulkRequest(payload)),
  });
}

export function getFileDownloadUrl(
  client: DriveClient,
  spaceId: string,
  fileId: string,
): Promise<FileDownloadResponse> {
  return request<FileDownloadResponse>(
    client,
    spacePath(spaceId, `/files/${encodeURIComponent(fileId)}/download`),
    { cache: 'no-store' },
  );
}

export function getFileVersionDownloadUrl(
  client: DriveClient,
  spaceId: string,
  fileId: string,
  versionId: string,
): Promise<FileDownloadResponse> {
  return request<FileDownloadResponse>(
    client,
    spacePath(
      spaceId,
      `/files/${encodeURIComponent(fileId)}/versions/${encodeURIComponent(versionId)}/download`,
    ),
    { cache: 'no-store' },
  );
}

/**
 * AI-DO `getFilePreviewUrl` returned an inline URL. The frame has no public
 * file URL: the image bytes come through the file gateway into a Blob
 * (`fileApi('/items/<id>/preview')` reaches `/api/files/items/<id>/preview`).
 */
export async function getFilePreviewBlob(
  client: DriveClient,
  fileId: string,
  options: { signal?: AbortSignal } = {},
): Promise<Blob> {
  try {
    const response = await client.fileApi(
      `/items/${encodeURIComponent(fileId)}/preview`,
      { signal: options.signal },
    );
    return await response.blob();
  } catch (error) {
    throw mapTransportError(error);
  }
}

export function deleteDriveFile(
  client: DriveClient,
  spaceId: string,
  fileId: string,
): Promise<void> {
  return request<void>(
    client,
    spacePath(spaceId, `/files/${encodeURIComponent(fileId)}`),
    { method: 'DELETE' },
  );
}

/** miy (보강): rename or move one file. */
export function updateDriveFile(
  client: DriveClient,
  spaceId: string,
  fileId: string,
  payload: Schemas['FileUpdateRequest'],
): Promise<FileItem> {
  return request<FileItem>(
    client,
    spacePath(spaceId, `/files/${encodeURIComponent(fileId)}`),
    { method: 'PATCH', body: JSON.stringify(payload) },
  );
}

/** miy (보강): move a selection under a folder or the space root. */
export function moveFileItems(
  client: DriveClient,
  spaceId: string,
  payload: FileBulkPayload & { targetFolderId: string | null },
): Promise<void> {
  const input: Schemas['MoveRequest'] = {
    ...toBulkRequest(payload),
    target_folder_id: payload.targetFolderId,
  };
  return request<void>(client, spacePath(spaceId, '/files/move'), {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function listFileVersions(
  client: DriveClient,
  spaceId: string,
  fileId: string,
  options: { signal?: AbortSignal } = {},
): Promise<FileVersionsResponse> {
  return request<FileVersionsResponse>(
    client,
    spacePath(spaceId, `/files/${encodeURIComponent(fileId)}/versions`),
    { signal: options.signal },
  );
}

// ------------------------------------------------------------------ spaces

export function listSpaces(
  client: DriveClient,
  options: { signal?: AbortSignal } = {},
): Promise<DriveSpacesResponse> {
  return request<DriveSpacesResponse>(client, '/v1/spaces', {
    signal: options.signal,
  });
}

export function createSpace(
  client: DriveClient,
  name: string,
): Promise<DriveSpace> {
  const input: CreateSpaceInput = { name };
  return request<DriveSpace>(client, '/v1/spaces', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function renameSpace(
  client: DriveClient,
  spaceId: string,
  name: string,
): Promise<DriveSpace> {
  return request<DriveSpace>(client, spacePath(spaceId), {
    method: 'PATCH',
    body: JSON.stringify({ name } satisfies Schemas['SpaceRequest']),
  });
}

export function deleteSpace(
  client: DriveClient,
  spaceId: string,
): Promise<void> {
  return request<void>(client, spacePath(spaceId), { method: 'DELETE' });
}

/** The SDK ACL snapshot as the server sends it. */
type ServerShareSnapshot = Schemas['SharingSnapshot'];

function toUiSnapshot(snapshot: ServerShareSnapshot): UiShareSnapshot {
  return {
    revision: String(snapshot.revision),
    entries: snapshot.entries.map((entry) => ({
      subject: {
        kind: entry.subject.kind,
        id: entry.subject.id,
        name: entry.subject.name,
      },
      role: entry.role,
      locked: entry.protected,
    })),
    roles: snapshot.roles,
    defaultRole: snapshot.defaultRole ?? null,
    canManage: snapshot.canManage,
  };
}

/** ShareEditor provider for a space's members (SDK resource ACL). */
export function spaceShareProvider(
  api: AppRuntime['api'],
  spaceId: string,
): ShareProvider {
  const base = spacePath(spaceId, '/sharing');
  return {
    async load({ signal }) {
      return toUiSnapshot(await api<ServerShareSnapshot>(base, { signal }));
    },
    async replace({ requestId, expectedRevision, entries }, { signal }) {
      const snapshot = await api<ServerShareSnapshot>(base, {
        method: 'PUT',
        signal,
        body: JSON.stringify({
          expectedRevision: Number(expectedRevision),
          requestId,
          entries: entries.map((entry) => ({
            subject: { kind: entry.subject.kind, id: entry.subject.id },
            role: entry.role,
          })),
        } satisfies Schemas['SharingRequest']),
      });
      return toUiSnapshot(snapshot);
    },
    async lookupResult(requestId, { signal }) {
      const snapshot = await api<ServerShareSnapshot | null>(
        `${base}/requests/${encodeURIComponent(requestId)}`,
        { signal },
      );
      return snapshot ? toUiSnapshot(snapshot) : null;
    },
  };
}

/** Name-based people and group selection for the share editor. */
export function directoryProvider(
  directory: AppRuntime['directory'],
): DirectoryProvider {
  return {
    async load({ kind, query, cursor, limit, signal }) {
      const page = await directory(
        {
          kind: kind === 'user' ? 'users' : 'groups',
          q: query || undefined,
          limit,
          cursor,
        },
        { signal },
      );
      const items: DirectoryOption[] =
        kind === 'user'
          ? page.users.map((user) => ({
              kind: 'user',
              id: user.id,
              name: user.name,
              description: user.primaryGroup?.name,
            }))
          : page.groups.map((group) => ({
              kind: 'group',
              id: group.id,
              name: group.name,
            }));
      return { items, nextCursor: page.nextCursor ?? null };
    },
  };
}

// ------------------------------------------------------------------- trash

export function listTrash(
  client: DriveClient,
  spaceId: string,
  options: { signal?: AbortSignal } = {},
): Promise<TrashResponse> {
  return request<TrashResponse>(client, spacePath(spaceId, '/trash'), {
    signal: options.signal,
  });
}

export function restoreTrashItems(
  client: DriveClient,
  spaceId: string,
  payload: FileBulkPayload,
): Promise<void> {
  return request<void>(client, spacePath(spaceId, '/trash/restore'), {
    method: 'POST',
    body: JSON.stringify(toBulkRequest(payload)),
  });
}

export function deleteTrashItemsForever(
  client: DriveClient,
  spaceId: string,
  payload: FileBulkPayload,
): Promise<void> {
  return request<void>(client, spacePath(spaceId, '/trash/delete'), {
    method: 'POST',
    body: JSON.stringify(toBulkRequest(payload)),
  });
}

export function emptyTrash(client: DriveClient, spaceId: string): Promise<void> {
  return request<void>(client, spacePath(spaceId, '/trash/empty'), {
    method: 'POST',
  });
}

// ------------------------------------------------------------------ errors

/** Map a transport failure to the AI-DO style error (abort stays abort). */
export function mapTransportError(error: unknown): unknown {
  const mapped = toApiRequestError(error);
  return mapped instanceof ApiRequestError ? toFilesApiError(mapped) : mapped;
}

export function isAbortError(error: unknown): boolean {
  return (
    (isApiError(error) && error.kind === 'abort') ||
    (error instanceof DOMException && error.name === 'AbortError')
  );
}
