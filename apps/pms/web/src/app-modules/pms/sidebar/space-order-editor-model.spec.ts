import { describe, expect, it } from 'vitest';

import type { PmsFolder, PmsTaskList } from '../api/pms-api';
import {
  createSpaceOrderDraft,
  createSpaceOrderSavePayload,
  getRootSpaceOrderDraftLists,
  getSpaceOrderDraftListsForParent,
  moveSpaceOrderDraftListParent,
  moveSpaceOrderDraftListStep,
  sortSpaceOrderFolders,
  summarizeSpaceOrderDraftChanges,
} from './space-order-editor-model';

function list(
  id: string,
  name: string,
  sortOrder: number,
  folderId: string | null = null,
): PmsTaskList {
  return {
    id,
    team_id: 'space-1',
    team_name: 'Space 1',
    folder_id: folderId,
    folder_name: null,
    key: id,
    name,
    role: 'admin',
    sort_order: sortOrder,
    status_mode: 'inherit',
    task_count: 3,
    // One of the three tasks is archived.
    active_task_count: 2,
    updated_at: '2026-05-30T00:00:00Z',
  } as PmsTaskList;
}

function folder(id: string, name: string, sortOrder: number): PmsFolder {
  return {
    id,
    team_id: 'space-1',
    name,
    sort_order: sortOrder,
    list_count: 0,
  };
}

describe('space order editor model', () => {
  it('creates a draft and exposes localized ordered views', () => {
    const draft = createSpaceOrderDraft({
      lists: [
        list('list-c', 'Charlie', 2000),
        list('list-b', 'Bravo', 1000, 'folder-b'),
        list('list-a', 'Alpha', 1000),
        { ...list('list-archived', 'Archived', 0), archived: true },
      ],
    });

    expect(draft.lists).toEqual([
      {
        id: 'list-c',
        name: 'Charlie',
        folder_id: null,
        sort_order: 2000,
        active_task_count: 2,
      },
      {
        id: 'list-b',
        name: 'Bravo',
        folder_id: 'folder-b',
        sort_order: 1000,
        active_task_count: 2,
      },
      {
        id: 'list-a',
        name: 'Alpha',
        folder_id: null,
        sort_order: 1000,
        active_task_count: 2,
      },
    ]);
    expect(
      sortSpaceOrderFolders(
        [folder('folder-b', 'Bravo', 1000), folder('folder-a', 'Alpha', 1000)],
        'en-US',
      ).map((item) => item.id),
    ).toEqual(['folder-a', 'folder-b']);
    expect(
      getRootSpaceOrderDraftLists(draft, 'en-US').map((item) => item.id),
    ).toEqual(['list-a', 'list-c']);
    expect(
      getSpaceOrderDraftListsForParent(draft, 'folder-b', 'en-US').map(
        (item) => item.id,
      ),
    ).toEqual(['list-b']);
  });

  it('summarizes changed list parents and list sort orders', () => {
    const originalLists = [
      list('list-a', 'Alpha', 0),
      list('list-b', 'Bravo', 1000),
    ];
    const draft = createSpaceOrderDraft({
      lists: originalLists,
    });

    const changedDraft = {
      lists: [
        { ...draft.lists[0], sort_order: 1000 },
        { ...draft.lists[1], folder_id: 'folder-a' },
      ],
    };

    expect(
      summarizeSpaceOrderDraftChanges({
        draft: changedDraft,
        originalLists,
      }),
    ).toEqual({
      changedListCount: 2,
      changedCount: 2,
      isDirty: true,
    });
  });

  it('moves lists within their current parent and preserves edge no-ops', () => {
    const draft = createSpaceOrderDraft({
      lists: [
        list('list-a', 'Alpha', 0),
        list('list-b', 'Bravo', 1000),
        list('list-c', 'Charlie', 0, 'folder-a'),
      ],
    });

    const moved = moveSpaceOrderDraftListStep(draft, 'list-b', 'up');

    expect(
      getRootSpaceOrderDraftLists(moved, 'en-US').map((item) => item.id),
    ).toEqual(['list-b', 'list-a']);
    expect(moved.lists.find((item) => item.id === 'list-b')?.sort_order).toBe(
      0,
    );
    expect(moved.lists.find((item) => item.id === 'list-a')?.sort_order).toBe(
      1000,
    );
    expect(moveSpaceOrderDraftListStep(draft, 'list-a', 'up')).toBe(draft);
  });

  it('moves a list to another parent and appends it to that parent', () => {
    const draft = createSpaceOrderDraft({
      lists: [
        list('root', 'Root', 0),
        list('foldered', 'Foldered', 0, 'folder-a'),
      ],
    });

    const moved = moveSpaceOrderDraftListParent(draft, 'root', 'folder-a');

    expect(
      getSpaceOrderDraftListsForParent(moved, 'folder-a', 'en-US').map(
        (item) => [item.id, item.sort_order],
      ),
    ).toEqual([
      ['foldered', 0],
      ['root', 1000],
    ]);
    expect(moved.lists.find((item) => item.id === 'root')?.folder_id).toBe(
      'folder-a',
    );
  });

  it('creates an isolated save payload from the current draft', () => {
    const draft = createSpaceOrderDraft({
      lists: [list('list-a', 'Alpha', 0)],
    });

    const payload = createSpaceOrderSavePayload(draft);

    expect(payload).toEqual(draft);
    expect(payload.lists[0]).not.toBe(draft.lists[0]);
  });
});
