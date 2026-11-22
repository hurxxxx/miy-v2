import { describe, expect, it } from 'vitest';

import type { PmsTask } from '../api/pms-api';
import {
  buildClearAssigneesPatch,
  buildDatePatch,
  buildToggleAssigneePatch,
  canDropTaskInList,
  dropZoneFromPointer,
  selectedAssigneeIds,
} from './list-view-editing-model';

function task(overrides: Partial<PmsTask> = {}): PmsTask {
  return {
    id: 'task-1',
    title: 'Task',
    task_number: 1,
    board_position: 1000,
    parent_id: null,
    status: 'todo',
    assignee_id: null,
    assignee_ids: [],
    start_date: null,
    due_date: null,
    ...overrides,
  } as PmsTask;
}

describe('list view editing model', () => {
  it('builds date patches that preserve start/due invariants', () => {
    expect(
      buildDatePatch(
        task({ due_date: '2026-05-30' }),
        'start_date',
        '2026-06-01',
      ),
    ).toEqual({
      due_date: '2026-06-01',
      start_date: '2026-06-01',
    });
    expect(
      buildDatePatch(
        task({ start_date: '2026-06-01' }),
        'due_date',
        '2026-05-30',
      ),
    ).toEqual({
      due_date: '2026-05-30',
      start_date: '2026-05-30',
    });
    expect(buildDatePatch(task(), 'due_date', '')).toEqual({
      due_date: null,
    });
    expect(buildDatePatch(task(), 'completed_date', '2026-06-18')).toEqual({
      completed_date: '2026-06-18',
    });
    expect(buildDatePatch(task(), 'completed_date', '')).toEqual({
      completed_date: null,
    });
  });

  it('selects and toggles assignee patch payloads', () => {
    const singleAssignee = task({ assignee_id: 'user-1', assignee_ids: [] });
    expect(selectedAssigneeIds(singleAssignee)).toEqual(['user-1']);

    expect(buildToggleAssigneePatch(singleAssignee, 'user-2')).toEqual({
      assignee_id: 'user-1',
      assignee_ids: ['user-1', 'user-2'],
    });
    expect(
      buildToggleAssigneePatch(
        task({ assignee_id: 'user-1', assignee_ids: ['user-1', 'user-2'] }),
        'user-1',
      ),
    ).toEqual({
      assignee_id: 'user-2',
      assignee_ids: ['user-2'],
    });
    expect(buildClearAssigneesPatch()).toEqual({
      assignee_id: null,
      assignee_ids: [],
    });
  });

  it('accepts hierarchy drops while preserving visible status groups', () => {
    const todoRoot = task({ id: 'todo-root', status: 'todo' });
    const activeRoot = task({
      id: 'active-root',
      board_position: 2000,
      status: 'active',
    });
    const todoChild = task({
      id: 'todo-child',
      parent_id: todoRoot.id,
      status: 'todo',
    });
    const activeChild = task({
      id: 'active-child',
      parent_id: activeRoot.id,
      status: 'todo',
    });
    const todoSibling = task({
      id: 'todo-sibling',
      board_position: 2000,
      status: 'todo',
    });

    expect(
      canDropTaskInList({
        groupBy: 'none',
        sourceTaskIds: [todoChild.id],
        targetTask: activeChild,
        tasks: [todoRoot, activeRoot, todoChild, activeChild],
        zone: 'after',
      }),
    ).toBe(true);
    expect(
      canDropTaskInList({
        groupBy: 'none',
        sourceTaskIds: [todoSibling.id],
        targetTask: todoRoot,
        tasks: [todoRoot, todoSibling],
        zone: 'inside',
      }),
    ).toBe(true);
    expect(
      canDropTaskInList({
        groupBy: 'status',
        sourceTaskIds: [todoRoot.id],
        targetTask: activeRoot,
        tasks: [todoRoot, activeRoot, todoChild, activeChild],
        zone: 'after',
      }),
    ).toBe(false);
    expect(
      canDropTaskInList({
        groupBy: 'none',
        sourceTaskIds: [todoRoot.id],
        targetTask: activeRoot,
        tasks: [todoRoot, activeRoot, todoChild, activeChild],
        zone: 'after',
      }),
    ).toBe(true);
  });

  it('refuses a drop that would send more rows than the reorder API takes', () => {
    const target = task({ id: 'target', board_position: 999_999_999 });
    const canDropAfterTarget = (count: number) => {
      const moving = Array.from({ length: count }, (_, index) =>
        task({ id: `moving-${index}`, board_position: (index + 1) * 1000 }),
      );
      return canDropTaskInList({
        groupBy: 'none',
        sourceTaskIds: moving.map((item) => item.id),
        targetTask: target,
        tasks: [...moving, target],
        zone: 'after',
      });
    };

    expect(canDropAfterTarget(500)).toBe(true);
    expect(canDropAfterTarget(501)).toBe(false);
  });

  it('refuses a spot that keeps the order even when positions are uneven', () => {
    // API-created tasks get positions 1, 2, 3; a moved one lands on a gap value.
    const first = task({ board_position: 1, id: 'first' });
    const second = task({ board_position: 2, id: 'second' });
    const third = task({ board_position: 4500, id: 'third' });
    const fourth = task({ board_position: 5000, id: 'fourth' });
    const child = task({ board_position: 1, id: 'child', parent_id: 'first' });
    const tasks = [first, second, third, fourth, child];
    const canDrop = (
      sourceTaskIds: string[],
      targetTask: PmsTask,
      zone: 'after' | 'before',
    ) =>
      canDropTaskInList({
        groupBy: 'none',
        sourceTaskIds,
        targetTask,
        tasks,
        zone,
      });

    expect(canDrop(['first'], second, 'before')).toBe(false);
    expect(canDrop(['second'], first, 'after')).toBe(false);
    expect(canDrop(['third'], fourth, 'before')).toBe(false);
    expect(canDrop(['third'], second, 'after')).toBe(false);
    expect(canDrop(['second', 'third'], fourth, 'before')).toBe(false);
    expect(canDrop(['first'], third, 'before')).toBe(true);
    expect(canDrop(['second'], third, 'after')).toBe(true);
    // Moving out from under a parent changes the parent even next to it.
    expect(canDrop(['child'], second, 'before')).toBe(true);
    // A selected subtask moves with its selected parent.
    expect(canDrop(['first', 'child'], second, 'before')).toBe(false);
  });

  it('refuses dropping the last child inside its own parent', () => {
    const parent = task({ board_position: 1000, id: 'parent' });
    const firstChild = task({
      board_position: 1,
      id: 'first-child',
      parent_id: 'parent',
    });
    const lastChild = task({
      board_position: 2,
      id: 'last-child',
      parent_id: 'parent',
    });
    const tasks = [parent, firstChild, lastChild];
    const canDropInside = (sourceTaskId: string) =>
      canDropTaskInList({
        groupBy: 'none',
        sourceTaskIds: [sourceTaskId],
        targetTask: parent,
        tasks,
        zone: 'inside',
      });

    expect(canDropInside('last-child')).toBe(false);
    expect(canDropInside('first-child')).toBe(true);
  });

  it('chooses the drop zone from pointer position', () => {
    const rect = { height: 40, top: 100 };

    expect(dropZoneFromPointer(rect, 110)).toBe('before');
    expect(dropZoneFromPointer(rect, 121)).toBe('inside');
    expect(dropZoneFromPointer(rect, 132)).toBe('after');
  });
});
