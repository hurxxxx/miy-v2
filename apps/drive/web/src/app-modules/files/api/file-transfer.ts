import type { components } from '../../../api.generated';
import { ApiError, isApiError, readJson } from '@miy/sdk';
import { requestId as newRequestId } from '@miy/ui';

import {
  mapTransportError,
  type DriveClient,
  type FileItem,
  type FileUploadProgress,
  type FileVisibility,
} from './files-api';

/**
 * Large transfer (miy; replaces AI-DO's single multipart `POST /files/upload`
 * with XHR progress). Bytes go to platform Storage through the drive
 * backend, never as one JSON body:
 *
 * 1. reserve (`POST /v1/spaces/<id>/files/uploads`) with a request ID made
 *    here; a lost answer is asked again with the same ID;
 * 2. up to 10 MiB: one `runtime.fileApi('/uploads/<id>/content')` PUT (the
 *    backend's `/api/files/uploads/<id>/content`); larger files (up to
 *    2 GiB): `fileApi('/uploads/<id>/parts?offset=N')`, one part of at most
 *    8 MiB per request, each starting at the committed `received`. After a
 *    failure the upload state is read again and the transfer resumes there;
 * 3. complete (`POST .../uploads/<id>/complete`), safe to repeat.
 *
 * `runtime.fileApi` uses `fetch`, which reports no upload progress, so
 * progress advances per part (a small file jumps to 100%).
 */
export const SINGLE_UPLOAD_LIMIT = 10 * 1024 * 1024;
export const MAX_UPLOAD_BYTES = 2 * 1024 * 1024 * 1024;
export const PART_BYTES = 8 * 1024 * 1024;
/** Consecutive failures without progress before a transfer gives up. */
const MAX_STALLED_ATTEMPTS = 5;
const COMPLETION_ATTEMPTS = 90;

export type UploadState = components['schemas']['UploadOut'];

/** What a resumed or cancelled transfer needs (serializable, no File). */
export interface UploadHandle {
  spaceId: string;
  requestId: string;
  uploadId: string | null;
}

export interface TransferOptions {
  folderId: string | null;
  visibility: FileVisibility;
  /** Upload a new version of this file. */
  fileId?: string | null;
  onProgress?: (progress: FileUploadProgress) => void;
  /** Called as soon as the request ID and later the upload ID are known. */
  onHandle?: (handle: UploadHandle) => void;
  signal?: AbortSignal;
  /** Resume this earlier attempt (same request ID). */
  resume?: UploadHandle | null;
  /** Wait between retries; replaced in tests. */
  sleep?: (ms: number) => Promise<void>;
  createRequestId?: () => string;
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Codes whose request may simply be sent again. */
const RETRY_CODES = new Set([
  'files_part_offset',
  'files_upload_incomplete',
  'files_upload_result_unknown',
  'file_upload_in_progress',
]);

export function retriable(error: unknown): boolean {
  if (!isApiError(error)) return false;
  if (error.kind === 'network') return true;
  if (error.kind !== 'http') return false;
  const status = error.status ?? 0;
  return (
    RETRY_CODES.has(error.code) ||
    (status >= 500 && status !== 501) ||
    status === 408 ||
    status === 429
  );
}

function throwIfAborted(signal?: AbortSignal) {
  if (signal?.aborted) {
    throw new ApiError('abort', 'request_aborted');
  }
}

function spaceUploads(spaceId: string, tail = ''): string {
  return `/v1/spaces/${encodeURIComponent(spaceId)}/files/uploads${tail}`;
}

/** Retry a call that is safe to repeat (same request ID / idempotent). */
async function repeatable<T>(
  operation: () => Promise<T>,
  options: TransferOptions,
  attempts = 3,
): Promise<T> {
  const sleep = options.sleep ?? defaultSleep;
  let lastError: unknown;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    throwIfAborted(options.signal);
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      if (!retriable(error) || attempt === attempts - 1) break;
      await sleep(1000 * 2 ** attempt);
    }
  }
  throw lastError;
}

export function getUploadState(
  client: DriveClient,
  spaceId: string,
  uploadId: string,
  signal?: AbortSignal,
): Promise<UploadState> {
  return client.api<UploadState>(spaceUploads(spaceId, `/${encodeURIComponent(uploadId)}`), {
    signal,
  });
}

export function cancelUpload(
  client: DriveClient,
  spaceId: string,
  uploadId: string,
): Promise<void> {
  return client.api<void>(spaceUploads(spaceId, `/${encodeURIComponent(uploadId)}`), {
    method: 'DELETE',
  });
}

async function completeTransfer(
  client: DriveClient,
  path: string,
  options: TransferOptions,
): Promise<FileItem> {
  const sleep = options.sleep ?? defaultSleep;
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await repeatable(
        () => client.api<FileItem>(path, { method: 'POST', signal: options.signal }),
        options,
      );
    } catch (error) {
      if (
        !isApiError(error) ||
        error.kind !== 'http' ||
        error.status !== 409 ||
        error.code !== 'files_upload_finalizing' ||
        attempt >= COMPLETION_ATTEMPTS
      ) {
        throw error;
      }
      await sleep(2000);
    }
  }
}

async function sendSingle(
  client: DriveClient,
  spaceId: string,
  upload: UploadState,
  file: File,
  options: TransferOptions,
): Promise<UploadState> {
  const sleep = options.sleep ?? defaultSleep;
  let state = upload;
  for (let attempt = 0; state.status === 'uploading'; attempt += 1) {
    throwIfAborted(options.signal);
    try {
      const response = await client.fileApi(
        `/uploads/${encodeURIComponent(upload.id)}/content`,
        {
          method: 'PUT',
          body: file,
          headers: { 'Content-Type': 'application/octet-stream' },
          signal: options.signal,
        },
      );
      state = await readJson<UploadState>(response, { signal: options.signal });
    } catch (error) {
      if (!retriable(error) || attempt >= MAX_STALLED_ATTEMPTS - 1) throw error;
      await sleep(1000 * 2 ** Math.min(attempt, 4));
      state = await repeatable(
        () => getUploadState(client, spaceId, upload.id, options.signal),
        options,
      );
    }
  }
  return state;
}

async function sendParts(
  client: DriveClient,
  spaceId: string,
  upload: UploadState,
  file: File,
  options: TransferOptions,
): Promise<UploadState> {
  const sleep = options.sleep ?? defaultSleep;
  const partSize = Math.min(upload.part_size || PART_BYTES, PART_BYTES);
  let state = upload;
  let stalled = 0;
  while (state.status === 'uploading' && state.received < file.size) {
    throwIfAborted(options.signal);
    const start = state.received;
    const end = Math.min(start + partSize, file.size);
    try {
      const response = await client.fileApi(
        `/uploads/${encodeURIComponent(upload.id)}/parts?offset=${start}`,
        {
          method: 'PUT',
          body: file.slice(start, end),
          headers: { 'Content-Type': 'application/octet-stream' },
          signal: options.signal,
        },
      );
      const next = await readJson<UploadState>(response, { signal: options.signal });
      stalled = next.received > start ? 0 : stalled + 1;
      state = next;
    } catch (error) {
      if (!retriable(error)) throw error;
      stalled += 1;
      if (stalled >= MAX_STALLED_ATTEMPTS) throw error;
      await sleep(1000 * 2 ** Math.min(stalled - 1, 4));
      // Resume from what Storage committed (the answer may have been lost).
      state = await repeatable(
        () => getUploadState(client, spaceId, upload.id, options.signal),
        options,
      );
      if (state.received > start) stalled = 0;
    }
    if (stalled >= MAX_STALLED_ATTEMPTS) {
      throw new ApiError('http', 'files_part_offset', 409);
    }
    options.onProgress?.({
      loaded: state.received,
      total: file.size,
      percent: file.size > 0 ? Math.round((state.received / file.size) * 100) : 100,
    });
  }
  return state;
}

/** Upload one file (or a new version of one) and return the file item. */
export async function transferFile(
  client: DriveClient,
  spaceId: string,
  file: File,
  options: TransferOptions,
): Promise<FileItem> {
  try {
    const requestId =
      options.resume?.requestId ??
      (options.createRequestId ?? newRequestId)();
    options.onHandle?.({ spaceId, requestId, uploadId: options.resume?.uploadId ?? null });
    let upload = await repeatable(
      () =>
        client.api<UploadState>(spaceUploads(spaceId), {
          method: 'POST',
          signal: options.signal,
          body: JSON.stringify({
            request_id: requestId,
            filename: file.name,
            content_type: file.type || 'application/octet-stream',
            size_bytes: file.size,
            folder_id: options.folderId,
            visibility: options.visibility,
            file_id: options.fileId ?? null,
          } satisfies components['schemas']['UploadRequest']),
        }),
      options,
    );
    options.onHandle?.({ spaceId, requestId, uploadId: upload.id });
    if (upload.status === 'cancelled') {
      throw new ApiError('http', 'files_upload_cancelled', 409);
    }
    if (upload.status === 'reserving') {
      upload = await repeatable(
        () => getUploadState(client, spaceId, upload.id, options.signal),
        options,
      );
    }
    options.onProgress?.({ loaded: upload.received, total: file.size, percent: 0 });
    if (upload.status === 'uploading') {
      upload =
        upload.transfer === 'parts'
          ? await sendParts(client, spaceId, upload, file, options)
          : await sendSingle(client, spaceId, upload, file, options);
    }
    if (upload.status === 'cancelled') {
      throw new ApiError('http', 'files_upload_cancelled', 409);
    }
    const item = await completeTransfer(
      client,
      spaceUploads(spaceId, `/${encodeURIComponent(upload.id)}/complete`),
      options,
    );
    options.onProgress?.({ loaded: file.size, total: file.size, percent: 100 });
    return item;
  } catch (error) {
    throw mapTransportError(error);
  }
}
