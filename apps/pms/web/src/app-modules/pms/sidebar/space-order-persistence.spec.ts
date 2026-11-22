import { describe, expect, it, vi } from 'vitest';

import { reorderPmsTaskLists } from '../api/pms-api';
import type { PmsTaskList } from '../api/pms-api';
import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  buildSpaceOrderChanges,
  persistSpaceOrderChanges,
} from './space-order-persistence';

vi.mock('../api/pms-api', async (original) => ({
  ...(await original<typeof import('../api/pms-api')>()),
  reorderPmsTaskLists: vi.fn().mockResolvedValue(undefined),
}));

describe('space order persistence', () => {
  it('saves list order in the same explicit owner space', async () => {
    const lists = [{ id: 'list-a', folder_id: null, sort_order: 2 }];
    const api = vi.fn() as unknown as AppApi;
    await persistSpaceOrderChanges({
      api,
      workspaceSlug: 'pms-owner',
      spaceId: 'space-a',
      changes: {
        listChanges: lists,
      },
    });
    expect(reorderPmsTaskLists).toHaveBeenCalledWith(
      api,
      'space-a',
      { items: lists },
      'pms-owner',
    );
  });
  it('builds only changed list placement patches', () => {
    const changes = buildSpaceOrderChanges({
      currentLists: [
        taskList({ id: 'list-1', folder_id: null, sort_order: 1 }),
        taskList({ id: 'list-2', folder_id: 'folder-1', sort_order: 2 }),
        taskList({
          id: 'list-archived',
          archived: true,
          folder_id: null,
          sort_order: 1,
        }),
      ],
      payload: {
        lists: [
          {
            id: 'list-1',
            name: 'List 1',
            folder_id: 'folder-1',
            sort_order: 3,
            active_task_count: 0,
          },
          {
            id: 'list-2',
            name: 'List 2',
            folder_id: 'folder-1',
            sort_order: 2,
            active_task_count: 0,
          },
          {
            id: 'list-archived',
            name: 'Archived list',
            folder_id: null,
            sort_order: 3,
            active_task_count: 0,
          },
        ],
      },
    });

    expect(changes).toEqual({
      listChanges: [{ id: 'list-1', folder_id: 'folder-1', sort_order: 3 }],
    });
  });
});

function taskList(overrides: Partial<PmsTaskList>): PmsTaskList {
  return {
    id: 'list-1',
    name: 'List',
    team_id: 'space-1',
    team_name: 'Space',
    folder_id: null,
    folder_name: null,
    sort_order: 1,
    task_count: 0,
    progress: 0,
    updated_at: '2026-01-01T00:00:00Z',
    created_at: '2026-01-01T00:00:00Z',
    status_mode: 'inherit',
    ...overrides,
  } as PmsTaskList;
}
