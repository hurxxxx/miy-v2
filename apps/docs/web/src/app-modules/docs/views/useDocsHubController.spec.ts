import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import type { DocsHubItem, DocsHubResponse } from '../api/docs-api';
import {
  useDocsHubController,
  type DocsHubControllerClient,
} from './useDocsHubController';

const api = vi.fn() as unknown as AppApi;

function doc(overrides: Partial<DocsHubItem> = {}): DocsHubItem {
  return {
    id: 'doc-1',
    title: 'Doc',
    doc_type: 'native_doc',
    source_type: 'native_doc',
    source_ref: null,
    collection: null,
    primary_target: null,
    source_deeplink: null,
    trashed_at: null,
    last_viewed_at: null,
    sharing_summary: null,
    page_count: 1,
    content_format: 'block',
    can_edit: true,
    can_manage: true,
    is_favorite: false,
    created_by_name: 'Ada',
    created_at: '2026-05-30T00:00:00Z',
    updated_at: '2026-05-30T00:00:00Z',
    ...overrides,
  } as DocsHubItem;
}

function client(
  overrides: Partial<DocsHubControllerClient> = {},
): DocsHubControllerClient {
  return {
    listDocsHub: vi.fn().mockResolvedValue({
      items: [doc()],
      total: 1,
      page: 1,
      page_size: 50,
    }),
    ...overrides,
  };
}

describe('useDocsHubController', () => {
  it('loads docs with the current hub filters', async () => {
    const testClient = client();
    const { result } = renderHook(() =>
      useDocsHubController({
        api,
        spaceId: 'hq',
        listEnabled: true,
        activeCategory: 'mine',
        activeSourceApp: 'meeting',
        activeSourceKind: 'minutes',
        activeSpaceId: 'space-1',
        client: testClient,
      }),
    );

    await waitFor(() => expect(result.current.state.docs).toHaveLength(1));

    expect(testClient.listDocsHub).toHaveBeenCalledWith(
      api,
      {
        view: 'mine',
        q: undefined,
        sort_by: 'updated_at',
        sort_dir: 'desc',
        source_app: 'meeting',
        source_kind: 'minutes',
        space_id: 'space-1',
        page: 1,
        page_size: 50,
      },
      'hq',
    );
    expect(result.current.state.total).toBe(1);
    expect(result.current.state.loadingList).toBe(false);
  });

  it('debounces search changes into a list reload', async () => {
    const testClient = client();
    const { result } = renderHook(() =>
      useDocsHubController({
        api,
        spaceId: 'hq',
        listEnabled: true,
        activeCategory: 'all',
        client: testClient,
        searchDebounceMs: 0,
      }),
    );

    await waitFor(() => expect(result.current.state.docs).toHaveLength(1));
    vi.mocked(testClient.listDocsHub).mockClear();

    act(() => {
      result.current.actions.handleSearchChange('roadmap');
    });

    await waitFor(() => {
      expect(testClient.listDocsHub).toHaveBeenCalledWith(
        api,
        expect.objectContaining({ q: 'roadmap' }),
        'hq',
      );
    });
  });

  it('does not load the hub list when list view is disabled', () => {
    const testClient = client();
    renderHook(() =>
      useDocsHubController({
        api,
        spaceId: 'hq',
        listEnabled: false,
        activeCategory: 'all',
        client: testClient,
      }),
    );

    expect(testClient.listDocsHub).not.toHaveBeenCalled();
  });

  it('loads another results page and resets to the first page on search', async () => {
    const testClient = client({
      listDocsHub: vi.fn().mockImplementation(async (_api, params) => ({
        items: [doc({ id: `doc-page-${params.page}` })],
        total: params.q ? 1 : 51,
        page: params.page,
        page_size: params.page_size,
      })),
    });
    const { result } = renderHook(() =>
      useDocsHubController({
        api,
        listEnabled: true,
        activeCategory: 'all',
        client: testClient,
        searchDebounceMs: 0,
      }),
    );

    await waitFor(() => expect(result.current.state.pageCount).toBe(2));
    act(() => result.current.actions.setPage(2));
    await waitFor(() => expect(result.current.state.docs[0]?.id).toBe('doc-page-2'));

    act(() => result.current.actions.handleSearchChange('policy'));
    await waitFor(() => expect(result.current.state.docs[0]?.id).toBe('doc-page-1'));
    expect(testClient.listDocsHub).toHaveBeenLastCalledWith(
      api,
      expect.objectContaining({ page: 1, page_size: 50, q: 'policy' }),
      undefined,
    );
    expect(result.current.state.pageCount).toBe(1);
  });

  it('clears list state when hub loading fails', async () => {
    const testClient = client({
      listDocsHub: vi.fn().mockRejectedValue(new Error('failed')),
    });
    const { result } = renderHook(() =>
      useDocsHubController({
        api,
        spaceId: 'hq',
        listEnabled: true,
        activeCategory: 'all',
        client: testClient,
      }),
    );

    await waitFor(() => expect(result.current.state.loadingList).toBe(false));

    expect(result.current.state.docs).toEqual([]);
    expect(result.current.state.total).toBe(0);
    expect(result.current.state.loadFailed).toBe(true);
  });

  it('keeps the newest list when an earlier request answers late (miy)', async () => {
    let resolveFirst: (value: DocsHubResponse) => void = () => undefined;
    const listDocsHub = vi
      .fn()
      .mockImplementationOnce(
        () => new Promise<DocsHubResponse>((resolve) => (resolveFirst = resolve)),
      )
      .mockResolvedValue({
        items: [doc({ id: 'newer', title: 'Newer' })],
        total: 1,
        page: 1,
        page_size: 50,
      });
    const { result, rerender } = renderHook(
      ({ category }: { category: 'all' | 'mine' }) =>
        useDocsHubController({
          api,
          spaceId: 'hq',
          listEnabled: true,
          activeCategory: category,
          client: { listDocsHub },
        }),
      { initialProps: { category: 'all' } },
    );
    rerender({ category: 'mine' });
    await waitFor(() => expect(result.current.state.docs[0]?.id).toBe('newer'));
    await act(async () => {
      resolveFirst({ items: [doc({ id: 'older' })], total: 1, page: 1, page_size: 50 });
    });
    expect(result.current.state.docs[0]?.id).toBe('newer');
  });
});
