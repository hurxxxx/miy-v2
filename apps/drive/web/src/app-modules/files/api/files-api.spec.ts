import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';

import {
  FilesApiError,
  browseFiles,
  getFileDownloadUrl,
  getFilePreviewBlob,
  moveFileItems,
  planFileArchive,
  searchFiles,
  type DriveClient,
} from './files-api';

/**
 * AI-DO files-api.spec.ts stubbed `fetch` and checked the workspace path.
 * miy calls `runtime.api` / `runtime.fileApi` with space-relative paths.
 */
function client(response: unknown = {}) {
  const api = vi.fn().mockResolvedValue(response);
  const fileApi = vi.fn();
  return { api, fileApi, client: { api, fileApi } as unknown as DriveClient };
}

describe('Files search API', () => {
  it('posts paged criteria through the space endpoint with cancellation', async () => {
    const { api, client: drive } = client({
      query: 'release plan',
      strategy: 'keyword',
      page: 2,
      page_size: 20,
      hits: [],
      has_more: false,
      max_ranked_results: 100,
      latency_ms: 3,
      trace_id: null,
    });
    const controller = new AbortController();

    await searchFiles(
      drive,
      'delivery hub',
      {
        page: 2,
        page_size: 20,
        query: 'release plan',
        strategy: 'keyword',
      },
      { signal: controller.signal },
    );

    expect(api).toHaveBeenCalledWith(
      '/v1/spaces/delivery%20hub/files/search',
      expect.objectContaining({
        body: JSON.stringify({
          page: 2,
          page_size: 20,
          query: 'release plan',
          strategy: 'keyword',
        }),
        method: 'POST',
        signal: controller.signal,
      }),
    );
  });

  it('gets a fresh download link from the download endpoint', async () => {
    const { api, client: drive } = client({ url: '/_miy/downloads/x' });

    await getFileDownloadUrl(drive, 'delivery-hub', 'file 1');

    expect(api).toHaveBeenCalledWith(
      '/v1/spaces/delivery-hub/files/file%201/download',
      expect.objectContaining({ cache: 'no-store' }),
    );
  });
});

describe('drive API (miy)', () => {
  it('browses a folder and posts selections in AI-DO snake case', async () => {
    const { api, client: drive } = client({ entries: [] });
    await browseFiles(drive, 's', 'f/1');
    expect(api).toHaveBeenLastCalledWith('/v1/spaces/s/files?folder_id=f%2F1', {
      signal: undefined,
    });
    await planFileArchive(drive, 's', { fileIds: ['a'], folderIds: ['b'] });
    expect(api).toHaveBeenLastCalledWith(
      '/v1/spaces/s/files/archive',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ file_ids: ['a'], folder_ids: ['b'] }),
      }),
    );
    await moveFileItems(drive, 's', { fileIds: ['a'], folderIds: [], targetFolderId: null });
    expect(api).toHaveBeenLastCalledWith(
      '/v1/spaces/s/files/move',
      expect.objectContaining({
        body: JSON.stringify({ file_ids: ['a'], folder_ids: [], target_folder_id: null }),
      }),
    );
  });

  it('reads previews through the file gateway as a Blob', async () => {
    const { fileApi, client: drive } = client();
    fileApi.mockResolvedValue(new Response('png', { status: 200 }));
    const blob = await getFilePreviewBlob(drive, 'file 1');
    expect(fileApi).toHaveBeenCalledWith('/items/file%201/preview', {
      signal: undefined,
    });
    expect(blob.size).toBe(3);
  });

  it('keeps the error code and a translated message', async () => {
    const { api, client: drive } = client();
    api.mockRejectedValue(new ApiError('http', 'files_folder_not_found', 404));
    const error = await browseFiles(drive, 's', 'missing').catch((caught) => caught);
    expect(error).toBeInstanceOf(FilesApiError);
    expect(error.status).toBe(404);
    expect(error.code).toBe('files_folder_not_found');
    expect(error.message).toBe('폴더를 찾을 수 없습니다.');
  });
});
