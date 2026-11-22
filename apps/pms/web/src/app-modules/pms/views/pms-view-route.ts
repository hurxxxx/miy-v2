export type PmsViewRoute =
  | { kind: 'assigned' }
  | { kind: 'today' }
  | { kind: 'list'; taskListId: string }
  | { kind: 'spaceTasks'; spaceId: string; tab: PmsTaskListToolTab }
  | { kind: 'spaceOverview'; spaceId: string }
  | { kind: 'overview' }
  | { kind: 'taskCreateFallback' };

export type PmsTaskListToolTab =
  | 'list'
  | 'board'
  | 'calendar'
  | 'gantt'
  | 'table';

/** The parts of an authorized PMS space runtime that link inside that space. */
export type PmsSpaceRuntimeLink = {
  app: { app_id: string };
  space: { id: string; key: string; kind: string };
  href: (path?: string) => string;
};

export type PmsToolPathOptions = {
  workspaceSlug?: string | null;
  runtime?: PmsSpaceRuntimeLink | null;
};

const PMS_TASK_LIST_TOOL_PREFIX = 'pms-list-';
const PMS_SPACE_TOOL_PREFIX = 'pms-space-';
const PMS_CREATE_TASK_PARAM = 'create';
const PMS_CREATE_TASK_TITLE_PARAM = 'title';
const PMS_CREATE_TASK_SOURCE_PARAM = 'source';
const PMS_CREATE_TASK_SOURCE_TODO_ID_PARAM = 'sourceTodoId';
const PMS_CREATE_TASK_PERSONAL_TODO_SOURCE = 'personal-todo';

export type PmsCreateTaskRequest = {
  sourceTodoId: string | null;
  title: string;
};

function appendQuery(path: string, params: URLSearchParams): string {
  const query = params.toString();
  return query ? `${path}?${query}` : path;
}

/**
 * miy: the app's own path. AI-DO built `/w/<workspaceSlug>/pms<suffix>` and
 * fell back to `/` without a workspace; inside the frame every path is
 * app-relative, so the workspace and app prefix are dropped.
 */
function buildPmsWorkspacePath(
  _workspaceSlug: string | null | undefined,
  suffix: string,
): string {
  if (!suffix) return '/';
  return suffix.startsWith('/') ? suffix : `/${suffix}`;
}

/**
 * A link inside the PMS space the runtime authorized, or null when the target
 * belongs to another space and must keep its legacy locator.
 */
function canonicalPmsSpacePath(
  options: PmsToolPathOptions,
  spaceId: string,
  suffix: string,
): string | null {
  const { runtime, workspaceSlug } = options;
  if (
    !runtime ||
    runtime.app.app_id !== 'pms' ||
    runtime.space.kind !== 'app_space' ||
    runtime.space.id !== spaceId ||
    (workspaceSlug && workspaceSlug !== runtime.space.key)
  ) {
    return null;
  }
  try {
    return runtime.href(suffix);
  } catch {
    return null;
  }
}

function encodePathSegment(value: string): string {
  return encodeURIComponent(value);
}

function decodePathSegment(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function resolvePmsWorkspaceViewRoute(
  pathname: string | null | undefined,
  requestedTab?: string | null,
  runtimeSpaceId?: string | null,
): PmsViewRoute | null {
  const canonical = pathname?.match(/^\/pms\/s\/[^/]+(?:\/(.*))?$/);
  if (canonical && runtimeSpaceId) {
    const segments = (canonical[1] ?? '').split('/').filter(Boolean);
    if (segments[0] === 'lists' && segments[1]) {
      return { kind: 'list', taskListId: decodePathSegment(segments[1]) };
    }
    if (!segments.length)
      return isPmsTaskListToolTab(requestedTab)
        ? { kind: 'spaceTasks', spaceId: runtimeSpaceId, tab: requestedTab }
        : { kind: 'spaceOverview', spaceId: runtimeSpaceId };
    return null;
  }
  // miy: app-relative paths (`/lists/<id>`, `/spaces/<id>`); AI-DO matched
  // `/w/<workspaceSlug>/pms/...`.
  const routeRest = pathname?.replace(/^\/+/, '') ?? '';
  if (!routeRest) {
    return null;
  }

  const segments = routeRest.split('/').filter(Boolean);
  if (segments[0] === 'lists' && segments[1]) {
    return { kind: 'list', taskListId: decodePathSegment(segments[1]) };
  }

  if (segments[0] !== 'spaces' || !segments[1]) {
    return null;
  }

  const spaceId = decodePathSegment(segments[1]);
  if (!segments[2]) {
    if (isPmsTaskListToolTab(requestedTab)) {
      return { kind: 'spaceTasks', spaceId, tab: requestedTab };
    }
    return { kind: 'spaceOverview', spaceId };
  }

  return null;
}

export function buildPmsTaskListToolId(taskListId: string): string {
  return `${PMS_TASK_LIST_TOOL_PREFIX}${taskListId}`;
}

export function buildPmsTaskListToolPath({
  settings = false,
  taskId,
  taskListId,
  tab = 'list',
  workspaceSlug,
}: {
  settings?: boolean;
  taskId?: string | null;
  taskListId: string;
  tab?: PmsTaskListToolTab;
} & PmsToolPathOptions): string {
  const params = new URLSearchParams();
  if (tab !== 'list') params.set('tab', tab);
  if (settings) params.set('settings', '1');
  if (taskId) params.set('task', taskId);
  return buildPmsWorkspacePath(
    workspaceSlug,
    appendQuery(`/lists/${encodePathSegment(taskListId)}`, params),
  );
}

export function buildPmsSpaceToolId(spaceId: string): string {
  return `${PMS_SPACE_TOOL_PREFIX}${spaceId}`;
}

export function buildPmsSpaceToolPath(
  spaceId: string,
  options: PmsToolPathOptions = {},
): string {
  return (
    canonicalPmsSpacePath(options, spaceId, '') ??
    buildPmsWorkspacePath(
      options.workspaceSlug,
      `/spaces/${encodePathSegment(spaceId)}`,
    )
  );
}

export function buildPmsSpaceTaskToolPath(
  args: {
    spaceId: string;
    tab: PmsTaskListToolTab;
  } & PmsToolPathOptions,
): string {
  const params = new URLSearchParams({ tab: args.tab });
  return (
    canonicalPmsSpacePath(args, args.spaceId, appendQuery('', params)) ??
    buildPmsWorkspacePath(
      args.workspaceSlug,
      appendQuery(`/spaces/${encodePathSegment(args.spaceId)}`, params),
    )
  );
}

export function buildPmsCreateTaskSearch({
  sourceTodoId,
  title,
}: {
  sourceTodoId?: string | null;
  title?: string | null;
} = {}): string {
  const params = new URLSearchParams({ [PMS_CREATE_TASK_PARAM]: '1' });
  const trimmedTitle = title?.trim();
  if (trimmedTitle) {
    params.set(PMS_CREATE_TASK_TITLE_PARAM, trimmedTitle);
  }
  if (sourceTodoId) {
    params.set(
      PMS_CREATE_TASK_SOURCE_PARAM,
      PMS_CREATE_TASK_PERSONAL_TODO_SOURCE,
    );
    params.set(PMS_CREATE_TASK_SOURCE_TODO_ID_PARAM, sourceTodoId);
  }
  return `?${params.toString()}`;
}

export function readPmsCreateTaskRequest(
  searchParams: URLSearchParams,
): PmsCreateTaskRequest {
  return {
    sourceTodoId:
      searchParams.get(PMS_CREATE_TASK_SOURCE_PARAM) ===
      PMS_CREATE_TASK_PERSONAL_TODO_SOURCE
        ? searchParams.get(PMS_CREATE_TASK_SOURCE_TODO_ID_PARAM)
        : null,
    title: searchParams.get(PMS_CREATE_TASK_TITLE_PARAM)?.trim() ?? '',
  };
}

export function clearPmsCreateTaskSearchParams(
  searchParams: URLSearchParams,
): URLSearchParams {
  const nextParams = new URLSearchParams(searchParams);
  nextParams.delete(PMS_CREATE_TASK_PARAM);
  nextParams.delete(PMS_CREATE_TASK_TITLE_PARAM);
  nextParams.delete(PMS_CREATE_TASK_SOURCE_PARAM);
  nextParams.delete(PMS_CREATE_TASK_SOURCE_TODO_ID_PARAM);
  return nextParams;
}

export function resolvePmsViewRoute({
  createTaskRequested,
  isNewTaskModalOpen,
  requestedTab,
  routePathname,
  runtimeSpaceId,
  toolId,
}: {
  createTaskRequested: boolean;
  isNewTaskModalOpen: boolean;
  requestedTab?: string | null;
  routePathname?: string | null;
  runtimeSpaceId?: string | null;
  toolId?: string;
}): PmsViewRoute {
  const workspaceRoute = resolvePmsWorkspaceViewRoute(
    routePathname,
    requestedTab,
    runtimeSpaceId,
  );
  if (workspaceRoute) {
    return workspaceRoute;
  }

  if (toolId === 'pms-tasks' || toolId === 'pms-tasks-assigned') {
    return { kind: 'assigned' };
  }
  if (toolId === 'pms-tasks-today') {
    return { kind: 'today' };
  }

  if (!toolId && (createTaskRequested || isNewTaskModalOpen)) {
    return { kind: 'taskCreateFallback' };
  }

  return { kind: 'overview' };
}

function isPmsTaskListToolTab(
  value: string | null | undefined,
): value is PmsTaskListToolTab {
  return (
    value === 'list' ||
    value === 'board' ||
    value === 'calendar' ||
    value === 'gantt' ||
    value === 'table'
  );
}
