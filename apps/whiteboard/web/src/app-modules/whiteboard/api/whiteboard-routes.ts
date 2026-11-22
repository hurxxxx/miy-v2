import type { paths } from '../../../api.generated';

type PathSegment = string | number;
type QueryValue = string | number | boolean | null | undefined;
type QueryEntry = readonly [string, QueryValue];

export type WhiteboardHubRouteParams = Omit<
  NonNullable<paths['/v1/whiteboard/hub']['get']['parameters']['query']>,
  'category'
>;

/**
 * AI-DO called `/api/v1/whiteboard` and rewrote it to
 * `/api/v1/workspaces/{slug}/whiteboard` for the current workspace
 * (`rewriteWorkspaceApiPath`). miy's `runtime.api` takes app-relative paths
 * (`/v1/...` reaches the app's `/api/v1/...`); the app-global space keeps the
 * plain root and an app space uses the workspace form with its ID.
 */
export const GLOBAL_SPACE_ID = 'global';

export function whiteboardApiRoot(spaceId: string | null | undefined): string {
  return !spaceId || spaceId === GLOBAL_SPACE_ID
    ? '/v1/whiteboard'
    : `/v1/workspaces/${encodeURIComponent(spaceId)}/whiteboard`;
}

function encodePathSegment(segment: PathSegment): string {
  return encodeURIComponent(String(segment));
}

function shouldOmitQueryValue(value: QueryValue): boolean {
  return value === null || value === undefined || value === '';
}

function appendQuery(
  path: string,
  entries: readonly QueryEntry[] = [],
): string {
  const query = new URLSearchParams();
  for (const [key, value] of entries) {
    if (shouldOmitQueryValue(value)) continue;
    query.set(key, String(value));
  }

  const serialized = query.toString();
  return serialized ? `${path}?${serialized}` : path;
}

export function buildWhiteboardApiPath(
  segments: readonly PathSegment[] = [],
  query: readonly QueryEntry[] = [],
  spaceId?: string | null,
): string {
  const root = whiteboardApiRoot(spaceId);
  const suffix = segments.map(encodePathSegment).join('/');
  const path = suffix ? `${root}/${suffix}` : root;
  return appendQuery(path, query);
}

export const whiteboardApiRoutes = {
  hub(params: WhiteboardHubRouteParams = {}, spaceId?: string | null): string {
    return buildWhiteboardApiPath(
      ['hub'],
      [
        ['view', params.view],
        ['q', params.q],
        ['sort_by', params.sort_by],
        ['sort_dir', params.sort_dir],
        ['page', params.page],
        ['page_size', params.page_size],
        ['source_app', params.source_app],
        ['source_kind', params.source_kind],
        ['space_id', params.space_id],
        ['target_app', params.target_app],
        ['target_type', params.target_type],
        ['target_id', params.target_id],
      ],
      spaceId,
    );
  },

  items(spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items'], [], spaceId);
  },

  item(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId], [], spaceId);
  },

  itemTarget(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'target'], [], spaceId);
  },

  itemRestore(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'restore'], [], spaceId);
  },

  itemPermanent(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'permanent'], [], spaceId);
  },

  itemFavorite(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'favorite'], [], spaceId);
  },

  itemView(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'view'], [], spaceId);
  },

  shareableUsers(q = '', spaceId?: string | null): string {
    return buildWhiteboardApiPath(['shareable-users'], [['q', q]], spaceId);
  },

  sharing(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'sharing'], [], spaceId);
  },

  userShare(itemId: string, userId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(
      ['items', itemId, 'sharing', 'users', userId],
      [],
      spaceId,
    );
  },

  /** miy: reserve a stored image of the board. */
  itemFiles(itemId: string, spaceId?: string | null): string {
    return buildWhiteboardApiPath(['items', itemId, 'files'], [], spaceId);
  },

  /** miy: the image bytes, through `runtime.fileApi` (no space). */
  fileContent(itemId: string, fileId: string): string {
    return `/whiteboard/items/${encodePathSegment(itemId)}/files/${encodePathSegment(fileId)}`;
  },
};
