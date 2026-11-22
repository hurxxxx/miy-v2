import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { usePmsSavedFilters } from './usePmsSavedFilters';
import type { PmsSavedFilter, PmsSavedFilterScope } from '../api/pms-api';

// miy reinforcement: saved filters are read and written on the server.
const apiMocks = vi.hoisted(() => ({
  listSavedFilters: vi.fn(),
  createSavedFilter: vi.fn(),
  updateSavedFilter: vi.fn(),
  deleteSavedFilter: vi.fn(),
  setDefaultSavedFilter: vi.fn(),
}));
const testApi = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/runtime/runtime-context', () => ({
  useAppApi: () => testApi,
}));
vi.mock('../api/pms-api', () => apiMocks);

function filter(id: string, overrides: Partial<PmsSavedFilter> = {}) {
  return {
    id,
    name: id,
    params: { archived_state: 'active' },
    sort: null,
    group_by: null,
    shared: false,
    space_id: 'space-1',
    list_id: 'list-1',
    owner_id: 'user-1',
    owner_name: 'User',
    is_mine: true,
    is_default: false,
    can_manage: true,
    created_at: '',
    updated_at: '',
    ...overrides,
  } as PmsSavedFilter;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe('usePmsSavedFilters', () => {
  beforeEach(() => {
    Object.values(apiMocks).forEach((mock) => mock.mockReset());
  });

  it('loads the view and opens it once with the default filter', async () => {
    const opened = filter('f-2', { is_default: true });
    apiMocks.listSavedFilters.mockResolvedValue({
      items: [filter('f-1'), opened],
      default_filter_id: 'f-2',
    });
    const onOpeningDefault = vi.fn();
    const { result } = renderHook(() =>
      usePmsSavedFilters({ scope: { listId: 'list-1' }, onOpeningDefault }),
    );

    await waitFor(() => expect(result.current.items).toHaveLength(2));
    expect(apiMocks.listSavedFilters).toHaveBeenCalledWith(testApi, {
      listId: 'list-1',
    });
    expect(onOpeningDefault).toHaveBeenCalledWith(opened, 'list:list-1');

    apiMocks.createSavedFilter.mockResolvedValue(filter('f-3'));
    await act(async () => {
      await result.current.create({ name: 'f-3', params: {} });
    });
    expect(apiMocks.createSavedFilter).toHaveBeenCalledWith(
      testApi,
      { listId: 'list-1' },
      { name: 'f-3', params: {} },
    );
    // The refresh after a write is not an opening.
    expect(onOpeningDefault).toHaveBeenCalledTimes(1);
  });

  it('drops a list that answers after the person moved to another view', async () => {
    const first = deferred<{ items: PmsSavedFilter[]; default_filter_id: null }>();
    apiMocks.listSavedFilters.mockImplementation(
      (_api: unknown, scope: PmsSavedFilterScope) =>
        scope.listId === 'list-1'
          ? first.promise
          : Promise.resolve({ items: [filter('mine-2')], default_filter_id: null }),
    );
    const { result, rerender } = renderHook(
      ({ listId }: { listId: string }) =>
        usePmsSavedFilters({ scope: { listId } }),
      { initialProps: { listId: 'list-1' } },
    );
    rerender({ listId: 'list-2' });
    await waitFor(() =>
      expect(result.current.items.map((item) => item.id)).toEqual(['mine-2']),
    );
    await act(async () => {
      first.resolve({ items: [filter('stale')], default_filter_id: null });
    });
    expect(result.current.items.map((item) => item.id)).toEqual(['mine-2']);
    expect(result.current.scopeKey).toBe('list:list-2');
  });

  it('reports a failed write and keeps the list', async () => {
    apiMocks.listSavedFilters.mockResolvedValue({
      items: [filter('f-1')],
      default_filter_id: null,
    });
    apiMocks.deleteSavedFilter.mockRejectedValue(new Error('offline'));
    const { result } = renderHook(() =>
      usePmsSavedFilters({ scope: { spaceId: 'space-1' } }),
    );
    await waitFor(() => expect(result.current.items).toHaveLength(1));
    let removed = true;
    await act(async () => {
      removed = await result.current.remove(result.current.items[0]);
    });
    expect(removed).toBe(false);
    expect(result.current.actionError).toBeTruthy();
    expect(result.current.items).toHaveLength(1);
  });
});
