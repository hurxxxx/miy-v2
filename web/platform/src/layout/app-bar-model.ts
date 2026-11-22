// Ported from AI-DO apps/web/src/components/layout/app-bar-model.ts (0002af062):
// the launcher and pin editor state, pinned ID normalisation and the draft
// reducer. What miy changes:
// - no workspaces, fixed apps or pinned-by-default list: the items are the
//   installations of this portal's environment, and an empty `pinnedApps`
//   (the default) shows every one of them;
// - one launcher menu (all apps by category) instead of a favourites menu and
//   a rail button per category;
// - one `pinnedApps` list serves both portals, so a save keeps the pins of apps
//   that are not installed in this environment.
import type { LauncherCategory } from "../data/launcher";

export type AppBarTranslator = (key: string, options?: Record<string, unknown>) => string;

/** An app of this portal's environment the person can open. */
export type AppBarAppItem = {
  /** The app ID (what `pinnedApps` holds). */
  id: string;
  title: string;
  /** Monogram shown in place of an app icon. */
  mark: string;
};

export type AppBarState = {
  appBarEditorOpen: boolean;
  appBarLayoutError: string | null;
  appBarLayoutSaving: boolean;
  /** The draft is the default arrangement: saving it stores `[]`. */
  draftIsDefault: boolean;
  draftPinnedAppIds: string[];
  launcherOpen: boolean;
};

export type AppBarAction =
  | { type: "patch"; patch: Partial<AppBarState> }
  | { type: "toggleLauncher" }
  | { type: "closeLauncherMenus" }
  | { type: "toggleDraftPinned"; appId: string; checked: boolean }
  | { type: "moveDraftPinned"; appId: string; direction: -1 | 1 };

export const INITIAL_APP_BAR_STATE: AppBarState = {
  appBarEditorOpen: false,
  appBarLayoutError: null,
  appBarLayoutSaving: false,
  draftIsDefault: false,
  draftPinnedAppIds: [],
  launcherOpen: false,
};

export function appBarReducer(state: AppBarState, action: AppBarAction): AppBarState {
  switch (action.type) {
    case "patch":
      return { ...state, ...action.patch };
    case "toggleLauncher":
      return {
        ...state,
        appBarEditorOpen: false,
        launcherOpen: !state.launcherOpen,
      };
    case "closeLauncherMenus":
      return {
        ...state,
        appBarEditorOpen: false,
        launcherOpen: false,
      };
    case "toggleDraftPinned":
      if (action.checked) {
        return state.draftPinnedAppIds.includes(action.appId)
          ? state
          : {
              ...state,
              appBarLayoutError: null,
              draftIsDefault: false,
              draftPinnedAppIds: [...state.draftPinnedAppIds, action.appId],
            };
      }
      return {
        ...state,
        appBarLayoutError: null,
        draftIsDefault: false,
        draftPinnedAppIds: state.draftPinnedAppIds.filter((item) => item !== action.appId),
      };
    case "moveDraftPinned": {
      const index = state.draftPinnedAppIds.indexOf(action.appId);
      const nextIndex = index + action.direction;
      if (index < 0 || nextIndex < 0 || nextIndex >= state.draftPinnedAppIds.length) {
        return state;
      }

      const next = [...state.draftPinnedAppIds];
      [next[index], next[nextIndex]] = [next[nextIndex]!, next[index]!];
      return { ...state, draftIsDefault: false, draftPinnedAppIds: next };
    }
  }
}

/** No saved pins: the rail shows every app the person can open. */
export function isDefaultPinnedApps(pinnedApps: readonly string[] | null | undefined) {
  return !Array.isArray(pinnedApps) || pinnedApps.length === 0;
}

export function resolvePinnedAppIds(
  pinnedApps: readonly string[] | null | undefined,
  visibleItems: readonly { id: string }[] = [],
): string[] {
  if (isDefaultPinnedApps(pinnedApps)) return visibleItems.map((item) => item.id);
  const knownAppIds = new Set<string>(visibleItems.map((item) => item.id));
  const pinnedIds: string[] = [];
  const pinnedIdSet = new Set<string>();

  for (const appId of pinnedApps!) {
    if (!knownAppIds.has(appId)) {
      continue;
    }
    if (!pinnedIdSet.has(appId)) {
      pinnedIdSet.add(appId);
      pinnedIds.push(appId);
    }
  }

  // Pins name apps of the other portal only: show every app, as with no pins.
  return pinnedIds.length > 0 ? pinnedIds : visibleItems.map((item) => item.id);
}

export function orderItemsByPinnedIds(visibleItems: AppBarAppItem[], pinnedAppIds: string[]): AppBarAppItem[] {
  const itemById = new Map(visibleItems.map((item) => [item.id, item]));
  return pinnedAppIds.flatMap((appId) => {
    const item = itemById.get(appId);
    return item ? [item] : [];
  });
}

export interface AppBarItemsProjection {
  draftItems: AppBarAppItem[];
  pinnedEligibleAppIds: Set<string>;
  pinnedItems: AppBarAppItem[];
}

export function buildAppBarItemsProjection({
  draftPinnedAppIds,
  pinnedAppIds,
  visibleItems,
}: {
  draftPinnedAppIds: string[];
  pinnedAppIds: string[];
  visibleItems: AppBarAppItem[];
}): AppBarItemsProjection {
  const pinnedItems = orderItemsByPinnedIds(visibleItems, pinnedAppIds);
  const pinnedEligibleAppIds = new Set(visibleItems.map((item) => item.id));
  const draftPinnedAppIdSet = new Set(draftPinnedAppIds);
  const draftItems = [
    ...orderItemsByPinnedIds(visibleItems, draftPinnedAppIds),
    ...visibleItems.filter((item) => !draftPinnedAppIdSet.has(item.id)),
  ];

  return {
    draftItems,
    pinnedEligibleAppIds,
    pinnedItems,
  };
}

export function resolveEditorDraftPinnedAppIds(pinnedAppIds: string[], pinnedEligibleAppIds: Set<string>): string[] {
  return pinnedAppIds.filter((appId) => pinnedEligibleAppIds.has(appId));
}

/** The default arrangement: every app the person can open, in the server's order. */
export function resolveDefaultDraftPinnedAppIds(visibleItems: readonly AppBarAppItem[]): string[] {
  return visibleItems.map((item) => item.id);
}

/**
 * The list to store: the draft's apps of this environment, then the saved pins
 * of apps this portal does not show (they belong to the other environment or
 * are not open to the person now), so a save here does not drop them there.
 */
export function resolveSavablePinnedAppIds(
  draftPinnedAppIds: string[],
  pinnedEligibleAppIds: Set<string>,
  savedPinnedApps: readonly string[] | null | undefined,
): string[] {
  const draft = draftPinnedAppIds.filter((appId) => pinnedEligibleAppIds.has(appId));
  const kept = (Array.isArray(savedPinnedApps) ? savedPinnedApps : []).filter(
    (appId) => !pinnedEligibleAppIds.has(appId),
  );
  return [...new Set([...draft, ...kept])];
}

export interface AppBarCategoryGroup {
  id: string;
  title: string;
  iconKey: string;
  items: AppBarAppItem[];
}

export const UNCATEGORIZED_GROUP_ID = "__uncategorized";

/**
 * The person's apps grouped by the administrator's categories, in category
 * order; apps in no category follow in one group. Ported from AI-DO's
 * buildEditorCategoryGroups (AppBarEditor.tsx), shared here by the launcher
 * menu and the editor.
 */
export function buildCategoryGroups({
  categories,
  items,
  t,
}: {
  categories: readonly LauncherCategory[];
  items: readonly AppBarAppItem[];
  t: AppBarTranslator;
}): AppBarCategoryGroup[] {
  const itemById = new Map(items.map((item) => [item.id, item]));
  const groupedAppIds = new Set<string>();
  const groups: AppBarCategoryGroup[] = [];

  for (const category of categories) {
    const categoryItems = category.appIds.flatMap((appId) => {
      const item = itemById.get(appId);
      if (!item || groupedAppIds.has(appId)) {
        return [];
      }
      groupedAppIds.add(appId);
      return [item];
    });
    if (categoryItems.length > 0) {
      groups.push({
        id: category.id,
        title: category.title,
        iconKey: category.iconKey,
        items: categoryItems,
      });
    }
  }

  const uncategorizedItems = items.filter((item) => !groupedAppIds.has(item.id));
  if (uncategorizedItems.length > 0) {
    groups.push({
      id: UNCATEGORIZED_GROUP_ID,
      title: t("shell:appBarEditor.uncategorized"),
      iconKey: "layout",
      items: uncategorizedItems,
    });
  }

  return groups;
}

export function filterCategoryGroups(groups: AppBarCategoryGroup[], query: string): AppBarCategoryGroup[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return groups;
  }
  return groups
    .map((group) => ({
      ...group,
      items: group.items.filter((item) =>
        `${group.title} ${item.title} ${item.id}`.toLowerCase().includes(normalizedQuery),
      ),
    }))
    .filter((group) => group.items.length > 0);
}

export type LauncherGridColumnCount = 3 | 4 | 5;

export function launcherGridColumnCount(itemCount: number): LauncherGridColumnCount {
  if (itemCount > 16) {
    return 5;
  }
  if (itemCount > 9) {
    return 4;
  }
  return 3;
}

export function isLauncherPathActive({ currentPathname, link }: { currentPathname: string; link: string }): boolean {
  const pathname = link.split(/[?#]/)[0] || "/";
  return currentPathname === pathname || currentPathname.startsWith(`${pathname}/`);
}
