import type { PmsTask, PmsTaskList } from '../api/pms-api';
import type { PmsTaskListToolTab } from './pms-view-route';

/** How a space's Gantt and Calendar tabs lay out tasks: one chart per list, or one for all. */
export type SpaceTasksLayout = 'by-list' | 'combined';

type LayoutStorage = Pick<Storage, 'getItem' | 'setItem'>;

function layoutStorageKey(spaceId: string): string {
  return `pms_space_tasks_layout_${spaceId}`;
}

function browserStorage(): LayoutStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

/** The layout last chosen for this space in this browser; one chart per list otherwise. */
export function loadSpaceTasksLayout(
  spaceId: string,
  storage: LayoutStorage | null = browserStorage(),
): SpaceTasksLayout {
  try {
    return storage?.getItem(layoutStorageKey(spaceId)) === 'combined'
      ? 'combined'
      : 'by-list';
  } catch {
    return 'by-list';
  }
}

export function saveSpaceTasksLayout(
  spaceId: string,
  layout: SpaceTasksLayout,
  storage: LayoutStorage | null = browserStorage(),
): void {
  try {
    storage?.setItem(layoutStorageKey(spaceId), layout);
  } catch {
    // Blocked storage only loses the remembered choice; the view still switches.
  }
}

/** Only the date views can merge lists; the others keep their per-list sections. */
export function supportsCombinedSpaceLayout(tab: PmsTaskListToolTab): boolean {
  return tab === 'gantt' || tab === 'calendar';
}

export type CombinedSpaceTasks = {
  /** Every list's visible tasks, in list order and each list's own order. */
  tasks: PmsTask[];
  listByTaskId: ReadonlyMap<string, PmsTaskList>;
  listNameByTaskId: ReadonlyMap<string, string>;
};

export function combineSpaceTasks(
  taskLists: readonly PmsTaskList[],
  visibleTasksByListId: ReadonlyMap<string, readonly PmsTask[]>,
): CombinedSpaceTasks {
  const tasks: PmsTask[] = [];
  const listByTaskId = new Map<string, PmsTaskList>();
  const listNameByTaskId = new Map<string, string>();
  for (const taskList of taskLists) {
    for (const task of visibleTasksByListId.get(taskList.id) ?? []) {
      tasks.push(task);
      listByTaskId.set(task.id, taskList);
      listNameByTaskId.set(task.id, taskList.name);
    }
  }
  return { tasks, listByTaskId, listNameByTaskId };
}
