import type { PmsFolder, PmsTaskList } from '../api/pms-api';
import {
  applyFlatReorder,
  computeFlatDropTarget,
  sortedSiblings,
  type ReorderableItem,
} from '../api/pms-sidebar-reorder';

export type SpaceOrderDirection = 'up' | 'down';

export type SpaceOrderDraftList = Pick<
  PmsTaskList,
  'id' | 'name' | 'folder_id' | 'sort_order' | 'active_task_count'
>;

export type SpaceOrderDraft = {
  lists: SpaceOrderDraftList[];
};

export type SpaceOrderDirtySummary = {
  changedListCount: number;
  changedCount: number;
  isDirty: boolean;
};

export type SpaceOrderSavePayload = {
  lists: SpaceOrderDraftList[];
};

export function createSpaceOrderDraft({
  lists,
}: {
  lists: PmsTaskList[];
}): SpaceOrderDraft {
  return {
    lists: lists
      .filter((list) => !list.archived)
      .map((list) => ({
        id: list.id,
        name: list.name,
        folder_id: list.folder_id,
        sort_order: list.sort_order,
        active_task_count: list.active_task_count,
      })),
  };
}

export function sortSpaceOrderFolders(
  folders: PmsFolder[],
  locale: string,
): PmsFolder[] {
  return Array.from(folders).sort(
    (left, right) =>
      left.sort_order - right.sort_order ||
      left.name.localeCompare(right.name, locale),
  );
}

export function getRootSpaceOrderDraftLists(
  draft: SpaceOrderDraft,
  locale: string,
): SpaceOrderDraftList[] {
  return getSpaceOrderDraftListsForParent(draft, null, locale);
}

export function getSpaceOrderDraftListsForParent(
  draft: SpaceOrderDraft,
  parentId: string | null,
  locale: string,
): SpaceOrderDraftList[] {
  return draft.lists
    .filter((list) => (list.folder_id ?? null) === parentId)
    .slice()
    .sort((left, right) => compareByOrderThenName(left, right, locale));
}

export function summarizeSpaceOrderDraftChanges({
  draft,
  originalLists,
}: {
  draft: SpaceOrderDraft;
  originalLists: PmsTaskList[];
}): SpaceOrderDirtySummary {
  const originalListMap = new Map(
    originalLists.map((list) => [
      list.id,
      {
        folder_id: list.folder_id ?? null,
        sort_order: list.sort_order,
      },
    ]),
  );
  const changedListCount = draft.lists.filter((list) => {
    const original = originalListMap.get(list.id);
    return (
      original &&
      (original.folder_id !== (list.folder_id ?? null) ||
        original.sort_order !== list.sort_order)
    );
  }).length;
  const changedCount = changedListCount;
  return {
    changedListCount,
    changedCount,
    isDirty: changedCount > 0,
  };
}

export function moveSpaceOrderDraftListStep(
  draft: SpaceOrderDraft,
  listId: string,
  direction: SpaceOrderDirection,
): SpaceOrderDraft {
  const items = toListItems(draft.lists);
  const active = items.find((item) => item.id === listId);
  if (!active) return draft;
  const siblings = sortedSiblings(items, active.parent_id ?? null);
  const currentIndex = siblings.findIndex((item) => item.id === listId);
  if (currentIndex < 0) return draft;
  if (direction === 'up') {
    if (currentIndex === 0) return draft;
    const target = computeFlatDropTarget(
      items,
      siblings[currentIndex - 1].id,
      'before',
    );
    if (!target) return draft;
    const result = applyFlatReorder(items, listId, target);
    return result ? commitListItems(draft, result.nextItems) : draft;
  }
  if (currentIndex >= siblings.length - 1) return draft;
  const target = computeFlatDropTarget(
    items,
    siblings[currentIndex + 1].id,
    'after',
  );
  if (!target) return draft;
  const result = applyFlatReorder(items, listId, target);
  return result ? commitListItems(draft, result.nextItems) : draft;
}

export function moveSpaceOrderDraftListParent(
  draft: SpaceOrderDraft,
  listId: string,
  nextParentId: string | null,
): SpaceOrderDraft {
  const items = toListItems(draft.lists);
  const targetParentId = nextParentId ?? null;
  const result = applyFlatReorder(items, listId, {
    parentId: targetParentId,
    index: sortedSiblings(items, targetParentId).length,
    zone: 'after',
  });
  return result ? commitListItems(draft, result.nextItems) : draft;
}

export function createSpaceOrderSavePayload(
  draft: SpaceOrderDraft,
): SpaceOrderSavePayload {
  return {
    lists: draft.lists.map((list) => ({ ...list })),
  };
}

function compareByOrderThenName(
  left: { sort_order: number; name: string },
  right: { sort_order: number; name: string },
  locale: string,
): number {
  if (left.sort_order !== right.sort_order) {
    return left.sort_order - right.sort_order;
  }
  return left.name.localeCompare(right.name, locale);
}

function toListItems(lists: SpaceOrderDraftList[]): ReorderableItem[] {
  return lists.map((list) => ({
    id: list.id,
    parent_id: list.folder_id ?? null,
    sort_order: list.sort_order,
    name: list.name,
  }));
}

function commitListItems(
  draft: SpaceOrderDraft,
  nextItems: ReorderableItem[],
): SpaceOrderDraft {
  const patchMap = new Map(nextItems.map((item) => [item.id, item]));
  return {
    ...draft,
    lists: draft.lists.map((list) => {
      const next = patchMap.get(list.id);
      return next
        ? { ...list, folder_id: next.parent_id, sort_order: next.sort_order }
        : list;
    }),
  };
}
