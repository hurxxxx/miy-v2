import { buildAppRoutePath } from '../../../platform/core-web';
import type { PmsSpace } from '../api/pms-api';
import {
  buildPmsSpaceToolPath,
  buildPmsTaskListToolPath,
} from '../views/pms-view-route';

/** The aggregate context is never a fallback owner for an individual row. */
export function pmsSidebarSpaceKey({
  canonical,
  currentWorkspaceSlug,
  spaces,
  spaceId,
}: {
  canonical: boolean;
  currentWorkspaceSlug: string | null;
  spaces: readonly PmsSpace[];
  spaceId: string | null | undefined;
}): string | null {
  if (!canonical) return currentWorkspaceSlug;
  const space = spaces.find((item) => item.id === spaceId);
  return space?.workspace_id === space?.id && space?.workspace_key
    ? space.workspace_key
    : null;
}

export function pmsSidebarSpacePath(
  routeBase: string | null,
  workspaceKey: string,
  spaceId: string,
): string {
  return routeBase
    ? buildAppRoutePath({ routeBase, spaceKey: workspaceKey })
    : buildPmsSpaceToolPath(spaceId, { workspaceSlug: workspaceKey });
}

export function pmsSidebarListPath(
  routeBase: string | null,
  workspaceKey: string,
  listId: string,
  settings = false,
): string {
  return routeBase
    ? buildAppRoutePath(
        { routeBase, spaceKey: workspaceKey },
        `/lists/${encodeURIComponent(listId)}${settings ? '?settings=1' : ''}`,
      )
    : buildPmsTaskListToolPath({
        workspaceSlug: workspaceKey,
        taskListId: listId,
        settings,
      });
}
