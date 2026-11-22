import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';
import { useRef, useState } from 'react';

import type { WhiteboardDetail } from '../api/whiteboard-api';
import { createWhiteboardFileStore } from '../api/whiteboard-files';
import { useWhiteboardScenePersistence } from './useWhiteboardScenePersistence';

// miy: single-editor saving (revision, conflict, kept edits) of the AI-DO hook.
vi.mock('@excalidraw/excalidraw', () => ({
  getSceneVersion: (elements: Array<{ version?: number }>) =>
    elements.reduce((sum, element) => sum + (element.version ?? 0), 0),
  serializeAsJSON: (elements: unknown, appState: unknown, files: unknown) =>
    JSON.stringify({ elements, appState, files }),
}));

const board = {
  id: 'board-1',
  title: 'Board',
  can_edit: true,
  revision: 4,
  scene: { elements: [], appState: {}, files: {} },
} as unknown as WhiteboardDetail;

function element(id: string, version: number) {
  return { id, type: 'rectangle', version };
}

function setup(api: ReturnType<typeof vi.fn>) {
  const setError = vi.fn();
  const hook = renderHook(() => {
    const [activeBoard, setActiveBoard] = useState<WhiteboardDetail | null>(board);
    const activeBoardIdRef = useRef<string | null>(board.id);
    const activeBoardRef = useRef<WhiteboardDetail | null>(board);
    const fileStoreRef = useRef(createWhiteboardFileStore());
    const persistence = useWhiteboardScenePersistence({
      activeBoard,
      activeBoardIdRef,
      activeBoardRef,
      api: api as never,
      fileApi: vi.fn() as never,
      fileStoreRef,
      setActiveBoard,
      setError,
      spaceId: 'global',
      translate: (key) => key,
    });
    return { persistence, activeBoardIdRef, activeBoard };
  });
  act(() => hook.result.current.persistence.resetLoadedScene(board.scene, 4));
  return { hook, setError };
}

async function change(
  hook: ReturnType<typeof setup>['hook'],
  elements: unknown[],
) {
  act(() =>
    hook.result.current.persistence.queueLocalChange({
      elements: elements as never,
      appState: {} as never,
      files: {} as never,
    }),
  );
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1200);
  });
}

function sentBody(api: ReturnType<typeof vi.fn>, call: number) {
  return JSON.parse(api.mock.calls[call][1].body as string);
}

describe('whiteboard scene persistence', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('saves on the loaded revision and continues from the saved one', async () => {
    const api = vi
      .fn()
      .mockResolvedValueOnce({ ...board, revision: 5 })
      .mockResolvedValueOnce({ ...board, revision: 6 });
    const { hook } = setup(api);
    await change(hook, [element('a', 1)]);
    expect(api).toHaveBeenCalledTimes(1);
    expect(api.mock.calls[0][0]).toBe('/v1/whiteboard/items/board-1');
    expect(sentBody(api, 0).base_revision).toBe(4);
    await change(hook, [element('a', 2)]);
    expect(sentBody(api, 1).base_revision).toBe(5);
    expect(hook.result.current.persistence.hasUnsavedScene()).toBe(false);
  });

  it('stops on a conflict, keeps the edit and resends it on keep mine', async () => {
    const api = vi
      .fn()
      .mockRejectedValueOnce(new ApiError('http', 'whiteboard_revision_conflict', 409))
      .mockResolvedValueOnce({ ...board, revision: 8 });
    const { hook, setError } = setup(api);
    await change(hook, [element('a', 1)]);
    expect(hook.result.current.persistence.conflict).toBe(true);
    expect(hook.result.current.persistence.saveStatus).toBe('error');
    expect(hook.result.current.persistence.hasUnsavedScene()).toBe(true);
    expect(setError).not.toHaveBeenCalled();
    // No automatic overwrite while the conflict stands.
    await change(hook, [element('a', 2)]);
    expect(api).toHaveBeenCalledTimes(1);
    await act(async () => {
      hook.result.current.persistence.keepLocalSceneOver(7);
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(api).toHaveBeenCalledTimes(2);
    const resent = sentBody(api, 1);
    expect(resent.base_revision).toBe(7);
    expect(resent.scene.elements).toEqual([element('a', 2)]);
    expect(hook.result.current.persistence.conflict).toBe(false);
    expect(hook.result.current.persistence.hasUnsavedScene()).toBe(false);
  });

  it('keeps a failed save pending and sends it again on retry', async () => {
    const api = vi
      .fn()
      .mockRejectedValueOnce(new ApiError('network', 'network_error', null))
      .mockResolvedValueOnce({ ...board, revision: 5 });
    const { hook, setError } = setup(api);
    await change(hook, [element('a', 1)]);
    expect(setError).toHaveBeenCalled();
    expect(hook.result.current.persistence.saveStatus).toBe('error');
    expect(hook.result.current.persistence.hasUnsavedScene()).toBe(true);
    await act(async () => {
      hook.result.current.persistence.retrySave();
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(api).toHaveBeenCalledTimes(2);
    expect(sentBody(api, 1).scene.elements).toEqual([element('a', 1)]);
    expect(sentBody(api, 1).base_revision).toBe(4);
    expect(hook.result.current.persistence.hasUnsavedScene()).toBe(false);
  });

  it('ignores a late answer after the editor moved to another board', async () => {
    let resolve: (value: unknown) => void = () => undefined;
    const api = vi.fn().mockReturnValueOnce(new Promise((done) => (resolve = done)));
    const { hook } = setup(api);
    await change(hook, [element('a', 1)]);
    expect(api).toHaveBeenCalledTimes(1);
    hook.result.current.activeBoardIdRef.current = 'board-2';
    await act(async () => {
      resolve({ ...board, title: 'Late', revision: 9 });
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(hook.result.current.activeBoard?.title).toBe('Board');
  });
});
