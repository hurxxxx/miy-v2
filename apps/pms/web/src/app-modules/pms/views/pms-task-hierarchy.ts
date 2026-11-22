import type { PmsTask, PmsTaskListGroupBy } from '../api/pms-api';

export interface IssueHierarchyInfo {
  childCount: number;
  depth: number;
  parent: PmsTask | null;
  sortKey: number[];
}

export type TaskReorderZone = 'after' | 'before' | 'inside';

export interface TaskBoardPositionUpdate {
  boardPosition: number;
  parentId?: string | null;
  taskId: string;
}

/** Same cap as the reorder API (`TaskReorderRequest.items`, max_length=500). */
export const TASK_REORDER_MAX_ITEMS = 500;

export interface TaskReorderInput {
  groupBy?: PmsTaskListGroupBy;
  sourceTaskId: string;
  targetTaskId: string;
  tasks: PmsTask[];
  zone: TaskReorderZone;
}

function issueOrder(task: PmsTask): number {
  return task.board_position || 0;
}

function compareSortKey(left: number[], right: number[]): number {
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const leftValue = left[index] ?? -1;
    const rightValue = right[index] ?? -1;
    if (leftValue !== rightValue) return leftValue - rightValue;
  }
  return 0;
}

function isDescendantOf(
  task: PmsTask,
  ancestorId: string,
  tasksById: Map<string, PmsTask>,
): boolean {
  const visited = new Set<string>();
  let parentId = task.parent_id ?? null;

  while (parentId && !visited.has(parentId)) {
    if (parentId === ancestorId) return true;
    visited.add(parentId);
    parentId = tasksById.get(parentId)?.parent_id ?? null;
  }

  return false;
}

function taskParentId(task: PmsTask): string | null {
  return task.parent_id ?? null;
}

function getDestinationSiblings(
  tasks: PmsTask[],
  sourceTaskId: string,
  destinationParentId: string | null,
): PmsTask[] {
  return sortTasksByHierarchy(
    tasks.filter(
      (task) =>
        task.id !== sourceTaskId && taskParentId(task) === destinationParentId,
    ),
    tasks,
  );
}

function buildSiblingPositionUpdates({
  destinationParentId,
  insertIndex,
  sourceTask,
  tasks,
}: {
  destinationParentId: string | null;
  insertIndex: number;
  sourceTask: PmsTask;
  tasks: PmsTask[];
}): TaskBoardPositionUpdate[] {
  const parentChanged = taskParentId(sourceTask) !== destinationParentId;
  const destinationSiblings = getDestinationSiblings(
    tasks,
    sourceTask.id,
    destinationParentId,
  );
  const safeInsertIndex = Math.max(
    0,
    Math.min(insertIndex, destinationSiblings.length),
  );
  const nextSiblings = [
    ...destinationSiblings.slice(0, safeInsertIndex),
    sourceTask,
    ...destinationSiblings.slice(safeInsertIndex),
  ];
  const sourceIndex = nextSiblings.findIndex(
    (task) => task.id === sourceTask.id,
  );
  const previousTask = nextSiblings[sourceIndex - 1] ?? null;
  const nextTask = nextSiblings[sourceIndex + 1] ?? null;
  const previousPosition = previousTask?.board_position ?? null;
  const nextPosition = nextTask?.board_position ?? null;

  const sourceUpdate = (boardPosition: number): TaskBoardPositionUpdate[] => {
    if (!parentChanged && boardPosition === sourceTask.board_position)
      return [];
    return [
      {
        boardPosition,
        ...(parentChanged ? { parentId: destinationParentId } : {}),
        taskId: sourceTask.id,
      },
    ];
  };

  if (
    previousPosition !== null &&
    nextPosition !== null &&
    nextPosition - previousPosition > 1
  ) {
    return sourceUpdate(Math.floor((previousPosition + nextPosition) / 2));
  }

  if (previousPosition !== null && nextPosition === null) {
    return sourceUpdate(previousPosition + 1000);
  }

  if (
    previousPosition === null &&
    nextPosition !== null &&
    nextPosition > 1000
  ) {
    return sourceUpdate(nextPosition - 1000);
  }

  return nextSiblings
    .map((task, index) => {
      const update: TaskBoardPositionUpdate = {
        boardPosition: (index + 1) * 1000,
        taskId: task.id,
      };
      if (task.id === sourceTask.id && parentChanged) {
        update.parentId = destinationParentId;
      }
      return update;
    })
    .filter((update) => {
      const task = nextSiblings.find((item) => item.id === update.taskId);
      return (
        task &&
        (task.board_position !== update.boardPosition ||
          update.parentId !== undefined)
      );
    });
}

export function buildTaskHierarchy(
  tasks: PmsTask[],
): Map<string, IssueHierarchyInfo> {
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const childCounts = new Map<string, number>();

  for (const task of tasks) {
    if (!task.parent_id) continue;
    childCounts.set(task.parent_id, (childCounts.get(task.parent_id) ?? 0) + 1);
  }

  const info = new Map<string, IssueHierarchyInfo>();

  for (const task of tasks) {
    const path: PmsTask[] = [];
    const visited = new Set<string>();
    let parent = task.parent_id ? (byId.get(task.parent_id) ?? null) : null;

    while (parent && !visited.has(parent.id)) {
      visited.add(parent.id);
      path.unshift(parent);
      parent = parent.parent_id ? (byId.get(parent.parent_id) ?? null) : null;
    }

    info.set(task.id, {
      childCount: childCounts.get(task.id) ?? 0,
      depth: path.length,
      parent: task.parent_id ? (byId.get(task.parent_id) ?? null) : null,
      sortKey: [...path.map(issueOrder), issueOrder(task)],
    });
  }

  return info;
}

export function sortTasksByHierarchy(
  tasks: PmsTask[],
  allIssues: PmsTask[] = tasks,
): PmsTask[] {
  const hierarchy = buildTaskHierarchy(allIssues);
  return Array.from(tasks).sort((left, right) => {
    const leftInfo = hierarchy.get(left.id);
    const rightInfo = hierarchy.get(right.id);
    const keyCompare = compareSortKey(
      leftInfo?.sortKey ?? [issueOrder(left)],
      rightInfo?.sortKey ?? [issueOrder(right)],
    );
    if (keyCompare !== 0) return keyCompare;
    return left.title.localeCompare(right.title);
  });
}

/** Preserve the server-provided sort order while keeping subtasks under parents. */
export function sortTasksByHierarchyUsingInputOrder(
  tasks: PmsTask[],
): PmsTask[] {
  const hierarchy = buildTaskHierarchy(tasks);
  const inputOrder = new Map(tasks.map((task, index) => [task.id, index]));

  const inputSortKey = (task: PmsTask): number[] => {
    const ancestors: PmsTask[] = [];
    const visited = new Set<string>();
    let current: PmsTask | null = task;
    while (current && !visited.has(current.id)) {
      visited.add(current.id);
      ancestors.unshift(current);
      current = hierarchy.get(current.id)?.parent ?? null;
    }
    return ancestors.map((item) => inputOrder.get(item.id) ?? 0);
  };

  return Array.from(tasks).sort((left, right) => {
    const keyCompare = compareSortKey(inputSortKey(left), inputSortKey(right));
    if (keyCompare !== 0) return keyCompare;
    return left.title.localeCompare(right.title);
  });
}

export function getTaskRoot(
  task: PmsTask,
  hierarchy: Map<string, IssueHierarchyInfo>,
): PmsTask {
  let current = task;
  let parent = hierarchy.get(current.id)?.parent ?? null;
  const visited = new Set<string>();

  while (parent && !visited.has(parent.id)) {
    visited.add(parent.id);
    current = parent;
    parent = hierarchy.get(current.id)?.parent ?? null;
  }

  return current;
}

export function getTaskAssigneeIds(task: PmsTask): string[] {
  const assigneeIds = task.assignee_ids?.filter(Boolean) ?? [];
  const effectiveIds =
    assigneeIds.length > 0
      ? assigneeIds
      : task.assignee_id
        ? [task.assignee_id]
        : [];
  return Array.from(new Set(effectiveIds)).sort();
}

export function getTaskAssigneeGroupKey(task: PmsTask): string {
  return JSON.stringify(getTaskAssigneeIds(task));
}

/** Names, not ids: each list makes its own 휴가 label, and a view across lists should
 *  still put them in one group. */
export function getTaskLabelNames(task: PmsTask): string[] {
  const names = (task.labels ?? [])
    .map((label) => label.name.trim())
    .filter(Boolean);
  return Array.from(new Set(names)).sort();
}

export function getTaskLabelGroupKey(task: PmsTask): string {
  return JSON.stringify(getTaskLabelNames(task));
}

export function getTaskListGroupKey(
  task: PmsTask,
  hierarchy: Map<string, IssueHierarchyInfo>,
  groupBy: PmsTaskListGroupBy,
): string | null {
  if (groupBy === 'none') return null;
  const root = getTaskRoot(task, hierarchy);
  if (groupBy === 'status') return root.status;
  return groupBy === 'label'
    ? getTaskLabelGroupKey(root)
    : getTaskAssigneeGroupKey(root);
}

export function getTaskBoardPositionUpdates({
  groupBy = 'none',
  sourceTaskId,
  targetTaskId,
  tasks,
  zone,
}: TaskReorderInput): TaskBoardPositionUpdate[] {
  if (sourceTaskId === targetTaskId) return [];

  const sourceTask = tasks.find((task) => task.id === sourceTaskId);
  const targetTask = tasks.find((task) => task.id === targetTaskId);
  if (!sourceTask || !targetTask) return [];
  const tasksById = new Map(tasks.map((task) => [task.id, task]));
  const destinationParentId =
    zone === 'inside' ? targetTask.id : taskParentId(targetTask);
  if (
    destinationParentId === sourceTask.id ||
    (destinationParentId &&
      isDescendantOf(
        tasksById.get(destinationParentId) ?? targetTask,
        sourceTask.id,
        tasksById,
      ))
  ) {
    return [];
  }

  const hierarchy = buildTaskHierarchy(tasks);
  if (
    groupBy !== 'none' &&
    zone !== 'inside' &&
    getTaskListGroupKey(sourceTask, hierarchy, groupBy) !==
      getTaskListGroupKey(targetTask, hierarchy, groupBy)
  ) {
    return [];
  }

  const withoutSource = getDestinationSiblings(
    tasks,
    sourceTask.id,
    destinationParentId,
  );
  const targetIndex = withoutSource.findIndex(
    (task) => task.id === targetTask.id,
  );
  if (zone !== 'inside' && targetIndex < 0) return [];

  const insertIndex =
    zone === 'inside'
      ? withoutSource.length
      : zone === 'before'
        ? targetIndex
        : targetIndex + 1;
  return buildSiblingPositionUpdates({
    destinationParentId,
    insertIndex,
    sourceTask,
    tasks,
  });
}

/**
 * True when the moving rows already sit where this drop would put them, so
 * nothing would change. Decided by sibling order, not by board_position:
 * positions are often uneven (API-created tasks get 1, 2, 3, …) and a
 * recomputed gap value differs from the stored one even when the order stays.
 */
function isPlacementUnchanged(
  movingTasks: readonly PmsTask[],
  targetTask: PmsTask,
  tasks: PmsTask[],
  zone: TaskReorderZone,
): boolean {
  if (movingTasks.length === 0) return false;
  const parentId = zone === 'inside' ? targetTask.id : taskParentId(targetTask);
  if (movingTasks.some((task) => taskParentId(task) !== parentId)) {
    return false;
  }
  const siblingIds = sortTasksByHierarchy(
    tasks.filter((task) => taskParentId(task) === parentId),
    tasks,
  ).map((task) => task.id);
  const movingIds = new Set(movingTasks.map((task) => task.id));
  const count = movingIds.size;
  let neighbours: string[];
  if (zone === 'inside') {
    // 'inside' appends after the last child.
    neighbours = siblingIds.slice(Math.max(0, siblingIds.length - count));
  } else {
    const targetIndex = siblingIds.indexOf(targetTask.id);
    if (targetIndex < 0) return false;
    neighbours =
      zone === 'before'
        ? siblingIds.slice(Math.max(0, targetIndex - count), targetIndex)
        : siblingIds.slice(targetIndex + 1, targetIndex + 1 + count);
  }
  return (
    neighbours.length === count &&
    neighbours.every((taskId) => movingIds.has(taskId))
  );
}

/** Positions for `count` tasks placed between two siblings, or null when the
 *  gap is too small and the siblings have to be renumbered. */
function boardPositionsBetween(
  previousPosition: number | null,
  nextPosition: number | null,
  count: number,
): number[] | null {
  const offsets = Array.from({ length: count }, (_, index) => index + 1);
  if (previousPosition !== null && nextPosition !== null) {
    if (nextPosition - previousPosition <= count) return null;
    const step = Math.floor((nextPosition - previousPosition) / (count + 1));
    return offsets.map((offset) => previousPosition + step * offset);
  }
  if (previousPosition !== null) {
    return offsets.map((offset) => previousPosition + offset * 1000);
  }
  if (nextPosition !== null) {
    // Positions start at 0, so the first sibling's own value is the gap.
    if (nextPosition <= count) return null;
    const step = Math.floor(nextPosition / (count + 1));
    return offsets.map((offset) => step * offset);
  }
  return offsets.map((offset) => offset * 1000);
}

export interface TaskGroupReorderInput {
  groupBy?: PmsTaskListGroupBy;
  sourceTaskIds: readonly string[];
  targetTaskId: string;
  tasks: PmsTask[];
  zone: TaskReorderZone;
}

/**
 * Moves the selected tasks as one block, in the order they are listed now.
 * A selected subtask whose ancestor is also selected stays under that
 * ancestor and moves with it. While grouped, the block must come from one
 * group: rows of different groups have no single on-screen order.
 */
export function getTaskGroupBoardPositionUpdates({
  groupBy = 'none',
  sourceTaskIds,
  targetTaskId,
  tasks,
  zone,
}: TaskGroupReorderInput): TaskBoardPositionUpdate[] {
  const tasksById = new Map(tasks.map((task) => [task.id, task]));
  const selectedIds = new Set(
    sourceTaskIds.filter((taskId) => tasksById.has(taskId)),
  );
  const movingTasks = sortTasksByHierarchy(
    tasks.filter(
      (task) =>
        selectedIds.has(task.id) &&
        !Array.from(selectedIds).some((ancestorId) =>
          isDescendantOf(task, ancestorId, tasksById),
        ),
    ),
    tasks,
  );
  const placementTarget = tasksById.get(targetTaskId);
  if (
    placementTarget &&
    isPlacementUnchanged(movingTasks, placementTarget, tasks, zone)
  ) {
    return [];
  }
  if (movingTasks.length <= 1) {
    return movingTasks.length === 1
      ? getTaskBoardPositionUpdates({
          groupBy,
          sourceTaskId: movingTasks[0].id,
          targetTaskId,
          tasks,
          zone,
        })
      : [];
  }

  const targetTask = tasksById.get(targetTaskId);
  if (!targetTask) return [];
  const movingIds = new Set(movingTasks.map((task) => task.id));
  if (
    movingIds.has(targetTask.id) ||
    movingTasks.some((task) => isDescendantOf(targetTask, task.id, tasksById))
  ) {
    return [];
  }

  const hierarchy = buildTaskHierarchy(tasks);
  if (groupBy !== 'none') {
    const blockGroupKey = getTaskListGroupKey(
      movingTasks[0],
      hierarchy,
      groupBy,
    );
    if (
      movingTasks.some(
        (task) =>
          getTaskListGroupKey(task, hierarchy, groupBy) !== blockGroupKey,
      ) ||
      (zone !== 'inside' &&
        getTaskListGroupKey(targetTask, hierarchy, groupBy) !== blockGroupKey)
    ) {
      return [];
    }
  }

  const destinationParentId =
    zone === 'inside' ? targetTask.id : taskParentId(targetTask);
  const siblings = sortTasksByHierarchy(
    tasks.filter(
      (task) =>
        !movingIds.has(task.id) && taskParentId(task) === destinationParentId,
    ),
    tasks,
  );
  const targetIndex = siblings.findIndex((task) => task.id === targetTask.id);
  if (zone !== 'inside' && targetIndex < 0) return [];
  const insertIndex =
    zone === 'inside'
      ? siblings.length
      : zone === 'before'
        ? targetIndex
        : targetIndex + 1;

  const gapPositions = boardPositionsBetween(
    siblings[insertIndex - 1]?.board_position ?? null,
    siblings[insertIndex]?.board_position ?? null,
    movingTasks.length,
  );
  const placedTasks = gapPositions
    ? movingTasks
    : [
        ...siblings.slice(0, insertIndex),
        ...movingTasks,
        ...siblings.slice(insertIndex),
      ];
  const positions =
    gapPositions ?? placedTasks.map((_, index) => (index + 1) * 1000);

  return placedTasks.flatMap((task, index) => {
    const parentChanged =
      movingIds.has(task.id) && taskParentId(task) !== destinationParentId;
    if (!parentChanged && task.board_position === positions[index]) return [];
    return [
      {
        boardPosition: positions[index],
        ...(parentChanged ? { parentId: destinationParentId } : {}),
        taskId: task.id,
      },
    ];
  });
}
