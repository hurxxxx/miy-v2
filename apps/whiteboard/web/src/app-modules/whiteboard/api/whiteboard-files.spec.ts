import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';

import {
  WhiteboardImageError,
  createWhiteboardFileStore,
  dataUrlToBlob,
  storeSceneFiles,
} from './whiteboard-files';

// miy (보강): images move from the scene to the core storage.
const PNG = 'data:image/png;base64,iVBORw0KGgo=';

function imageScene(files: Record<string, unknown>, fileIds = Object.keys(files)) {
  return {
    elements: fileIds.map((fileId, index) => ({ id: `img-${index}`, type: 'image', fileId })),
    appState: {},
    files,
  };
}

function okResponse(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200 });
}

describe('whiteboard stored images', () => {
  it('stores a new image before the scene names it', async () => {
    const api = vi.fn().mockResolvedValue({ id: 'row-1', file_id: 'f1', status: 'pending' });
    const fileApi = vi.fn().mockResolvedValue(okResponse({ id: 'row-1', status: 'ready' }));
    const store = createWhiteboardFileStore();
    const saved = await storeSceneFiles(
      imageScene({ f1: { id: 'f1', mimeType: 'image/png', dataURL: PNG, created: 7 } }),
      store,
      { api, fileApi, boardId: 'b1', spaceId: 'global' },
    );
    expect(saved.files).toEqual({
      f1: { id: 'f1', mimeType: 'image/png', storage: 'row-1', created: 7 },
    });
    const reserved = JSON.parse(api.mock.calls[0][1].body);
    expect(api.mock.calls[0][0]).toBe('/v1/whiteboard/items/b1/files');
    expect(reserved).toMatchObject({ file_id: 'f1', mime: 'image/png' });
    expect(fileApi.mock.calls[0][0]).toBe('/whiteboard/items/b1/files/row-1');
    expect(fileApi.mock.calls[0][1].method).toBe('PUT');
    // Stored once: the next save reuses the entry without another upload.
    await storeSceneFiles(
      imageScene({ f1: { id: 'f1', mimeType: 'image/png', dataURL: PNG, created: 7 } }),
      store,
      { api, fileApi, boardId: 'b1', spaceId: 'global' },
    );
    expect(api).toHaveBeenCalledTimes(1);
  });

  it('resumes an interrupted upload with the same request ID', async () => {
    const api = vi
      .fn()
      .mockRejectedValueOnce(new ApiError('network', 'network_error', null))
      .mockResolvedValueOnce({ id: 'row-1', file_id: 'f1', status: 'ready' });
    const fileApi = vi.fn();
    const store = createWhiteboardFileStore();
    const scene = imageScene({ f1: { id: 'f1', mimeType: 'image/png', dataURL: PNG } });
    const context = { api, fileApi, boardId: 'b1', spaceId: 'global' };
    await expect(storeSceneFiles(scene, store, context)).rejects.toBeTruthy();
    const saved = await storeSceneFiles(scene, store, context);
    const first = JSON.parse(api.mock.calls[0][1].body).request_id;
    const second = JSON.parse(api.mock.calls[1][1].body).request_id;
    expect(second).toBe(first);
    // The bytes were already stored: no second transfer.
    expect(fileApi).not.toHaveBeenCalled();
    expect((saved.files.f1 as { storage: string }).storage).toBe('row-1');
  });

  it('keeps the stored entry of an image the editor could not load', async () => {
    const stored = { f1: { id: 'f1', mimeType: 'image/png', storage: 'row-1', created: 1 } };
    const store = createWhiteboardFileStore(imageScene(stored));
    const saved = await storeSceneFiles(imageScene({}, ['f1']), store, {
      api: vi.fn(),
      fileApi: vi.fn(),
      boardId: 'b1',
      spaceId: 'global',
    });
    expect(saved.files).toEqual(stored);
  });

  it('refuses images over 10 MB before any request', async () => {
    const big = `data:image/png;base64,${btoa('x'.repeat(10 * 1024 * 1024 + 3))}`;
    const api = vi.fn();
    await expect(
      storeSceneFiles(imageScene({ f1: { id: 'f1', mimeType: 'image/png', dataURL: big } }), createWhiteboardFileStore(), {
        api,
        fileApi: vi.fn(),
        boardId: 'b1',
        spaceId: 'global',
      }),
    ).rejects.toBeInstanceOf(WhiteboardImageError);
    expect(api).not.toHaveBeenCalled();
  });

  it('decodes data URLs', () => {
    expect(dataUrlToBlob(PNG).type).toBe('image/png');
    expect(dataUrlToBlob('data:image/svg+xml,%3Csvg%2F%3E').type).toBe('image/svg+xml');
  });
});
