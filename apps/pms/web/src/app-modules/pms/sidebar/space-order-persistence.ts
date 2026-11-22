import { reorderPmsTaskLists, type PmsTaskList } from '../api/pms-api';
import type { SpaceOrderSavePayload } from './space-order-editor-model';
import { type AppApi } from '../../../platform/runtime/runtime-context';

export type SpaceOrderListChange = Pick<
  SpaceOrderSavePayload['lists'][number],
  'id' | 'folder_id' | 'sort_order'
>;

export interface SpaceOrderChanges {
  listChanges: SpaceOrderListChange[];
}

export function buildSpaceOrderChanges({
  currentLists,
  payload,
}: {
  currentLists: PmsTaskList[];
  payload: SpaceOrderSavePayload;
}): SpaceOrderChanges {
  const currentListMap = new Map(
    currentLists
      .filter((list) => !list.archived)
      .map((list) => [list.id, list]),
  );

  return {
    listChanges: payload.lists.flatMap((item) => {
      const current = currentListMap.get(item.id);
      if (
        !current ||
        ((current.folder_id ?? null) === item.folder_id &&
          current.sort_order === item.sort_order)
      ) {
        return [];
      }
      return [
        {
          id: item.id,
          folder_id: item.folder_id,
          sort_order: item.sort_order,
        },
      ];
    }),
  };
}

export async function persistSpaceOrderChanges({
  workspaceSlug,
  changes,
  spaceId,
  api,
}: {
  workspaceSlug?: string | null;
  changes: SpaceOrderChanges;
  spaceId: string;
  api: AppApi;
}): Promise<void> {
  const { listChanges } = changes;
  if (listChanges.length > 0) {
    await reorderPmsTaskLists(
      api,
      spaceId,
      { items: listChanges },
      workspaceSlug,
    );
  }
}
