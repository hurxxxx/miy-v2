import { describe, expect, it, vi } from 'vitest';
import type { ApiErrorKind, FileRequestInit } from '@miy/sdk';

import { FilesApiError, type DriveClient } from './files-api';
import { PART_BYTES, retriable, transferFile, type UploadState } from './file-transfer';

/** The frame bundles its own SDK and passes its errors to Drive. */
class FrameApiError extends Error {
  readonly name = 'ApiError';
  readonly requestId = null;
  readonly retryable = false;

  constructor(
    readonly kind: ApiErrorKind,
    readonly code: string,
    readonly status: number | null = null,
  ) {
    super(code);
  }
}

/**
 * miy large transfer: a reservation with a client request ID, 8 MiB parts
 * through `runtime.fileApi` resuming from the committed offset, completion.
 * A fake backend keeps the committed offset like Core.
 */
function fakeBackend(size: number, options: {
  failPartOnce?: number;
  transfer?: 'single' | 'parts';
  pendingCompletions?: number;
  completionError?: FrameApiError;
} = {}) {
  let received = 0;
  let status: UploadState['status'] = 'uploading';
  let failedOnce = false;
  let pendingCompletions = options.pendingCompletions ?? 0;
  const parts: number[] = [];
  const reservations: string[] = [];
  const state = (): UploadState => ({
    id: 'upload-1',
    file_id: 'file-1',
    request_id: 'req-1',
    filename: 'big.bin',
    content_type: 'application/octet-stream',
    size_bytes: size,
    transfer: options.transfer ?? 'parts',
    status,
    received,
    part_size: PART_BYTES,
  });
  const api = vi.fn(async (path: string, init?: RequestInit) => {
    if (path === '/v1/spaces/s/files/uploads' && init?.method === 'POST') {
      reservations.push(JSON.parse(String(init.body)).request_id);
      return state();
    }
    if (path === '/v1/spaces/s/files/uploads/upload-1') return state();
    if (path === '/v1/spaces/s/files/uploads/upload-1/complete') {
      if (options.completionError) throw options.completionError;
      if (pendingCompletions > 0) {
        pendingCompletions -= 1;
        throw new FrameApiError('http', 'files_upload_finalizing', 409);
      }
      return { id: 'file-1', filename: 'big.bin' };
    }
    throw new Error(`unexpected ${path}`);
  });
  const fileApi = vi.fn(async (path: string, init?: FileRequestInit) => {
    const body = init?.body as Blob;
    init?.onUploadProgress?.(Math.floor(body.size / 2), body.size);
    if (path.includes('/content')) {
      received = body.size;
      status = 'ready';
      return new Response(JSON.stringify(state()), { status: 200 });
    }
    const offset = Number(new URL(path, 'https://x').searchParams.get('offset'));
    if (offset !== received) {
      throw new FrameApiError('http', 'files_part_offset', 409);
    }
    received += body.size;
    parts.push(offset);
    if (options.failPartOnce === offset && !failedOnce) {
      // The part was stored but the answer was lost.
      failedOnce = true;
      throw new FrameApiError('network', 'network_unavailable');
    }
    if (received === size) status = 'uploading';
    return new Response(JSON.stringify(state()), { status: 200 });
  });
  return {
    client: { api, fileApi } as unknown as DriveClient,
    api,
    fileApi,
    parts,
    reservations,
  };
}

function bigFile(size: number): File {
  return new File([new Uint8Array(size)], 'big.bin');
}

const noSleep = () => Promise.resolve();

describe('transferFile', () => {
  it('sends 8 MiB parts from the committed offset and completes', async () => {
    const size = PART_BYTES * 2 + 5;
    const backend = fakeBackend(size);
    const progress: number[] = [];
    const handles: Array<string | null> = [];
    const item = await transferFile(backend.client, 's', bigFile(size), {
      folderId: null,
      visibility: 'private',
      onProgress: (p) => progress.push(p.loaded),
      onHandle: (h) => handles.push(h.uploadId),
      createRequestId: () => '00000000-0000-4000-8000-000000000001',
      sleep: noSleep,
    });
    expect(item).toMatchObject({ id: 'file-1' });
    expect(backend.parts).toEqual([0, PART_BYTES, PART_BYTES * 2]);
    expect(backend.reservations).toEqual(['00000000-0000-4000-8000-000000000001']);
    expect(handles).toEqual([null, 'upload-1']);
    expect(progress).toEqual(expect.arrayContaining([PART_BYTES / 2, PART_BYTES + PART_BYTES / 2]));
    expect(progress.at(-1)).toBe(size);
  });

  it('resumes after a lost answer from what Storage committed', async () => {
    const size = PART_BYTES * 2;
    const backend = fakeBackend(size, { failPartOnce: 0 });
    await transferFile(backend.client, 's', bigFile(size), {
      folderId: null,
      visibility: 'private',
      sleep: noSleep,
    });
    // The first part is not sent twice: the state read says it arrived.
    expect(backend.parts).toEqual([0, PART_BYTES]);
    expect(backend.api).toHaveBeenCalledWith(
      '/v1/spaces/s/files/uploads/upload-1',
      expect.anything(),
    );
  });

  it('repeats completion with the same upload after digest preparation', async () => {
    const size = PART_BYTES + 1;
    const backend = fakeBackend(size, { pendingCompletions: 4 });
    const sleep = vi.fn(noSleep);
    await expect(transferFile(backend.client, 's', bigFile(size), {
      folderId: null,
      visibility: 'private',
      sleep,
    })).resolves.toMatchObject({ id: 'file-1' });
    expect(sleep.mock.calls).toEqual([[2000], [2000], [2000], [2000]]);
    expect(backend.parts).toEqual([0, PART_BYTES]);
    expect(backend.reservations).toHaveLength(1);
  });

  it('stops waiting when the completion retry budget is exhausted', async () => {
    const size = PART_BYTES + 1;
    const backend = fakeBackend(size, { pendingCompletions: 100 });
    const sleep = vi.fn(noSleep);
    await expect(transferFile(backend.client, 's', bigFile(size), {
      folderId: null,
      visibility: 'private',
      sleep,
    })).rejects.toMatchObject({ code: 'files_upload_finalizing', status: 409 });
    expect(sleep).toHaveBeenCalledTimes(89);
    expect(backend.parts).toEqual([0, PART_BYTES]);
    expect(backend.reservations).toHaveLength(1);
  });

  it('does not wait on a non-conflict response carrying the finalizing code', async () => {
    const backend = fakeBackend(10, {
      transfer: 'single',
      completionError: new FrameApiError('http', 'files_upload_finalizing', 403),
    });
    const sleep = vi.fn(noSleep);
    await expect(transferFile(backend.client, 's', bigFile(10), {
      folderId: null,
      visibility: 'private',
      sleep,
    })).rejects.toMatchObject({ code: 'files_upload_finalizing', status: 403 });
    expect(sleep).not.toHaveBeenCalled();
  });

  it('reuses the request ID of a resumed attempt', async () => {
    const backend = fakeBackend(10, { transfer: 'single' });
    const progress: number[] = [];
    await transferFile(backend.client, 's', bigFile(10), {
      folderId: 'f',
      visibility: 'workspace',
      resume: { spaceId: 's', requestId: 'req-earlier', uploadId: 'upload-1' },
      onProgress: (value) => progress.push(value.loaded),
      sleep: noSleep,
    });
    expect(backend.reservations).toEqual(['req-earlier']);
    expect(progress).toEqual([0, 5, 10]);
    expect(backend.fileApi).toHaveBeenCalledWith(
      '/uploads/upload-1/content',
      expect.objectContaining({ method: 'PUT' }),
    );
  });

  it('stops at once when cancelled and maps other errors', async () => {
    const backend = fakeBackend(10, { transfer: 'single' });
    const controller = new AbortController();
    controller.abort();
    await expect(
      transferFile(backend.client, 's', bigFile(10), {
        folderId: null,
        visibility: 'private',
        signal: controller.signal,
        sleep: noSleep,
      }),
    ).rejects.toMatchObject({ kind: 'abort' });

    backend.api.mockRejectedValueOnce(new FrameApiError('http', 'files_storage_quota_exceeded', 409));
    const error = await transferFile(backend.client, 's', bigFile(10), {
      folderId: null,
      visibility: 'private',
      sleep: noSleep,
    }).catch((caught) => caught);
    expect(error).toBeInstanceOf(FilesApiError);
    expect(error.code).toBe('files_storage_quota_exceeded');
    expect(backend.api).toHaveBeenCalledTimes(1);
  });

  it('retries only what may be sent again', () => {
    expect(retriable(new FrameApiError('network', 'network_unavailable'))).toBe(true);
    expect(retriable(new FrameApiError('http', 'files_part_offset', 409))).toBe(true);
    expect(retriable(new FrameApiError('http', 'http_502', 502))).toBe(true);
    expect(retriable(new FrameApiError('http', 'http_501', 501))).toBe(false);
    expect(retriable(new FrameApiError('http', 'files_storage_quota_exceeded', 409))).toBe(false);
    expect(retriable(new FrameApiError('http', 'acl_denied', 403))).toBe(false);
    expect(retriable(new FrameApiError('abort', 'request_aborted'))).toBe(false);
    expect(retriable(new Error('network_unavailable'))).toBe(false);
  });
});
