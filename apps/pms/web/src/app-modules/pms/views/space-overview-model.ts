import type {
  PmsFolder,
  PmsSpace,
  PmsSpaceMember,
  PmsTaskList,
} from '../api/pms-api';

export type SpaceOverviewRecentItem = {
  id: string;
  kind: 'folder' | 'list';
  name: string;
  context: string;
  updatedAt: string;
};

export interface SpaceOverviewState {
  lists: PmsTaskList[];
  folders: PmsFolder[];
  members: PmsSpaceMember[];
  spaceMeta: PmsSpace | null;
  loading: boolean;
  createListOpen: boolean;
  membersModalOpen: boolean;
  collapsedFolderIds: Set<string>;
  refreshSeq: number;
}

export type SpaceOverviewAction =
  | { type: 'loadStart' }
  | {
      type: 'loadSuccess';
      lists: PmsTaskList[];
      folders: PmsFolder[];
      members: PmsSpaceMember[];
      spaceMeta: PmsSpace | null;
    }
  | { type: 'loadDone' }
  | { type: 'setCreateListOpen'; open: boolean }
  | { type: 'setMembersModalOpen'; open: boolean }
  | { type: 'toggleFolder'; folderId: string }
  | { type: 'setAllFoldersCollapsed'; folderIds: string[]; collapsed: boolean }
  | { type: 'addList'; list: PmsTaskList; locale: string }
  | {
      type: 'applyOrderChanges';
      listChanges: Array<{
        id: string;
        folder_id: string | null;
        sort_order: number;
      }>;
      locale: string;
    }
  | { type: 'setSpaceMeta'; spaceMeta: PmsSpace | null }
  | {
      type: 'taskListChanged';
      change:
        | { type: 'updated'; taskList: PmsTaskList }
        | { type: 'deleted'; taskListId: string };
      locale: string;
      spaceId: string;
    }
  | { type: 'membersChanged' };

export const SPACE_OVERVIEW_INITIAL_STATE: SpaceOverviewState = {
  lists: [],
  folders: [],
  members: [],
  spaceMeta: null,
  loading: true,
  createListOpen: false,
  membersModalOpen: false,
  collapsedFolderIds: new Set(),
  refreshSeq: 0,
};

export function sortTaskLists(
  items: PmsTaskList[],
  locale: string,
): PmsTaskList[] {
  const sorted = Array.from(items);
  sorted.sort(
    (left, right) =>
      (left.folder_name ?? '').localeCompare(right.folder_name ?? '', locale) ||
      left.sort_order - right.sort_order ||
      left.name.localeCompare(right.name, locale),
  );
  return sorted;
}

export function sortSpaceFolders(
  items: PmsFolder[],
  locale: string,
): PmsFolder[] {
  const sorted = Array.from(items);
  sorted.sort(
    (left, right) =>
      left.sort_order - right.sort_order ||
      left.name.localeCompare(right.name, locale),
  );
  return sorted;
}

export function buildSpaceOverviewLoadSuccess(input: {
  lists: PmsTaskList[];
  folders: PmsFolder[];
  members: PmsSpaceMember[];
  spaces: PmsSpace[] | null;
  spaceId: string;
  locale: string;
}): Extract<SpaceOverviewAction, { type: 'loadSuccess' }> {
  return {
    type: 'loadSuccess',
    lists: sortTaskLists(input.lists, input.locale),
    folders: sortSpaceFolders(input.folders, input.locale),
    members: input.members,
    spaceMeta:
      input.spaces?.find((space) => space.id === input.spaceId) ?? null,
  };
}

export function spaceOverviewReducer(
  state: SpaceOverviewState,
  action: SpaceOverviewAction,
): SpaceOverviewState {
  switch (action.type) {
    case 'loadStart':
      return { ...state, loading: true };
    case 'loadSuccess':
      return {
        ...state,
        lists: action.lists,
        folders: action.folders,
        members: action.members,
        spaceMeta: action.spaceMeta,
      };
    case 'loadDone':
      return { ...state, loading: false };
    case 'setCreateListOpen':
      return { ...state, createListOpen: action.open };
    case 'setMembersModalOpen':
      return { ...state, membersModalOpen: action.open };
    case 'toggleFolder': {
      const collapsedFolderIds = new Set(state.collapsedFolderIds);
      if (collapsedFolderIds.has(action.folderId)) {
        collapsedFolderIds.delete(action.folderId);
      } else {
        collapsedFolderIds.add(action.folderId);
      }
      return { ...state, collapsedFolderIds };
    }
    case 'setAllFoldersCollapsed':
      return {
        ...state,
        collapsedFolderIds: action.collapsed
          ? new Set(action.folderIds)
          : new Set(),
      };
    case 'addList':
      return {
        ...state,
        lists: sortTaskLists([...state.lists, action.list], action.locale),
      };
    case 'applyOrderChanges': {
      const listChangeMap = new Map(
        action.listChanges.map((item) => [item.id, item]),
      );
      const folderNameMap = new Map(
        state.folders.map((folder) => [folder.id, folder.name]),
      );
      return {
        ...state,
        lists: sortTaskLists(
          state.lists.map((list) => {
            const change = listChangeMap.get(list.id);
            return change
              ? {
                  ...list,
                  folder_id: change.folder_id,
                  folder_name: change.folder_id
                    ? (folderNameMap.get(change.folder_id) ?? null)
                    : null,
                  sort_order: change.sort_order,
                }
              : list;
          }),
          action.locale,
        ),
      };
    }
    case 'setSpaceMeta':
      return { ...state, spaceMeta: action.spaceMeta };
    case 'taskListChanged': {
      const change = action.change;
      if (change.type === 'deleted') {
        return {
          ...state,
          lists: state.lists.filter((list) => list.id !== change.taskListId),
        };
      }
      if (change.taskList.team_id !== action.spaceId) return state;
      const withoutChanged = state.lists.filter(
        (list) => list.id !== change.taskList.id,
      );
      return {
        ...state,
        lists: change.taskList.archived
          ? withoutChanged
          : sortTaskLists([change.taskList, ...withoutChanged], action.locale),
      };
    }
    case 'membersChanged':
      return { ...state, refreshSeq: state.refreshSeq + 1 };
    default:
      return state;
  }
}

export function buildSpaceOverviewCollections({
  collapsedFolderIds,
  fallbackSpaceName,
  folders,
  lists,
  spaceName,
}: {
  lists: PmsTaskList[];
  folders: PmsFolder[];
  collapsedFolderIds: Set<string>;
  spaceName?: string | null;
  fallbackSpaceName: string;
}): {
  rootLists: PmsTaskList[];
  listsByFolder: Map<string, PmsTaskList[]>;
  recentItems: SpaceOverviewRecentItem[];
  allFoldersCollapsed: boolean;
} {
  const activeLists = lists.filter((list) => !list.archived);
  const folderMap = new Map(folders.map((folder) => [folder.id, folder]));
  const listsByFolder = new Map<string, PmsTaskList[]>();
  const rootLists: PmsTaskList[] = [];
  for (const list of activeLists) {
    if (list.folder_id && folderMap.has(list.folder_id)) {
      const arr = listsByFolder.get(list.folder_id) ?? [];
      arr.push(list);
      listsByFolder.set(list.folder_id, arr);
    } else if (!list.folder_id) {
      rootLists.push(list);
    }
  }

  return {
    rootLists,
    listsByFolder,
    recentItems: buildRecentItems({
      fallbackSpaceName,
      folders,
      lists: activeLists,
      spaceName,
    }),
    allFoldersCollapsed:
      folders.length > 0 &&
      folders.every((folder) => collapsedFolderIds.has(folder.id)),
  };
}

function buildRecentItems({
  fallbackSpaceName,
  folders,
  lists,
  spaceName,
}: {
  lists: PmsTaskList[];
  folders: PmsFolder[];
  spaceName?: string | null;
  fallbackSpaceName: string;
}): SpaceOverviewRecentItem[] {
  const context = spaceName ?? fallbackSpaceName;
  const listItems: SpaceOverviewRecentItem[] = lists.map((list) => ({
    id: list.id,
    kind: 'list',
    name: list.name,
    context: list.folder_name ?? list.team_name ?? '',
    updatedAt: list.updated_at,
  }));
  const folderItems: SpaceOverviewRecentItem[] = folders.map((folder) => ({
    id: folder.id,
    kind: 'folder',
    name: folder.name,
    context,
    updatedAt: '',
  }));
  const items = [...listItems, ...folderItems];
  items.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  return items.slice(0, 6);
}

export type SpaceProgressTotals = {
  activeTaskCount: number;
  openTaskCount: number;
  overdueTaskCount: number;
};

type TaskListCounts = Pick<
  PmsTaskList,
  'active_task_count' | 'open_task_count' | 'overdue_task_count'
>;

/**
 * Space-wide totals: plain sums of what the list API already counts per list.
 * Archived tasks are not in use, so they are left out of every count here.
 */
export function summarizeSpaceProgress(
  lists: readonly TaskListCounts[],
): SpaceProgressTotals {
  return lists.reduce<SpaceProgressTotals>(
    (totals, list) => ({
      activeTaskCount: totals.activeTaskCount + list.active_task_count,
      openTaskCount: totals.openTaskCount + list.open_task_count,
      overdueTaskCount: totals.overdueTaskCount + list.overdue_task_count,
    }),
    { activeTaskCount: 0, openTaskCount: 0, overdueTaskCount: 0 },
  );
}

/**
 * Share of unarchived tasks no longer open: finished or closed. Archived tasks
 * are left out of both sides, so archiving an unfinished task does not raise
 * the percent. The API's `progress` is not used here: it weights statuses and
 * drops closed tasks from its base, so a list of only completed tasks reads 0%.
 */
export function taskDonePercent(
  activeTaskCount: number,
  openTaskCount: number,
): number {
  if (activeTaskCount <= 0) return 0;
  const done = Math.round(
    ((activeTaskCount - openTaskCount) / activeTaskCount) * 100,
  );
  return Math.min(100, Math.max(0, done));
}
