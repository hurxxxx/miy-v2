import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';

import {
  getWhiteboard,
  listWhiteboardHub,
  listWhiteboardShareableUsers,
  normalizeWhiteboardScene,
  updateWhiteboard,
  WhiteboardApiError,
} from './whiteboard-api';
import { whiteboardApiRoutes } from './whiteboard-routes';

// AI-DO api/whiteboard-api.spec.ts: `runtime.api` with app-relative paths in
// place of `fetch`, the space ID in place of the workspace slug rewrite.
describe('whiteboard scene codec', () => {
  it('compacts duplicate element ids with the last element winning', () => {
    const firstShape = { id: 'shape-1', version: 1 };
    const anonymous = { type: 'text' };
    const secondShape = { id: 'shape-2', version: 1 };
    const latestShape = { id: 'shape-1', version: 2 };

    expect(
      normalizeWhiteboardScene({
        elements: [firstShape, anonymous, secondShape, latestShape],
        appState: { zoom: 1 },
        files: { file1: { id: 'file1' } },
      }).elements,
    ).toEqual([latestShape, secondShape, anonymous]);
  });

  it('defaults invalid scene values', () => {
    expect(normalizeWhiteboardScene(null)).toEqual({
      elements: [],
      appState: {},
      files: {},
    });
    expect(normalizeWhiteboardScene(['not', 'a', 'scene'])).toEqual({
      elements: [],
      appState: {},
      files: {},
    });
    expect(
      normalizeWhiteboardScene({
        title: 'Sketch',
        elements: 'bad',
        appState: null,
        files: [],
      }),
    ).toEqual({
      title: 'Sketch',
      elements: [],
      appState: {},
      files: {},
    });
  });
});

describe('whiteboard API routes', () => {
  it('omits empty query values while preserving encoded values', () => {
    expect(
      whiteboardApiRoutes.hub({
        view: 'recent',
        q: '',
        page: 0,
        page_size: 25,
        target_id: 'task/1',
      }),
    ).toBe(
      '/v1/whiteboard/hub?view=recent&page=0&page_size=25&target_id=task%2F1',
    );
    expect(whiteboardApiRoutes.shareableUsers()).toBe(
      '/v1/whiteboard/shareable-users',
    );
  });

  it('encodes item and sharing path segments', () => {
    expect(whiteboardApiRoutes.item('board/1')).toBe(
      '/v1/whiteboard/items/board%2F1',
    );
    expect(whiteboardApiRoutes.userShare('board/1', 'user 1')).toBe(
      '/v1/whiteboard/items/board%2F1/sharing/users/user%201',
    );
    expect(whiteboardApiRoutes.item('board/1', 'global')).toBe(
      '/v1/whiteboard/items/board%2F1',
    );
    expect(whiteboardApiRoutes.fileContent('board/1', 'row 1')).toBe(
      '/whiteboard/items/board%2F1/files/row%201',
    );
  });
});

describe('whiteboard API client', () => {
  it('uses route helpers for omitted query strings and the space prefix', async () => {
    const api = vi.fn().mockResolvedValue({ items: [], page: 1, page_size: 25, total: 0 });

    await listWhiteboardHub(
      api,
      { view: 'recent', q: '', page: 0, page_size: 25, target_id: 'task/1' },
      'team space',
    );
    expect(api).toHaveBeenCalledWith(
      '/v1/workspaces/team%20space/whiteboard/hub?view=recent&page=0&page_size=25&target_id=task%2F1',
      {},
    );

    api.mockResolvedValueOnce([]);
    await listWhiteboardShareableUsers(api, '', 'team space');
    expect(api).toHaveBeenLastCalledWith(
      '/v1/workspaces/team%20space/whiteboard/shareable-users',
      {},
    );
  });

  it('sends the base revision with a scene save', async () => {
    const api = vi.fn().mockResolvedValue({});
    const scene = { elements: [], appState: {}, files: {} };
    await updateWhiteboard(api, 'board-1', { scene, base_revision: 3 }, 'global');
    expect(api).toHaveBeenCalledWith('/v1/whiteboard/items/board-1', {
      method: 'PATCH',
      body: JSON.stringify({ scene, base_revision: 3 }),
    });
  });

  it('wraps API failures as WhiteboardApiError with the server code', async () => {
    const api = vi.fn().mockRejectedValue(
      new ApiError('http', 'whiteboard_revision_conflict', 409),
    );

    let caught: unknown;
    try {
      await getWhiteboard(api, 'board 1', 'team space');
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(WhiteboardApiError);
    expect(caught).toMatchObject({
      status: 409,
      code: 'whiteboard_revision_conflict',
      message: '다른 사람이 이 화이트보드를 먼저 저장했습니다.',
    });
    expect(api).toHaveBeenCalledWith(
      '/v1/workspaces/team%20space/whiteboard/items/board%201',
      {},
    );
  });
});
