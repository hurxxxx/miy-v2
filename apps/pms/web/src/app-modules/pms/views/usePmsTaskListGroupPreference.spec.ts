import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { usePmsTaskListGroupPreference } from './usePmsTaskListGroupPreference';

const apiMocks = vi.hoisted(() => ({
  getPmsViewPreferences: vi.fn(),
  updatePmsViewPreferences: vi.fn(),
}));

// miy: the hook reads `runtime.api` where AI-DO read the bearer token.
const testApi = vi.hoisted(() => vi.fn());
vi.mock('../../../platform/runtime/runtime-context', () => ({
  useAppApi: () => testApi,
}));

vi.mock('../api/pms-api', () => apiMocks);

describe('usePmsTaskListGroupPreference', () => {
  beforeEach(() => {
    apiMocks.getPmsViewPreferences.mockReset();
    apiMocks.updatePmsViewPreferences.mockReset();
    apiMocks.getPmsViewPreferences.mockResolvedValue({
      task_list_group_by: 'status',
    });
    apiMocks.updatePmsViewPreferences.mockImplementation(
      async (_api: unknown, preference: { task_list_group_by: string }) =>
        preference,
    );
  });

  it('loads the account preference for the current workspace', async () => {
    apiMocks.getPmsViewPreferences.mockResolvedValue({
      task_list_group_by: 'assignee',
    });
    const { result } = renderHook(() =>
      usePmsTaskListGroupPreference({
        enabled: true,
        workspaceSlug: 'workspace-1',
      }),
    );

    await waitFor(() => expect(result.current.groupBy).toBe('assignee'));
    expect(apiMocks.getPmsViewPreferences).toHaveBeenCalledWith(
      testApi,
      'workspace-1',
    );
  });

  it('serializes rapid changes so the last selection is persisted last', async () => {
    let resolveFirstSave:
      | ((value: { task_list_group_by: 'assignee' }) => void)
      | undefined;
    apiMocks.updatePmsViewPreferences
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirstSave = resolve;
          }),
      )
      .mockResolvedValueOnce({ task_list_group_by: 'none' });
    const { result } = renderHook(() =>
      usePmsTaskListGroupPreference({
        enabled: true,
        workspaceSlug: 'workspace-1',
      }),
    );
    await waitFor(() =>
      expect(apiMocks.getPmsViewPreferences).toHaveBeenCalledTimes(1),
    );

    act(() => {
      result.current.setGroupBy('assignee');
      result.current.setGroupBy('none');
    });
    await waitFor(() =>
      expect(apiMocks.updatePmsViewPreferences).toHaveBeenCalledTimes(1),
    );
    expect(result.current.groupBy).toBe('none');

    resolveFirstSave?.({ task_list_group_by: 'assignee' });
    await waitFor(() =>
      expect(apiMocks.updatePmsViewPreferences).toHaveBeenCalledTimes(2),
    );
    expect(apiMocks.updatePmsViewPreferences).toHaveBeenNthCalledWith(
      2,
      testApi,
      { task_list_group_by: 'none' },
      'workspace-1',
    );
  });
});
