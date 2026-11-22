import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  DOCS_PAGE_REVISION_CONFLICT,
  DocsApiError,
  type DocsPageItem,
} from '../api/docs-api';
import {
  type DocsPageContentSaveControllerOptions,
  useDocsPageContentSaveController,
} from './useDocsPageContentSaveController';

const api = vi.fn() as unknown as AppApi;

function page(overrides: Partial<DocsPageItem> = {}): DocsPageItem {
  return {
    id: 'page-1',
    doc_id: 'doc-1',
    title: 'Page',
    parent_id: null,
    sort_order: 1000,
    content_format: 'block',
    content_blocks: [],
    content_text: null,
    can_edit: true,
    trashed_at: null,
    created_at: '2026-05-30T00:00:00Z',
    updated_at: '2026-05-30T00:00:00Z',
    revision: 2,
    ...overrides,
  } as DocsPageItem;
}

const blocks = (text: string) => [{ type: 'paragraph', content: text }];

function renderController(
  overrides: Partial<DocsPageContentSaveControllerOptions> = {},
) {
  const savePage = vi.fn<DocsPageContentSaveControllerOptions['savePage']>(
    async (_api, pageId, payload) =>
      page({ id: pageId, revision: payload.expected_revision + 1 }),
  );
  const onSavedPage = vi.fn<(updated: DocsPageItem, newer: boolean) => void>();
  const hook = renderHook(() =>
    useDocsPageContentSaveController({
      api,
      savePage,
      onSavedPage,
      ...overrides,
    }),
  );
  return { ...hook, onSavedPage, savePage };
}

describe('useDocsPageContentSaveController (miy single-editor saving)', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('debounces block saves and sends the latest blocks with the base revision', async () => {
    vi.useFakeTimers();
    const { result, savePage, onSavedPage } = renderController();
    act(() => {
      result.current.setBaseRevision('page-1', 2);
      result.current.queueBlockSave('page-1', blocks('first'));
      result.current.queueBlockSave('page-1', blocks('second'));
    });
    expect(result.current.dirty).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(799);
    });
    expect(savePage).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(savePage).toHaveBeenCalledTimes(1);
    expect(savePage).toHaveBeenCalledWith(api, 'page-1', {
      content_blocks: blocks('second'),
      expected_revision: 2,
    });
    expect(onSavedPage).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'page-1', revision: 3 }),
      false,
    );
    expect(result.current.saveState.status).toBe('idle');
    expect(result.current.dirty).toBe(false);
  });

  it('chains edits made during a save onto the returned revision', async () => {
    let finish: (value: DocsPageItem) => void = () => undefined;
    const savePage = vi
      .fn<DocsPageContentSaveControllerOptions['savePage']>()
      .mockImplementationOnce(
        () => new Promise<DocsPageItem>((resolve) => (finish = resolve)),
      )
      .mockImplementation(async (_api, pageId, payload) =>
        page({ id: pageId, revision: payload.expected_revision + 1 }),
      );
    const { result, onSavedPage } = renderController({ savePage, debounceMs: 0 });
    act(() => {
      result.current.setBaseRevision('page-1', 2);
      result.current.queueBlockSave('page-1', blocks('one'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(savePage).toHaveBeenCalledTimes(1);
    act(() => {
      result.current.queueBlockSave('page-1', blocks('two'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    // Never two saves of one page at a time.
    expect(savePage).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish(page({ revision: 3 }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(onSavedPage).toHaveBeenNthCalledWith(1, expect.objectContaining({ revision: 3 }), true);
    expect(savePage).toHaveBeenLastCalledWith(api, 'page-1', {
      content_blocks: blocks('two'),
      expected_revision: 3,
    });
    expect(result.current.saveState.status).toBe('idle');
  });

  it('stops on a revision conflict, keeps the draft and resumes with "keep mine"', async () => {
    const savePage = vi
      .fn<DocsPageContentSaveControllerOptions['savePage']>()
      .mockRejectedValueOnce(
        new DocsApiError(409, 'conflict', DOCS_PAGE_REVISION_CONFLICT),
      )
      .mockImplementation(async (_api, pageId, payload) =>
        page({ id: pageId, revision: payload.expected_revision + 1 }),
      );
    const { result } = renderController({ savePage, debounceMs: 0 });
    act(() => {
      result.current.setBaseRevision('page-1', 2);
      result.current.queueBlockSave('page-1', blocks('mine'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(result.current.saveState).toEqual({ status: 'conflict', pageId: 'page-1' });
    expect(result.current.hasDraft('page-1')).toBe(true);
    // Further typing is kept but not sent while the conflict is open.
    act(() => {
      result.current.queueBlockSave('page-1', blocks('mine, more'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(savePage).toHaveBeenCalledTimes(1);
    await act(async () => {
      await result.current.resolveConflictKeepMine('page-1', 5);
    });
    expect(savePage).toHaveBeenLastCalledWith(api, 'page-1', {
      content_blocks: blocks('mine, more'),
      expected_revision: 5,
    });
    expect(result.current.saveState.status).toBe('idle');
  });

  it('drops the draft when the person reloads the newer page', async () => {
    const savePage = vi
      .fn<DocsPageContentSaveControllerOptions['savePage']>()
      .mockRejectedValue(new DocsApiError(409, 'conflict', DOCS_PAGE_REVISION_CONFLICT));
    const { result } = renderController({ savePage, debounceMs: 0 });
    act(() => {
      result.current.setBaseRevision('page-1', 2);
      result.current.queueBlockSave('page-1', blocks('mine'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    act(() => {
      result.current.resolveConflictReload(page({ revision: 7 }));
    });
    expect(result.current.hasDraft('page-1')).toBe(false);
    expect(result.current.dirty).toBe(false);
  });

  it('keeps a failed draft and sends it again on retry', async () => {
    const savePage = vi
      .fn<DocsPageContentSaveControllerOptions['savePage']>()
      .mockRejectedValueOnce(new DocsApiError(503, 'unavailable'))
      .mockImplementation(async (_api, pageId, payload) =>
        page({ id: pageId, revision: payload.expected_revision + 1 }),
      );
    const { result } = renderController({ savePage, debounceMs: 0 });
    act(() => {
      result.current.setBaseRevision('page-1', 2);
      result.current.queueBlockSave('page-1', blocks('kept'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(result.current.saveState.status).toBe('failed');
    expect(result.current.dirty).toBe(true);
    await act(async () => {
      await result.current.retry();
    });
    expect(savePage).toHaveBeenLastCalledWith(api, 'page-1', {
      content_blocks: blocks('kept'),
      expected_revision: 2,
    });
    expect(result.current.dirty).toBe(false);
  });

  it('saves drafts of two pages one after the other', async () => {
    const { result, savePage } = renderController({ debounceMs: 0 });
    act(() => {
      result.current.setBaseRevision('page-1', 1);
      result.current.setBaseRevision('page-2', 4);
      result.current.queueBlockSave('page-1', blocks('a'));
      result.current.queueBlockSave('page-2', blocks('b'));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(savePage.mock.calls.map((call) => [call[1], call[2].expected_revision])).toEqual([
      ['page-1', 1],
      ['page-2', 4],
    ]);
  });
});
