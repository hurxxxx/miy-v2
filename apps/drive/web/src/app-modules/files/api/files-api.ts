import { ApiError, type AppRuntime } from '@miy/sdk';
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

export type FileVisibility = 'private' | 'workspace';
export type FileIngressBlockReason =
  | 'workspace_access_required'
  | 'parent_manage_required';
export type SpaceRole = 'viewer' | 'member' | 'admin';

export interface FileFolderItem {
  id: string;
  parent_id: string | null;
  name: string;
  visibility: FileVisibility;
  owner_id: string;
  owner_name: string;
  can_manage: boolean;
  created_at: string;
  updated_at: string;
}

export interface FileItem {
  id: string;
  folder_id: string | null;
  filename: string;
  content_type: string;
  size_bytes: number;
  visibility: FileVisibility;
  owner_id: string;
  owner_name: string;
  can_delete: boolean;
  /** miy: the current version and how many there are. */
  version_number: number;
  version_count: number;
  created_at: string;
  updated_at: string;
}

export interface FileSpaceSummary {
  id: string;
  name: string;
  role: SpaceRole;
  can_manage: boolean;
}

export interface FileBrowseResponse {
  space: FileSpaceSummary;
  current_folder: FileFolderItem | null;
  breadcrumbs: FileFolderItem[];
  folders: FileFolderItem[];
  files: FileItem[];
  all_folders: FileFolderItem[];
  can_ingest: boolean;
  ingest_block_reason: FileIngressBlockReason | null;
}

export interface FileDownloadResponse {
  url: string;
  expires_at?: string;
}

export type FileSearchStrategy = 'keyword' | 'semantic' | 'hybrid';

export interface FileSearchHighlight {
  start: number;
  end: number;
}

export interface FileSearchSnippet {
  text: string;
  highlights: FileSearchHighlight[];
}

export interface FileSearchHit {
  rank: number;
  file_id: string;
  filename: string;
  folder_id: string | null;
  content_type: string;
  size_bytes: number;
  score: number;
  methods: string[];
  snippet: FileSearchSnippet;
  updated_at: string;
  author?: string | null;
}

export interface FileSearchPayload {
  query: string;
  strategy: FileSearchStrategy;
  page: number;
  page_size: number;
}

export interface FileSearchResponse {
  query: string;
  strategy: FileSearchStrategy;
  page: number;
  page_size: number;
  hits: FileSearchHit[];
  has_more: boolean;
  max_ranked_results: number;
  latency_ms: number;
  trace_id: string | null;
}

export interface FileUploadProgress {
  loaded: number;
  total: number;
  percent: number;
}

export interface FileBulkPayload {
  fileIds: string[];
  folderIds: string[];
}

/** miy: AI-DO zipped a selection; drive plans it and downloads each file. */
export interface FileArchiveEntry {
  path: string;
  file_id: string | null;
  size_bytes: number;
}

export interface FileArchivePlan {
  entries: FileArchiveEntry[];
  file_count: number;
  total_size: number;
}

export interface FileVersionItem {
  id: string;
  version_number: number;
  filename: string;
  content_type: string;
  size_bytes: number;
  uploaded_by: string;
  uploader_name: string;
  sha256: string | null;
  is_current: boolean;
  created_at: string;
}

export interface FileVersionsResponse {
  file: FileItem;
  versions: FileVersionItem[];
}

export interface DriveSpace {
  id: string;
  name: string;
  role: SpaceRole;
  can_manage: boolean;
  can_upload: boolean;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface DriveSpacesResponse {
  user_id: string;
  spaces: DriveSpace[];
}

export interface TrashItem {
  kind: 'file' | 'folder';
  id: string;
  name: string;
  content_type: string | null;
  size_bytes: number;
  file_count: number;
  visibility: FileVisibility;
  owner_id: string;
  owner_name: string;
  trashed_by: string;
  trashed_by_name: string;
  original_path: string;
  deleted_at: string;
}

export interface TrashResponse {
  space: FileSpaceSummary;
  items: TrashItem[];
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

function toBulkRequest(payload: FileBulkPayload) {
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
  payload: {
    name: string;
    parent_id?: string | null;
    visibility: FileVisibility;
  },
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
  payload: {
    name?: string;
    visibility?: FileVisibility;
    parent_id?: string | null;
  },
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
  payload: { filename?: string; folder_id?: string | null },
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
  return request<void>(client, spacePath(spaceId, '/files/move'), {
    method: 'POST',
    body: JSON.stringify({
      ...toBulkRequest(payload),
      target_folder_id: payload.targetFolderId,
    }),
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
  return request<DriveSpace>(client, '/v1/spaces', {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
}

export function renameSpace(
  client: DriveClient,
  spaceId: string,
  name: string,
): Promise<DriveSpace> {
  return request<DriveSpace>(client, spacePath(spaceId), {
    method: 'PATCH',
    body: JSON.stringify({ name }),
  });
}

export function deleteSpace(
  client: DriveClient,
  spaceId: string,
): Promise<void> {
  return request<void>(client, spacePath(spaceId), { method: 'DELETE' });
}

/** The SDK ACL snapshot as the server sends it. */
interface ServerShareSnapshot {
  revision: number;
  entries: Array<{
    subject: { kind: 'user' | 'group'; id: string; name: string };
    role: string;
    protected: boolean;
  }>;
  roles: Array<{ id: string; label: string }>;
  defaultRole?: string | null;
  canManage: boolean;
}

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
        }),
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
    (error instanceof ApiError && error.kind === 'abort') ||
    (error instanceof DOMException && error.name === 'AbortError')
  );
}
