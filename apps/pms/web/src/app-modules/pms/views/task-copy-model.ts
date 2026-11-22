import type { PmsTaskList, TaskCopyUnmatched } from '../api/pms-api';

export type TaskCopyTarget = {
  id: string;
  key: string;
  name: string;
  spaceName: string;
  isCurrent: boolean;
};

export type TaskCopyUnmatchedKind = keyof TaskCopyUnmatched;

export type TaskCopyUnmatchedPart = {
  kind: TaskCopyUnmatchedKind;
  count: number;
};

const UNMATCHED_ORDER: readonly TaskCopyUnmatchedKind[] = [
  'assignees',
  'labels',
  'milestones',
  'custom_fields',
];

/**
 * Lists a copy can go to: active lists the user may edit, the current list first,
 * then by space and list name.
 */
export function buildTaskCopyTargets(
  taskLists: readonly PmsTaskList[],
  currentTaskListId: string,
): TaskCopyTarget[] {
  return taskLists
    .filter((taskList) => !taskList.archived && taskList.role !== 'viewer')
    .map((taskList) => ({
      id: taskList.id,
      key: taskList.key,
      name: taskList.name,
      spaceName: taskList.team_name ?? '',
      isCurrent: taskList.id === currentTaskListId,
    }))
    .sort(
      (left, right) =>
        Number(right.isCurrent) - Number(left.isCurrent) ||
        left.spaceName.localeCompare(right.spaceName) ||
        left.name.localeCompare(right.name),
    );
}

export function filterTaskCopyTargets(
  targets: readonly TaskCopyTarget[],
  query: string,
): TaskCopyTarget[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [...targets];
  return targets.filter((target) =>
    [target.name, target.key, target.spaceName].some((value) =>
      value.toLocaleLowerCase().includes(needle),
    ),
  );
}

/** The values left off the copies, in a fixed order, skipping kinds with none. */
export function taskCopyUnmatchedParts(
  unmatched: TaskCopyUnmatched,
): TaskCopyUnmatchedPart[] {
  return UNMATCHED_ORDER.filter((kind) => unmatched[kind] > 0).map((kind) => ({
    kind,
    count: unmatched[kind],
  }));
}
