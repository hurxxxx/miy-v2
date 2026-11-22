import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';

import { listAllPmsTaskLists, listAllTodayOverdueTasks } from '../api/pms-api';
import { loadTodayOverdueTasks } from './TodayOverdueView';

vi.mock('../api/pms-api', () => ({
  listAllPmsTaskLists: vi.fn(),
  listAllTodayOverdueTasks: vi.fn(),
}));

const listAllPmsTaskListsMock = vi.mocked(listAllPmsTaskLists);
// miy: the screens pass `runtime.api` where AI-DO passed the bearer token.
const api = vi.fn() as unknown as AppApi;
const listAllTodayOverdueTasksMock = vi.mocked(listAllTodayOverdueTasks);

describe('loadTodayOverdueTasks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads task lists and assigned due tasks in the requested workspace', async () => {
    const firstTask = { id: 'task-1' };
    const secondTask = { id: 'task-2' };
    const firstList = { id: 'list-1' };
    const secondList = { id: 'list-2' };
    listAllPmsTaskListsMock.mockResolvedValue({
      items: [firstList, secondList],
    } as never);
    listAllTodayOverdueTasksMock.mockResolvedValue({
      items: [firstTask, secondTask],
    } as never);

    await expect(
      loadTodayOverdueTasks(api, '2026-07-09', 'delivery-hub'),
    ).resolves.toEqual({
      taskLists: [firstList, secondList],
      tasks: [firstTask, secondTask],
    });

    expect(listAllPmsTaskListsMock).toHaveBeenCalledWith(
      api,
      undefined,
      'delivery-hub',
    );
    expect(listAllTodayOverdueTasksMock).toHaveBeenCalledWith(
      api,
      '2026-07-09',
      'delivery-hub',
    );
  });
});
