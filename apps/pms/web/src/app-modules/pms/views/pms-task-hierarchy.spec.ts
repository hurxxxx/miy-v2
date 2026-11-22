import { describe, expect, it } from 'vitest';

import type { PmsTask } from '../api/pms-api';
import { pmsTaskFixture } from '../test-fixtures';
import {
  buildTaskHierarchy,
  getTaskBoardPositionUpdates,
  getTaskGroupBoardPositionUpdates,
  getTaskRoot,
  sortTasksByHierarchy,
  sortTasksByHierarchyUsingInputOrder,
} from './pms-task-hierarchy';

function task(
  id: string,
  title: string,
  taskNumber: number,
  parentId: string | null = null,
  status = 'todo',
): PmsTask {
  return pmsTaskFixture({
    id,
    title,
    reference: `PMS-${taskNumber}`,
    board_position: taskNumber,
    parent_id: parentId,
    status,
  });
}

describe('pms task hierarchy', () => {
  it('calculates depth, direct parent, child counts, and hierarchy order', () => {
    const root = task('root', 'Root task', 1);
    const child = task('child', 'Child task', 2, root.id);
    const grandchild = task('grandchild', 'Grandchild task', 3, child.id);
    const sibling = task('sibling', 'Sibling task', 4, root.id);

    const hierarchy = buildTaskHierarchy([grandchild, sibling, child, root]);

    expect(hierarchy.get(root.id)).toMatchObject({
      childCount: 2,
      depth: 0,
      parent: null,
      sortKey: [1],
    });
    expect(hierarchy.get(child.id)).toMatchObject({
      childCount: 1,
      depth: 1,
      parent: root,
      sortKey: [1, 2],
    });
    expect(hierarchy.get(grandchild.id)).toMatchObject({
      childCount: 0,
      depth: 2,
      parent: child,
      sortKey: [1, 2, 3],
    });

    expect(sortTasksByHierarchy([grandchild, sibling, child, root])).toEqual([
      root,
      child,
      grandchild,
      sibling,
    ]);
  });

  it('resolves subtasks to the top-level task for status grouping', () => {
    const root = task('root', 'Root task', 1, null, 'in_progress');
    const child = task('child', 'Child task', 2, root.id, 'todo');
    const grandchild = task(
      'grandchild',
      'Grandchild task',
      3,
      child.id,
      'done',
    );
    const todoRoot = task('todo-root', 'Todo root task', 4, null, 'todo');
    const tasks = [grandchild, todoRoot, child, root];
    const hierarchy = buildTaskHierarchy(tasks);

    const inProgressGroup = sortTasksByHierarchy(tasks).filter(
      (item) => getTaskRoot(item, hierarchy).status === 'in_progress',
    );
    const inProgressRootCount = inProgressGroup.filter(
      (item) => getTaskRoot(item, hierarchy).id === item.id,
    ).length;

    expect(inProgressGroup).toEqual([root, child, grandchild]);
    expect(inProgressRootCount).toBe(1);
    expect(getTaskRoot(todoRoot, hierarchy)).toBe(todoRoot);
  });

  it('preserves server date order while keeping descendants under their parent', () => {
    const firstRoot = task('first-root', 'First root', 1);
    const firstChild = task('first-child', 'First child', 2, firstRoot.id);
    const secondRoot = task('second-root', 'Second root', 3);
    const secondChild = task('second-child', 'Second child', 4, secondRoot.id);

    expect(
      sortTasksByHierarchyUsingInputOrder([
        secondChild,
        secondRoot,
        firstChild,
        firstRoot,
      ]),
    ).toEqual([secondRoot, secondChild, firstRoot, firstChild]);
  });

  it('calculates a single board position update when a reorder gap exists', () => {
    const first = task('first', 'First task', 1000);
    const second = task('second', 'Second task', 3000);
    const third = task('third', 'Third task', 5000);

    expect(
      getTaskBoardPositionUpdates({
        sourceTaskId: third.id,
        targetTaskId: second.id,
        tasks: [first, second, third],
        zone: 'before',
      }),
    ).toEqual([{ boardPosition: 2000, taskId: third.id }]);
  });

  it('rebalances siblings when adjacent board positions leave no gap', () => {
    const first = task('first', 'First task', 1);
    const second = task('second', 'Second task', 2);
    const third = task('third', 'Third task', 3);

    expect(
      getTaskBoardPositionUpdates({
        sourceTaskId: third.id,
        targetTaskId: first.id,
        tasks: [first, second, third],
        zone: 'before',
      }),
    ).toEqual([
      { boardPosition: 1000, taskId: third.id },
      { boardPosition: 2000, taskId: first.id },
      { boardPosition: 3000, taskId: second.id },
    ]);
  });

  it('rejects cross-status sibling reorder while grouped by status', () => {
    const todoRoot = task('todo-root', 'Todo root', 1000, null, 'todo');
    const activeRoot = task('active-root', 'Active root', 2000, null, 'doing');

    expect(
      getTaskBoardPositionUpdates({
        groupBy: 'status',
        sourceTaskId: todoRoot.id,
        targetTaskId: activeRoot.id,
        tasks: [todoRoot, activeRoot],
        zone: 'after',
      }),
    ).toEqual([]);
  });

  it('allows cross-status hierarchy moves while grouped by status', () => {
    const todoRoot = task('todo-root', 'Todo root', 1000, null, 'todo');
    const activeRoot = task('active-root', 'Active root', 2000, null, 'doing');

    expect(
      getTaskBoardPositionUpdates({
        groupBy: 'status',
        sourceTaskId: todoRoot.id,
        targetTaskId: activeRoot.id,
        tasks: [todoRoot, activeRoot],
        zone: 'inside',
      }),
    ).toEqual([
      { boardPosition: 1000, parentId: activeRoot.id, taskId: todoRoot.id },
    ]);
  });

  it('rejects cross-assignee sibling reorder while grouped by assignee', () => {
    const first = {
      ...task('first', 'First task', 1000),
      assignee_id: 'user-1',
      assignee_ids: ['user-1'],
    };
    const second = {
      ...task('second', 'Second task', 2000),
      assignee_id: 'user-2',
      assignee_ids: ['user-2'],
    };

    expect(
      getTaskBoardPositionUpdates({
        groupBy: 'assignee',
        sourceTaskId: first.id,
        targetTaskId: second.id,
        tasks: [first, second],
        zone: 'after',
      }),
    ).toEqual([]);
  });

  it('allows reorder within the same assignee set regardless of assignment order', () => {
    const first = {
      ...task('first', 'First task', 1000),
      assignee_id: 'user-1',
      assignee_ids: ['user-1', 'user-2'],
    };
    const second = {
      ...task('second', 'Second task', 2000),
      assignee_id: 'user-2',
      assignee_ids: ['user-2', 'user-1'],
    };

    expect(
      getTaskBoardPositionUpdates({
        groupBy: 'assignee',
        sourceTaskId: first.id,
        targetTaskId: second.id,
        tasks: [first, second],
        zone: 'after',
      }),
    ).toEqual([{ boardPosition: 3000, taskId: first.id }]);
  });

  it('keeps a reorder inside its label group', () => {
    const leave = {
      ...task('leave', 'Leave', 1000),
      labels: [{ color: '#22c55e', id: 'leave', name: '휴가' }],
    };
    const trip = {
      ...task('trip', 'Trip', 2000),
      labels: [{ color: '#3b82f6', id: 'trip', name: '출장' }],
    };
    const otherLeave = {
      ...task('other-leave', 'Other leave', 3000),
      labels: [{ color: '#22c55e', id: 'leave', name: '휴가' }],
    };

    expect(
      getTaskBoardPositionUpdates({
        groupBy: 'label',
        sourceTaskId: leave.id,
        targetTaskId: trip.id,
        tasks: [leave, trip, otherLeave],
        zone: 'after',
      }),
    ).toEqual([]);
    expect(
      getTaskBoardPositionUpdates({
        groupBy: 'label',
        sourceTaskId: leave.id,
        targetTaskId: otherLeave.id,
        tasks: [leave, trip, otherLeave],
        zone: 'after',
      }),
    ).toEqual([{ boardPosition: 4000, taskId: leave.id }]);
  });

  it('moves a task under another task when dropped inside the target', () => {
    const first = task('first', 'First task', 1000);
    const second = task('second', 'Second task', 2000);
    const existingChild = task(
      'existing-child',
      'Existing child',
      1000,
      second.id,
    );

    expect(
      getTaskBoardPositionUpdates({
        sourceTaskId: first.id,
        targetTaskId: second.id,
        tasks: [first, second, existingChild],
        zone: 'inside',
      }),
    ).toEqual([{ boardPosition: 2000, parentId: second.id, taskId: first.id }]);
  });

  it('moves a child back to its parent level after the current parent', () => {
    const parent = task('parent', 'Parent task', 1000);
    const nextRoot = task('next-root', 'Next root', 2000);
    const child = task('child', 'Child task', 1000, parent.id);

    expect(
      getTaskBoardPositionUpdates({
        sourceTaskId: child.id,
        targetTaskId: parent.id,
        tasks: [parent, nextRoot, child],
        zone: 'after',
      }),
    ).toEqual([{ boardPosition: 1500, parentId: null, taskId: child.id }]);
  });

  it('rejects moves into the source task subtree', () => {
    const parent = task('parent', 'Parent task', 1000);
    const child = task('child', 'Child task', 1000, parent.id);
    const grandchild = task('grandchild', 'Grandchild task', 1000, child.id);

    expect(
      getTaskBoardPositionUpdates({
        sourceTaskId: parent.id,
        targetTaskId: grandchild.id,
        tasks: [parent, child, grandchild],
        zone: 'inside',
      }),
    ).toEqual([]);
  });
});

describe('moving several selected tasks at once', () => {
  it('moves the selected tasks after the target in list order', () => {
    const a = task('a', 'A', 1000);
    const b = task('b', 'B', 2000);
    const c = task('c', 'C', 3000);

    expect(
      getTaskGroupBoardPositionUpdates({
        sourceTaskIds: [c.id, a.id],
        targetTaskId: b.id,
        tasks: [a, b, c],
        zone: 'after',
      }),
    ).toEqual([
      { boardPosition: 3000, taskId: a.id },
      { boardPosition: 4000, taskId: c.id },
    ]);
  });

  it('fits the block into the gap before the target', () => {
    const a = task('a', 'A', 1000);
    const b = task('b', 'B', 2000);
    const c = task('c', 'C', 3000);
    const d = task('d', 'D', 4000);
    const e = task('e', 'E', 5000);

    expect(
      getTaskGroupBoardPositionUpdates({
        sourceTaskIds: [d.id, e.id],
        targetTaskId: b.id,
        tasks: [a, b, c, d, e],
        zone: 'before',
      }),
    ).toEqual([
      { boardPosition: 1333, taskId: d.id },
      { boardPosition: 1666, taskId: e.id },
    ]);
  });

  it('fits the block before the first sibling using the gap down to 0', () => {
    const first = task('first', 'First', 3000);
    const b = task('b', 'B', 4000);
    const c = task('c', 'C', 5000);

    expect(
      getTaskGroupBoardPositionUpdates({
        sourceTaskIds: [b.id, c.id],
        targetTaskId: first.id,
        tasks: [first, b, c],
        zone: 'before',
      }),
    ).toEqual([
      { boardPosition: 1000, taskId: b.id },
      { boardPosition: 2000, taskId: c.id },
    ]);
  });

  it('renumbers the siblings when the gap cannot hold the block', () => {
    const a = task('a', 'A', 1);
    const b = task('b', 'B', 2);
    const c = task('c', 'C', 3);
    const d = task('d', 'D', 4);

    expect(
      getTaskGroupBoardPositionUpdates({
        sourceTaskIds: [c.id, d.id],
        targetTaskId: b.id,
        tasks: [a, b, c, d],
        zone: 'before',
      }),
    ).toEqual([
      { boardPosition: 1000, taskId: a.id },
      { boardPosition: 2000, taskId: c.id },
      { boardPosition: 3000, taskId: d.id },
      { boardPosition: 4000, taskId: b.id },
    ]);
  });

  it('leaves a selected subtask under its selected parent', () => {
    const parent = task('parent', 'Parent', 1000);
    const child = task('child', 'Child', 1000, parent.id);
    const other = task('other', 'Other', 2000);
    const target = task('target', 'Target', 3000);

    expect(
      getTaskGroupBoardPositionUpdates({
        sourceTaskIds: [parent.id, child.id, other.id],
        targetTaskId: target.id,
        tasks: [parent, child, other, target],
        zone: 'after',
      }),
    ).toEqual([
      { boardPosition: 4000, taskId: parent.id },
      { boardPosition: 5000, taskId: other.id },
    ]);
  });

  it('puts the block under the target when dropped inside it', () => {
    const a = task('a', 'A', 1000);
    const b = task('b', 'B', 2000);
    const target = task('target', 'Target', 3000);
    const existingChild = task(
      'existing-child',
      'Existing child',
      1000,
      target.id,
    );

    expect(
      getTaskGroupBoardPositionUpdates({
        sourceTaskIds: [a.id, b.id],
        targetTaskId: target.id,
        tasks: [a, b, target, existingChild],
        zone: 'inside',
      }),
    ).toEqual([
      { boardPosition: 2000, parentId: target.id, taskId: a.id },
      { boardPosition: 3000, parentId: target.id, taskId: b.id },
    ]);
  });

  it('rejects a drop on a selected task or inside a moving subtree', () => {
    const parent = task('parent', 'Parent', 1000);
    const child = task('child', 'Child', 1000, parent.id);
    const other = task('other', 'Other', 2000);
    const tasks = [parent, child, other];

    for (const targetTaskId of [other.id, child.id]) {
      expect(
        getTaskGroupBoardPositionUpdates({
          sourceTaskIds: [parent.id, other.id],
          targetTaskId,
          tasks,
          zone: 'inside',
        }),
      ).toEqual([]);
    }
  });

  it('keeps the block inside one status group', () => {
    const todo = task('todo', 'Todo', 1000, null, 'todo');
    const doing = task('doing', 'Doing', 2000, null, 'doing');
    const todoTarget = task('todo-target', 'Todo target', 3000, null, 'todo');
    const todoOther = task('todo-other', 'Todo other', 4000, null, 'todo');
    const tasks = [todo, doing, todoTarget, todoOther];

    for (const zone of ['after', 'inside'] as const) {
      expect(
        getTaskGroupBoardPositionUpdates({
          groupBy: 'status',
          sourceTaskIds: [todo.id, doing.id],
          targetTaskId: todoTarget.id,
          tasks,
          zone,
        }),
      ).toEqual([]);
    }
    expect(
      getTaskGroupBoardPositionUpdates({
        groupBy: 'status',
        sourceTaskIds: [todo.id, todoOther.id],
        targetTaskId: todoTarget.id,
        tasks,
        zone: 'before',
      }),
    ).toEqual([
      { boardPosition: 2333, taskId: todo.id },
      { boardPosition: 2666, taskId: todoOther.id },
    ]);
  });

  it('moves a single selected task exactly like a plain drag', () => {
    const first = task('first', 'First task', 1000);
    const second = task('second', 'Second task', 3000);
    const third = task('third', 'Third task', 5000);
    const input = {
      targetTaskId: second.id,
      tasks: [first, second, third],
      zone: 'before' as const,
    };

    expect(
      getTaskGroupBoardPositionUpdates({ ...input, sourceTaskIds: [third.id] }),
    ).toEqual(
      getTaskBoardPositionUpdates({ ...input, sourceTaskId: third.id }),
    );
  });
});
