import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';

import { FilesApiError, type DriveClient } from './files-api';
import { PART_BYTES, retriable, transferFile, type UploadState } from './file-transfer';

/**
 * miy large transfer: a reservation with a client request ID, 8 MiB parts
 * through `runtime.fileApi` resuming from the committed offset, completion.
 * A fake backend keeps the committed offset like Core.
 */
function fakeBackend(size: number, options: { failPartOnce?: number; transfer?: 'single' | 'parts' } = {}) {
  let received = 0;
  let status: UploadState['status'] = 'uploading';
  let failedOnce = false;
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
      return { id: 'file-1', filename: 'big.bin' };
    }
    throw new Error(`unexpected ${path}`);
  });
  const fileApi = vi.fn(async (path: string, init?: RequestInit) => {
    const body = init?.body as Blob;
    if (path.includes('/content')) {
      received = body.size;
      status = 'ready';
      return new Response(JSON.stringify(state()), { status: 200 });
    }
    const offset = Number(new URL(path, 'https://x').searchParams.get('offset'));
    if (offset !== received) {
      throw new ApiError('http', 'files_part_offset', 409);
    }
    received += body.size;
    parts.push(offset);
    if (options.failPartOnce === offset && !failedOnce) {
      // The part was stored but the answer was lost.
      failedOnce = true;
      throw new ApiError('network', 'network_unavailable');
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

  it('reuses the request ID of a resumed attempt', async () => {
    const backend = fakeBackend(10, { transfer: 'single' });
    await transferFile(backend.client, 's', bigFile(10), {
      folderId: 'f',
      visibility: 'workspace',
      resume: { spaceId: 's', requestId: 'req-earlier', uploadId: 'upload-1' },
      sleep: noSleep,
    });
    expect(backend.reservations).toEqual(['req-earlier']);
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

    backend.api.mockRejectedValueOnce(new ApiError('http', 'files_storage_quota_exceeded', 409));
    const error = await transferFile(backend.client, 's', bigFile(10), {
      folderId: null,
      visibility: 'private',
      sleep: noSleep,
    }).catch((caught) => caught);
    expect(error).toBeInstanceOf(FilesApiError);
    expect(error.code).toBe('files_storage_quota_exceeded');
  });

  it('retries only what may be sent again', () => {
    expect(retriable(new ApiError('network', 'network_unavailable'))).toBe(true);
    expect(retriable(new ApiError('http', 'files_part_offset', 409))).toBe(true);
    expect(retriable(new ApiError('http', 'http_502', 502))).toBe(true);
    expect(retriable(new ApiError('http', 'files_storage_quota_exceeded', 409))).toBe(false);
    expect(retriable(new ApiError('http', 'acl_denied', 403))).toBe(false);
    expect(retriable(new ApiError('abort', 'request_aborted'))).toBe(false);
  });
});
