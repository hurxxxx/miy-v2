import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import { reorderTaskListTasks, reorderTaskListTasksByTitle } from './pms-api';

// miy: `runtime.api` with app-relative paths. AI-DO rewrote the path into the
// selected workspace (`/api/v1/workspaces/hq/pms/...`); miy has one PMS per
// installation and ignores the workspace argument.
const asApi = (mock: ReturnType<typeof vi.fn>) => mock as unknown as AppApi;

describe('PMS task reorder', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('uses one batch request', async () => {
    const fetchSpy = vi.fn().mockResolvedValue({ items: [] });

    await reorderTaskListTasks(
      asApi(fetchSpy),
      'list-1',
      {
        items: [
          {
            board_position: 1000,
            parent_id: null,
            task_id: 'task-1',
          },
        ],
      },
      'hq',
    );

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe(
      '/v1/pms/lists/list-1/tasks/reorder',
    );
    expect(fetchSpy.mock.calls[0]?.[1]).toMatchObject({ method: 'PATCH' });
  });

  it('saves name order for the whole list in the requested direction', async () => {
    const fetchSpy = vi.fn().mockResolvedValue({ updated_count: 3 });

    await expect(
      reorderTaskListTasksByTitle(asApi(fetchSpy), 'list-1', 'desc', 'hq'),
    ).resolves.toEqual({ updated_count: 3 });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe(
      '/v1/pms/lists/list-1/tasks/reorder-by-title',
    );
    expect(fetchSpy.mock.calls[0]?.[1]).toMatchObject({
      body: JSON.stringify({ sort_dir: 'desc' }),
      method: 'PATCH',
    });
  });
});
