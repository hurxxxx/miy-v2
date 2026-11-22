import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { usePmsSpacePreferences } from './usePmsSpacePreferences';

const apiMocks = vi.hoisted(() => ({
  listSpacePreferences: vi.fn(),
  updateSpacePreference: vi.fn(),
}));

// miy: the hook reads `runtime.api` where AI-DO read the bearer token.
const testApi = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/runtime/runtime-context', () => ({
  useAppApi: () => testApi,
}));

vi.mock('../api/pms-api', () => apiMocks);

describe('usePmsSpacePreferences', () => {
  beforeEach(() => {
    apiMocks.listSpacePreferences.mockReset();
    apiMocks.updateSpacePreference.mockReset();
    apiMocks.listSpacePreferences.mockResolvedValue({
      items: [{ hidden: true, pinned: false, space_id: 'reference' }],
    });
  });

  it('loads the current user preferences for the workspace', async () => {
    const { result } = renderHook(() => usePmsSpacePreferences('workspace-1'));

    await waitFor(() =>
      expect(result.current.preferences.get('reference')?.hidden).toBe(true),
    );
    expect(apiMocks.listSpacePreferences).toHaveBeenCalledWith(
      testApi,
      'workspace-1',
    );
  });

  it('keeps every mounted view in step after a change is saved', async () => {
    apiMocks.updateSpacePreference.mockResolvedValue({
      hidden: false,
      pinned: true,
      space_id: 'main',
    });
    const sidebar = renderHook(() => usePmsSpacePreferences('workspace-1'));
    const overview = renderHook(() => usePmsSpacePreferences('workspace-1'));
    await waitFor(() =>
      expect(overview.result.current.preferences.size).toBe(1),
    );

    await act(async () => {
      await overview.result.current.setSpacePreference('main', {
        pinned: true,
      });
    });

    expect(apiMocks.updateSpacePreference).toHaveBeenCalledWith(
      testApi,
      'main',
      { pinned: true },
      'workspace-1',
    );
    expect(sidebar.result.current.preferences.get('main')?.pinned).toBe(true);
    expect(overview.result.current.preferences.get('main')?.pinned).toBe(true);
  });

  it('drops a space back to the default placement', async () => {
    apiMocks.updateSpacePreference.mockResolvedValue({
      hidden: false,
      pinned: false,
      space_id: 'reference',
    });
    const { result } = renderHook(() => usePmsSpacePreferences('workspace-1'));
    await waitFor(() => expect(result.current.preferences.size).toBe(1));

    await act(async () => {
      await result.current.setSpacePreference('reference', { hidden: false });
    });

    expect(result.current.preferences.has('reference')).toBe(false);
  });

  it('keeps a change saved while the first load was still on its way', async () => {
    let resolveLoad: ((value: unknown) => void) | undefined;
    apiMocks.listSpacePreferences.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveLoad = resolve;
        }),
    );
    apiMocks.updateSpacePreference.mockResolvedValue({
      hidden: false,
      pinned: true,
      space_id: 'main',
    });
    const { result } = renderHook(() => usePmsSpacePreferences('workspace-1'));

    await act(async () => {
      await result.current.setSpacePreference('main', { pinned: true });
    });
    // The snapshot was taken before the save and does not know about it.
    await act(async () => {
      resolveLoad?.({
        items: [{ hidden: true, pinned: false, space_id: 'reference' }],
      });
    });

    expect(result.current.preferences.get('main')?.pinned).toBe(true);
    expect(result.current.preferences.get('reference')?.hidden).toBe(true);
  });
  it('forgets the previous workspace placement when the next one fails to load', async () => {
    const { rerender, result } = renderHook(
      ({ workspaceSlug }) => usePmsSpacePreferences(workspaceSlug),
      { initialProps: { workspaceSlug: 'workspace-1' } },
    );
    await waitFor(() => expect(result.current.preferences.size).toBe(1));

    apiMocks.listSpacePreferences.mockRejectedValue(new Error('offline'));
    rerender({ workspaceSlug: 'workspace-2' });

    await waitFor(() =>
      expect(apiMocks.listSpacePreferences).toHaveBeenLastCalledWith(
        testApi,
        'workspace-2',
      ),
    );
    expect(result.current.preferences.size).toBe(0);
  });
});
