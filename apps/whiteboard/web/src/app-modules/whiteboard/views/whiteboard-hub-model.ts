import type {
  createWhiteboard,
  listWhiteboardHub,
  WhiteboardDetail,
  WhiteboardHubItem,
  WhiteboardVisibility,
} from '../api/whiteboard-api';

export type WhiteboardHubView =
  | 'all'
  | 'mine'
  | 'recent'
  | 'favorites'
  | 'archived';
export type WhiteboardLayoutMode = 'cards' | 'list';
export type WhiteboardSortValue =
  | 'updated_desc'
  | 'viewed_desc'
  | 'created_desc'
  | 'title_asc';
export type WhiteboardSortOption = {
  labelKey: string;
  sortBy: string;
  sortDir: 'asc' | 'desc';
};
export type WhiteboardHubListParams = Parameters<typeof listWhiteboardHub>[1];
export type WhiteboardCreatePayload = Parameters<typeof createWhiteboard>[1];
export interface WhiteboardTargetFilter {
  app: string;
  type: string;
  id: string;
}

export interface WhiteboardViewState {
  items: WhiteboardHubItem[];
  loadingList: boolean;
  error: string | null;
}

export type WhiteboardViewAction =
  | { type: 'listStarted' }
  | { type: 'listLoaded'; items: WhiteboardHubItem[] }
  | { type: 'listFailed'; error: string }
  | { type: 'setError'; error: string }
  | { type: 'upsertItem'; item: WhiteboardDetail }
  | { type: 'removeItem'; id: string }
  | { type: 'restoreItem'; item: WhiteboardDetail; archivedView: boolean };

type LayoutModeStorage = Pick<Storage, 'getItem' | 'setItem'>;

// miy: the same key under the app's own frame origin.
export const VIEW_MODE_STORAGE_KEY = 'ai-do:whiteboard:view-mode';
export const WHITEBOARD_HUB_PAGE_SIZE = 200;

export const VIEW_LABEL_KEYS: Record<WhiteboardHubView, string> = {
  all: 'shell:nav.whiteboard-all',
  mine: 'shell:nav.whiteboard-my',
  recent: 'shell:nav.whiteboard-recent',
  favorites: 'shell:nav.whiteboard-favorites',
  archived: 'shell:nav.whiteboard-archived',
};

export const SORT_OPTIONS: Record<WhiteboardSortValue, WhiteboardSortOption> = {
  updated_desc: {
    labelKey: 'apps:whiteboard.recentlyUpdated',
    sortBy: 'updated_at',
    sortDir: 'desc',
  },
  viewed_desc: {
    labelKey: 'apps:whiteboard.recentlyViewed',
    sortBy: 'last_viewed_at',
    sortDir: 'desc',
  },
  created_desc: {
    labelKey: 'apps:whiteboard.recentlyCreated',
    sortBy: 'created_at',
    sortDir: 'desc',
  },
  title_asc: {
    labelKey: 'apps:whiteboard.tableName',
    sortBy: 'title',
    sortDir: 'asc',
  },
};

export const WHITEBOARD_VIEW_INITIAL_STATE: WhiteboardViewState = {
  items: [],
  loadingList: false,
  error: null,
};

export function whiteboardViewReducer(
  state: WhiteboardViewState,
  action: WhiteboardViewAction,
): WhiteboardViewState {
  switch (action.type) {
    case 'listStarted':
      return {
        ...state,
        loadingList: true,
        error: null,
      };
    case 'listLoaded':
      return {
        ...state,
        items: action.items,
        loadingList: false,
      };
    case 'listFailed':
      return {
        ...state,
        items: [],
        loadingList: false,
        error: action.error,
      };
    case 'setError':
      return {
        ...state,
        error: action.error,
      };
    case 'upsertItem':
      return {
        ...state,
        items: state.items.some((item) => item.id === action.item.id)
          ? state.items.map((item) =>
              item.id === action.item.id ? { ...item, ...action.item } : item,
            )
          : [action.item, ...state.items],
      };
    case 'removeItem':
      return {
        ...state,
        items: state.items.filter((item) => item.id !== action.id),
      };
    case 'restoreItem':
      return {
        ...state,
        items: action.archivedView
          ? state.items.filter((item) => item.id !== action.item.id)
          : state.items.map((item) =>
              item.id === action.item.id ? { ...item, ...action.item } : item,
            ),
      };
    default:
      return state;
  }
}

export function viewFromSearch(value: string | null): WhiteboardHubView {
  if (value === 'trash') return 'archived';
  if (
    value === 'mine' ||
    value === 'recent' ||
    value === 'favorites' ||
    value === 'archived'
  ) {
    return value;
  }
  return 'all';
}

export function readWhiteboardTargetFilter(
  searchParams: URLSearchParams,
): WhiteboardTargetFilter | null {
  const spaceId = searchParams.get('space_id');
  if (spaceId) return { app: 'pms', type: 'space', id: spaceId };

  const app = searchParams.get('target_app');
  const type = searchParams.get('target_type');
  const id = searchParams.get('target_id');
  if (!app || !type || !id) return null;
  return { app, type, id };
}

export function buildWhiteboardHubListParams(options: {
  view: WhiteboardHubView;
  query: string;
  sortValue: WhiteboardSortValue;
  targetFilter: WhiteboardTargetFilter | null;
}): WhiteboardHubListParams {
  const sortOption = SORT_OPTIONS[options.sortValue];
  const isSpaceTarget =
    options.targetFilter?.app === 'pms' &&
    options.targetFilter.type === 'space';
  return {
    view: options.view,
    q: options.query,
    sort_by: sortOption.sortBy,
    sort_dir: sortOption.sortDir,
    page_size: WHITEBOARD_HUB_PAGE_SIZE,
    space_id: isSpaceTarget ? options.targetFilter?.id : undefined,
    target_app: isSpaceTarget ? undefined : options.targetFilter?.app,
    target_type: isSpaceTarget ? undefined : options.targetFilter?.type,
    target_id: isSpaceTarget ? undefined : options.targetFilter?.id,
  };
}

export function buildWhiteboardCreatePayload(options: {
  title: string;
  targetFilter: WhiteboardTargetFilter | null;
  currentWorkspaceId: string | null | undefined;
  itemCount: number;
  visibility?: WhiteboardVisibility;
}): WhiteboardCreatePayload {
  const visibility =
    options.visibility ?? (options.targetFilter ? 'workspace' : 'personal');
  let primaryTarget: WhiteboardCreatePayload['primary_target'] = null;
  if (visibility === 'workspace') {
    primaryTarget = options.targetFilter
      ? { ...options.targetFilter, sort_order: options.itemCount }
      : options.currentWorkspaceId
        ? {
            app: 'whiteboard',
            type: 'workspace_sidebar',
            id: options.currentWorkspaceId,
            sort_order: 0,
          }
        : null;
  }

  return {
    title: options.title,
    source_app: primaryTarget?.app === 'pms' ? 'pms' : 'whiteboard',
    source_kind: 'manual',
    primary_target: primaryTarget,
  };
}

/**
 * AI-DO built `/w/<slug>/whiteboard/<id>` portal paths (or an embedding app's
 * route). In the app frame a board is `/<id>` and the hub `/`, both keeping
 * the current query (`space`, `view`, target filter).
 */
export function buildWhiteboardHubItemPath(options: {
  itemId: string;
  searchParams: URLSearchParams;
}): string {
  return `/${encodeURIComponent(options.itemId)}${searchSuffix(options.searchParams)}`;
}

export function buildWhiteboardHubRootPath(options: {
  searchParams: URLSearchParams;
}): string {
  return `/${searchSuffix(options.searchParams)}`;
}

export function readStoredLayoutMode(
  storage: LayoutModeStorage | null = getBrowserLayoutStorage(),
): WhiteboardLayoutMode {
  if (!storage) return 'cards';
  return storage.getItem(VIEW_MODE_STORAGE_KEY) === 'list' ? 'list' : 'cards';
}

export function writeStoredLayoutMode(
  layoutMode: WhiteboardLayoutMode,
  storage: LayoutModeStorage | null = getBrowserLayoutStorage(),
): void {
  if (!storage) return;
  storage.setItem(VIEW_MODE_STORAGE_KEY, layoutMode);
}

function searchSuffix(searchParams: URLSearchParams): string {
  const search = searchParams.toString();
  return search ? `?${search}` : '';
}

function getBrowserLayoutStorage(): LayoutModeStorage | null {
  if (typeof window === 'undefined') return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}
