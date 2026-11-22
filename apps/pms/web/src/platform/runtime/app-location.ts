/**
 * The app's screens inside the frame (app-relative paths, parsed from
 * `runtime.location.pathname`). AI-DO mounted the same screens under
 * `/w/:workspaceSlug/pms/...` (app-modules/pms/routes.ts); miy has one PMS
 * per installation, so the workspace prefix is gone:
 *
 * - `/` PMS overview, `/spaces/:spaceId` space, `/lists/:taskListId` list
 *   (query parameters as in AI-DO, e.g. `?task=<id>`, `?tab=board`) — PMSView;
 * - `/assigned` AssignedToMeView, `/today` TodayOverdueView;
 * - `/tasks/:taskId` notification and home-card destination (opens the list
 *   with `?task=` when the list is readable, otherwise the single task);
 * - `/widgets/my-tasks` side panel (AI-DO's floating PMS widget content).
 *
 * Unknown paths show the overview.
 */
export type AppRoute =
  | { kind: 'pms' }
  | { kind: 'assigned' }
  | { kind: 'today' }
  | { kind: 'task'; taskId: string }
  | { kind: 'widget'; widget: 'my-tasks' };

export type PmsRouteParams = {
  /** AI-DO route parameter kept for the ported screens; constant in miy. */
  workspaceSlug: string;
  spaceId?: string;
  taskListId?: string;
  taskId?: string;
};

/**
 * AI-DO addressed PMS through a workspace slug (`/w/:workspaceSlug/pms`). The
 * ported screens still pass the slug around (API helpers accept and ignore it,
 * links drop it); miy uses this constant so their "workspace selected" checks
 * hold.
 */
export const PMS_WORKSPACE_KEY = 'pms';

export const WIDGET_ROUTE_PATHS = {
  myTasks: '/widgets/my-tasks',
} as const;

function decodeSegment(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function segments(pathname: string): string[] {
  return pathname.split('/').filter(Boolean);
}

export function resolveAppRoute(pathname: string): AppRoute {
  const parts = segments(pathname);
  if (parts.length === 1 && parts[0] === 'assigned') return { kind: 'assigned' };
  if (parts.length === 1 && parts[0] === 'today') return { kind: 'today' };
  if (parts.length === 2 && parts[0] === 'tasks' && parts[1]) {
    return { kind: 'task', taskId: decodeSegment(parts[1]) };
  }
  if (parts.length === 2 && parts[0] === 'widgets' && parts[1] === 'my-tasks') {
    return { kind: 'widget', widget: 'my-tasks' };
  }
  return { kind: 'pms' };
}

/** Route parameters for the ported `useParams()` calls. */
export function matchPmsRoute(pathname: string): PmsRouteParams {
  const parts = segments(pathname);
  const params: PmsRouteParams = { workspaceSlug: PMS_WORKSPACE_KEY };
  if (parts[0] === 'spaces' && parts[1]) params.spaceId = decodeSegment(parts[1]);
  if (parts[0] === 'lists' && parts[1]) params.taskListId = decodeSegment(parts[1]);
  if (parts[0] === 'tasks' && parts[1]) params.taskId = decodeSegment(parts[1]);
  return params;
}
