import { ApiError, checkedResponse } from '@miy/sdk';

import { requestId } from '../../../../../platform-components/ui/utilities';

import {
  apiFetchJsonWithMappedError,
  ApiRequestError,
  toApiRequestError,
} from '../../../platform/api/client';
import { i18n } from '../../../platform/i18n';
import type {
  AppApi,
  AppApiBlob,
  AppFileApi,
} from '../../../platform/runtime/runtime-context';
import type { ApiSchema } from './pms-api-types';

export type PmsTaskList = Omit<
  ApiSchema<'TaskListItem'>,
  'folder_id' | 'folder_name'
> & {
  folder_id: string | null;
  folder_name: string | null;
  status_mode: 'inherit' | 'custom';
};

export type PmsTaskListsResponse = Omit<
  ApiSchema<'TaskListsResponse'>,
  'items'
> & {
  items: PmsTaskList[];
};

export type PmsSpace = ApiSchema<'SpaceItem'>;

export type PmsSpaceMember = ApiSchema<'SpaceMemberItem'>;

export type PmsTaskListMember = PmsSpaceMember;

export type PmsSpaceMembersResponse = Omit<
  ApiSchema<'SpaceMemberListResponse'>,
  'items'
> & {
  items: PmsSpaceMember[];
};

export type PmsUserSummary = ApiSchema<'SpaceUserItem'>;

export type PmsSpaceMembersBulkResult = Omit<
  ApiSchema<'SpaceMembersBulkCreateResponse'>,
  'items'
> & {
  items: PmsSpaceMember[];
};

/** How the current user wants one space shown in their own sidebar. */
export type PmsSpacePreference = ApiSchema<'SpacePreferenceItem'>;

export type PmsSpacePreferenceChange =
  ApiSchema<'SpacePreferenceUpdateRequest'>;

export type PmsMilestone = ApiSchema<'MilestoneItem'>;

export type PmsMilestonesResponse = Omit<
  ApiSchema<'MilestoneListResponse'>,
  'items'
> & {
  items: PmsMilestone[];
};

export type PmsLabel = ApiSchema<'LabelItem'>;

export type PmsTask = Omit<
  ApiSchema<'TaskItem'>,
  'description_blocks' | 'labels' | 'parent_id' | 'recurrence_rule'
> & {
  description_blocks: Record<string, unknown>[] | null;
  parent_id: string | null;
  recurrence_rule: string | null;
  labels: PmsLabel[];
};

export type PmsTasksResponse = Omit<ApiSchema<'TaskItemsResponse'>, 'items'> & {
  items: PmsTask[];
};

export type PmsWorkspaceRef = {
  id: string;
  slug: string;
  name: string;
  role?: string | null;
};

export type PersonalPmsTask = PmsTask & {
  workspace: PmsWorkspaceRef;
};

export type PersonalPmsAssignedTasksResponse = {
  items: PersonalPmsTask[];
  total: number;
  page: number;
  page_size: number;
  workspaces: PmsWorkspaceRef[];
};

export type PmsComment = Omit<ApiSchema<'TaskCommentItem'>, 'body_blocks'> & {
  body_blocks: Record<string, unknown>[] | null;
};

export type PmsActivityLog = ApiSchema<'ActivityLogItem'>;

export type PmsActivityLogsResponse = Omit<
  ApiSchema<'ActivityLogListResponse'>,
  'items'
> & {
  items: PmsActivityLog[];
};

export type PmsAttachment = ApiSchema<'AttachmentItem'>;

export type PmsChecklistItem = ApiSchema<'ChecklistItemResponse'>;

export type PmsTaskDetail = Omit<
  ApiSchema<'TaskDetailResponse'>,
  | 'attachments'
  | 'checklist_items'
  | 'comments'
  | 'linked_docs'
  | 'task'
  | 'subtasks'
> & {
  task: PmsTask;
  comments: PmsComment[];
  subtasks: PmsTask[];
  attachments: PmsAttachment[];
  checklist_items: PmsChecklistItem[];
};

export type PmsLabelsResponse = Omit<
  ApiSchema<'LabelListResponse'>,
  'items'
> & {
  items: PmsLabel[];
};

export type TaskArchivedState = 'active' | 'archived' | 'all';

export type PmsDashboardStatusCount = ApiSchema<'StatusCountItem'>;

export type PmsDashboardPriorityCount = ApiSchema<'PriorityCountItem'>;

export type PmsDashboardTaskList = ApiSchema<'DashboardTaskListItem'>;

export type PmsDashboardRecentActivity = ApiSchema<'RecentActivityItem'>;

export type PmsDashboardSummary = Omit<
  ApiSchema<'DashboardSummaryResponse'>,
  'lists' | 'priority_counts' | 'recent_activity' | 'status_counts'
> & {
  status_counts: PmsDashboardStatusCount[];
  priority_counts: PmsDashboardPriorityCount[];
  lists: PmsDashboardTaskList[];
  recent_activity: PmsDashboardRecentActivity[];
};

// ── Folders ─────────────────────────────────────────────────────────

export type PmsFolder = ApiSchema<'FolderItem'>;

export type PmsFoldersResponse = Omit<
  ApiSchema<'FolderListResponse'>,
  'items'
> & {
  items: PmsFolder[];
};

// ── Task Templates ──────────────────────────────────────────────────

export type PmsTaskTemplate = Omit<
  ApiSchema<'TaskTemplateItem'>,
  'checklist_items'
> & {
  checklist_items: { text: string }[] | null;
};

export type PmsTaskTemplatesResponse = Omit<
  ApiSchema<'TaskTemplateListResponse'>,
  'items'
> & {
  items: PmsTaskTemplate[];
};

// ── Custom Fields ───────────────────────────────────────────────────

export type PmsCustomField = Omit<
  ApiSchema<'CustomFieldItem'>,
  'field_type'
> & {
  field_type: 'text' | 'number' | 'date' | 'select';
};

export type PmsCustomFieldsResponse = Omit<
  ApiSchema<'CustomFieldListResponse'>,
  'items'
> & {
  items: PmsCustomField[];
};

export type PmsCustomFieldValue = ApiSchema<'CustomFieldValueItem'>;

// ── Task List Custom Statuses ───────────────────────────────────────

export type PmsStatusCategory = 'not_started' | 'active' | 'done' | 'closed';

export type PmsTaskListStatus = Omit<
  ApiSchema<'TaskListStatusItem'>,
  'category'
> & {
  category: PmsStatusCategory;
};

export type PmsTaskListStatusesResponse = {
  mode: 'inherit' | 'custom';
  source: 'space' | 'list';
  items: PmsTaskListStatus[];
};

export type PmsSpaceStatusesResponse = {
  items: PmsTaskListStatus[];
};

export type PmsViewPreferences = ApiSchema<'PmsViewPreferencesResponse'>;

export type PmsTaskListGroupBy = PmsViewPreferences['task_list_group_by'];

export type PmsTaskSortField =
  | 'board_position'
  | 'completed_date'
  | 'created_at'
  | 'due_date'
  | 'start_date'
  | 'title';
export type PmsTaskSortDirection = 'asc' | 'desc';
export type PmsTaskSort = {
  field: PmsTaskSortField;
  direction: PmsTaskSortDirection;
};

export const DEFAULT_PMS_TASK_SORT: PmsTaskSort = {
  direction: 'asc',
  field: 'board_position',
};

const PMS_MAX_PAGE_SIZE = 100;
const PMS_ASSIGNED_PAGE_SIZE = 50;

type PmsPageOptions = {
  page?: number;
  pageSize?: number;
};

type PmsTaskListQueryOptions = PmsPageOptions & {
  archived?: boolean;
};

type PmsTaskListPageOptions = PmsPageOptions & {
  sort?: PmsTaskSort;
};

class PmsApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** File gateway and blob failures as the PMS error the screens show. */
function mapPmsTransportError(error: unknown): unknown {
  const mapped =
    error instanceof ApiRequestError ? error : toApiRequestError(error);
  return mapped instanceof ApiRequestError
    ? new PmsApiError(mapped.status, mapped.message)
    : mapped;
}

/**
 * miy: every call goes through the app's own `runtime.api` with an
 * app-relative path (`/v1/pms/...` reaches the Rust `/api/v1/pms/...`). AI-DO
 * sent a bearer token and rewrote the path into the caller's workspace
 * (`/api/v1/workspaces/<slug>/pms/...`). The `workspaceSlug` parameters of
 * the exported functions are kept so the ported screens stay unchanged; miy
 * has one PMS per installation and ignores them. The server checks space and
 * list access on every request.
 */
async function request<T>(
  path: string,
  api: AppApi,
  init: RequestInit = {},
  _workspaceSlug?: string | null,
): Promise<T> {
  return apiFetchJsonWithMappedError<T>(
    path,
    api,
    init,
    (error) => new PmsApiError(error.status, error.message),
  );
}

export function listPmsTaskLists(
  api: AppApi,
  teamId?: string,
  workspaceSlug?: string | null,
  options: PmsTaskListQueryOptions = {},
): Promise<PmsTaskListsResponse> {
  const params = new URLSearchParams({
    archived: String(options.archived ?? false),
    page: String(options.page ?? 1),
    page_size: String(options.pageSize ?? 50),
  });
  if (teamId) params.set('team_id', teamId);
  return request<PmsTaskListsResponse>(
    `/v1/pms/lists?${params}`,
    api,
    {},
    workspaceSlug,
  );
}

export async function listAllPmsTaskLists(
  api: AppApi,
  teamId?: string,
  workspaceSlug?: string | null,
  options: Pick<PmsTaskListQueryOptions, 'archived'> = {},
): Promise<PmsTaskListsResponse> {
  const items: PmsTaskList[] = [];
  let page = 1;
  let lastResponse: PmsTaskListsResponse | null = null;
  do {
    lastResponse = await listPmsTaskLists(api, teamId, workspaceSlug, {
      archived: options.archived,
      page,
      pageSize: PMS_MAX_PAGE_SIZE,
    });
    items.push(...lastResponse.items);
    if (items.length >= lastResponse.total || lastResponse.items.length === 0) {
      break;
    }
    page += 1;
  } while (lastResponse);

  return {
    items,
    total: lastResponse?.total ?? 0,
    page: 1,
    page_size: PMS_MAX_PAGE_SIZE,
  };
}

export function listSpaces(
  api: AppApi,
  workspaceSlug?: string | null,
): Promise<PmsSpace[]> {
  return request<PmsSpace[]>('/v1/pms/spaces', api, {}, workspaceSlug);
}

export function listSpacePreferences(
  api: AppApi,
  workspaceSlug?: string | null,
): Promise<{ items: PmsSpacePreference[] }> {
  return request<{ items: PmsSpacePreference[] }>(
    '/v1/pms/space-preferences',
    api,
    {},
    workspaceSlug,
  );
}

export function updateSpacePreference(
  api: AppApi,
  spaceId: string,
  change: PmsSpacePreferenceChange,
  workspaceSlug?: string | null,
): Promise<PmsSpacePreference> {
  return request<PmsSpacePreference>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/preference`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(change),
    },
    workspaceSlug,
  );
}

export function listPmsUsers(
  api: AppApi,
  workspaceSlug?: string | null,
): Promise<PmsUserSummary[]> {
  return request<PmsUserSummary[]>(
    '/v1/pms/users',
    api,
    {},
    workspaceSlug,
  );
}

export function getPmsViewPreferences(
  api: AppApi,
  workspaceSlug?: string | null,
): Promise<PmsViewPreferences> {
  return request<PmsViewPreferences>(
    '/v1/pms/view-preferences',
    api,
    {},
    workspaceSlug,
  );
}

export function updatePmsViewPreferences(
  api: AppApi,
  payload: PmsViewPreferences,
  workspaceSlug?: string | null,
): Promise<PmsViewPreferences> {
  return request<PmsViewPreferences>(
    '/v1/pms/view-preferences',
    api,
    {
      body: JSON.stringify(payload),
      method: 'PATCH',
    },
    workspaceSlug,
  );
}

export function createSpace(
  api: AppApi,
  payload: { name: string; description?: string },
  workspaceSlug?: string | null,
): Promise<PmsSpace> {
  return request<PmsSpace>(
    '/v1/pms/spaces',
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateSpace(
  api: AppApi,
  spaceId: string,
  payload: { name?: string; description?: string },
  workspaceSlug?: string | null,
): Promise<PmsSpace> {
  return request<PmsSpace>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteSpace(
  api: AppApi,
  spaceId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

/** Every member of a space, read page by page; callers decide from the full list. */
export async function listSpaceMembers(
  api: AppApi,
  spaceId: string,
  workspaceSlug?: string | null,
): Promise<PmsSpaceMembersResponse> {
  const items: PmsSpaceMember[] = [];
  let page = 1;
  let response: PmsSpaceMembersResponse;
  do {
    response = await request<PmsSpaceMembersResponse>(
      `/v1/pms/spaces/${encodeURIComponent(spaceId)}/members?page=${page}&page_size=${PMS_MAX_PAGE_SIZE}`,
      api,
      {},
      workspaceSlug,
    );
    items.push(...response.items);
    page += 1;
  } while (response.items.length > 0 && items.length < response.total);
  return { ...response, items, page: 1, page_size: PMS_MAX_PAGE_SIZE };
}

export function addSpaceMember(
  api: AppApi,
  spaceId: string,
  payload: { user_id: string; role: string },
  workspaceSlug?: string | null,
): Promise<PmsSpaceMember> {
  return request<PmsSpaceMember>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/members`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

/** Add several users at once; the server skips anyone already a member. */
export function addSpaceMembers(
  api: AppApi,
  spaceId: string,
  payload: { user_ids: string[]; role: string },
  workspaceSlug?: string | null,
): Promise<PmsSpaceMembersBulkResult> {
  return request<PmsSpaceMembersBulkResult>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/members/bulk`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateSpaceMemberRole(
  api: AppApi,
  spaceId: string,
  userId: string,
  role: string,
  workspaceSlug?: string | null,
): Promise<PmsSpaceMember> {
  return request<PmsSpaceMember>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/members/${encodeURIComponent(userId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    },
    workspaceSlug,
  );
}

export function removeSpaceMember(
  api: AppApi,
  spaceId: string,
  userId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/members/${encodeURIComponent(userId)}`,
    api,
    { method: 'DELETE' },
    workspaceSlug,
  );
}

export function getPmsTaskList(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<PmsTaskList> {
  return request<PmsTaskList>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}`,
    api,
    {},
    workspaceSlug,
  );
}

export function createPmsTaskList(
  api: AppApi,
  payload: {
    key?: string;
    name: string;
    description?: string;
    team_id?: string | null;
    folder_id?: string | null;
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskList> {
  return request<PmsTaskList>(
    '/v1/pms/lists',
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updatePmsTaskList(
  api: AppApi,
  taskListId: string,
  payload: {
    name?: string;
    description?: string;
    status?: string;
    archived?: boolean;
    folder_id?: string | null;
    sort_order?: number;
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskList> {
  return request<PmsTaskList>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deletePmsTaskList(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

export function reorderPmsTaskLists(
  api: AppApi,
  spaceId: string,
  payload: {
    items: Array<{
      id: string;
      folder_id: string | null;
      sort_order: number;
    }>;
  },
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/lists/reorder`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function getPmsDashboardSummary(
  api: AppApi,
  taskListId?: string,
  workspaceSlug?: string | null,
): Promise<PmsDashboardSummary> {
  const suffix = taskListId ? `?list_id=${encodeURIComponent(taskListId)}` : '';
  return request<PmsDashboardSummary>(
    `/v1/pms/dashboard/summary${suffix}`,
    api,
    {},
    workspaceSlug,
  );
}

export function listTaskListMilestones(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<PmsMilestonesResponse> {
  return request<PmsMilestonesResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/milestones?page=1&page_size=20`,
    api,
    {},
    workspaceSlug,
  );
}

export function createTaskListMilestone(
  api: AppApi,
  taskListId: string,
  payload: {
    title: string;
    description: string;
    status: string;
    due_date: string | null;
  },
  workspaceSlug?: string | null,
): Promise<PmsMilestone> {
  return request<PmsMilestone>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/milestones`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export interface TaskFilterParams {
  q?: string;
  status?: string[];
  priority?: string;
  assignee_id?: string;
  label_id?: string;
  milestone_id?: string;
  due_date_from?: string;
  due_date_to?: string;
  start_date_from?: string;
  start_date_to?: string;
  archived_state?: TaskArchivedState;
}

/** The filter bar as query parameters. The task list and its Excel export share it. */
export function appendTaskFilterSearchParams(
  search: URLSearchParams,
  params: TaskFilterParams,
): void {
  if (params.q) search.set('q', params.q);
  if (params.priority && params.priority !== 'all')
    search.set('priority', params.priority);
  if (params.assignee_id) search.set('assignee_id', params.assignee_id);
  if (params.label_id) search.set('label_id', params.label_id);
  if (params.milestone_id) search.set('milestone_id', params.milestone_id);
  if (params.due_date_from) search.set('due_date_from', params.due_date_from);
  if (params.due_date_to) search.set('due_date_to', params.due_date_to);
  if (params.start_date_from)
    search.set('start_date_from', params.start_date_from);
  if (params.start_date_to) search.set('start_date_to', params.start_date_to);
  if (params.archived_state === 'active') search.set('archived', 'false');
  if (params.archived_state === 'archived') search.set('archived', 'true');
  params.status?.forEach((value) => search.append('status', value));
}

export function listTaskListTasks(
  api: AppApi,
  taskListId: string,
  params: TaskFilterParams = {},
  workspaceSlug?: string | null,
  options: PmsTaskListPageOptions = {},
): Promise<PmsTasksResponse> {
  const sort = options.sort ?? DEFAULT_PMS_TASK_SORT;
  const search = new URLSearchParams({
    page: String(options.page ?? 1),
    page_size: String(options.pageSize ?? PMS_MAX_PAGE_SIZE),
    sort_by: sort.field,
    sort_dir: sort.direction,
  });
  appendTaskFilterSearchParams(search, params);

  return request<PmsTasksResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/tasks?${search.toString()}`,
    api,
    {},
    workspaceSlug,
  );
}

export async function listAllTaskListTasks(
  api: AppApi,
  taskListId: string,
  params: TaskFilterParams = {},
  workspaceSlug?: string | null,
  options: Pick<PmsTaskListPageOptions, 'sort'> = {},
): Promise<PmsTasksResponse> {
  if (params.status && params.status.length === 0) {
    return { items: [], total: 0, page: 1, page_size: PMS_MAX_PAGE_SIZE };
  }
  const items: PmsTask[] = [];
  let page = 1;
  let lastResponse: PmsTasksResponse | null = null;
  do {
    lastResponse = await listTaskListTasks(
      api,
      taskListId,
      params,
      workspaceSlug,
      {
        page,
        pageSize: PMS_MAX_PAGE_SIZE,
        sort: options.sort,
      },
    );
    items.push(...lastResponse.items);
    if (items.length >= lastResponse.total || lastResponse.items.length === 0) {
      break;
    }
    page += 1;
  } while (lastResponse);

  return {
    items,
    total: lastResponse?.total ?? 0,
    page: 1,
    page_size: PMS_MAX_PAGE_SIZE,
  };
}

export function listAssignedTasks(
  api: AppApi,
  options: {
    limit?: number;
    workspaceSlug?: string | null;
  } & PmsPageOptions = {},
): Promise<PmsTasksResponse> {
  const search = new URLSearchParams();
  if (options.page !== undefined) {
    search.set('page', String(options.page));
    search.set('page_size', String(options.pageSize ?? PMS_ASSIGNED_PAGE_SIZE));
  } else if (options.limit !== undefined) {
    search.set('limit', String(options.limit));
  }
  const query = search.toString();
  return request<PmsTasksResponse>(
    `/v1/pms/tasks/assigned${query ? `?${query}` : ''}`,
    api,
    {},
    options.workspaceSlug,
  );
}

export async function listAllAssignedTasks(
  api: AppApi,
  workspaceSlug?: string | null,
): Promise<PmsTasksResponse> {
  const items: PmsTask[] = [];
  let page = 1;
  let lastResponse: PmsTasksResponse | null = null;
  do {
    lastResponse = await listAssignedTasks(api, {
      page,
      pageSize: PMS_ASSIGNED_PAGE_SIZE,
      workspaceSlug,
    });
    items.push(...lastResponse.items);
    if (items.length >= lastResponse.total || lastResponse.items.length === 0) {
      break;
    }
    page += 1;
  } while (lastResponse);

  return {
    items,
    total: lastResponse?.total ?? 0,
    page: 1,
    page_size: PMS_ASSIGNED_PAGE_SIZE,
  };
}

/**
 * AI-DO read the personal widget's `/api/v1/personal-widgets/pms/tasks/assigned`
 * (open assigned tasks across every PMS space, each with its space). miy
 * composes the same response from the app's own endpoints: the spaces
 * (`/v1/pms/spaces`), the active lists (`/v1/pms/lists`, for each task's space)
 * and the assigned tasks (`/v1/pms/tasks/assigned`, same filter and order). A
 * space's `slug` is its id here, so the widget can ask for that space's lists
 * with `team_id`.
 */
export async function listPersonalPmsAssignedTasks(
  api: AppApi,
): Promise<PersonalPmsAssignedTasksResponse> {
  const [spaces, taskLists, assigned] = await Promise.all([
    listSpaces(api),
    listAllPmsTaskLists(api),
    listAllAssignedTasks(api),
  ]);
  const workspaces: PmsWorkspaceRef[] = spaces
    .filter((space) => space.current_user_role)
    .map((space) => ({
      id: space.id,
      slug: space.id,
      name: space.name,
      role: space.current_user_role,
    }))
    .sort(
      (left, right) =>
        left.name.localeCompare(right.name) || left.id.localeCompare(right.id),
    );
  const workspaceById = new Map(workspaces.map((item) => [item.id, item]));
  const spaceIdByListId = new Map(
    taskLists.items.map((taskList) => [taskList.id, taskList.team_id]),
  );
  const items: PersonalPmsTask[] = [];
  for (const task of assigned.items) {
    const spaceId = spaceIdByListId.get(task.list_id);
    const workspace = spaceId ? workspaceById.get(spaceId) : undefined;
    if (!workspace) continue;
    items.push({
      ...task,
      workspace: {
        id: workspace.id,
        slug: workspace.slug,
        name: workspace.name,
      },
    });
  }
  return {
    items,
    total: items.length,
    page: 1,
    page_size: PMS_ASSIGNED_PAGE_SIZE,
    workspaces,
  };
}

export function listTodayOverdueTasks(
  api: AppApi,
  today: string,
  options: { workspaceSlug?: string | null } & PmsPageOptions = {},
): Promise<PmsTasksResponse> {
  const search = new URLSearchParams({
    today,
    page: String(options.page ?? 1),
    page_size: String(options.pageSize ?? PMS_ASSIGNED_PAGE_SIZE),
  });
  return request<PmsTasksResponse>(
    `/v1/pms/tasks/today-overdue?${search.toString()}`,
    api,
    {},
    options.workspaceSlug,
  );
}

export async function listAllTodayOverdueTasks(
  api: AppApi,
  today: string,
  workspaceSlug?: string | null,
): Promise<PmsTasksResponse> {
  const items: PmsTask[] = [];
  let page = 1;
  let lastResponse: PmsTasksResponse | null = null;
  do {
    lastResponse = await listTodayOverdueTasks(api, today, {
      page,
      pageSize: PMS_ASSIGNED_PAGE_SIZE,
      workspaceSlug,
    });
    items.push(...lastResponse.items);
    if (items.length >= lastResponse.total || lastResponse.items.length === 0) {
      break;
    }
    page += 1;
  } while (lastResponse);

  return {
    items,
    total: lastResponse?.total ?? 0,
    page: 1,
    page_size: PMS_ASSIGNED_PAGE_SIZE,
  };
}

export function createTaskListTask(
  api: AppApi,
  taskListId: string,
  payload: {
    title: string;
    description: string;
    description_blocks?: Record<string, unknown>[] | null;
    status: string;
    priority: string;
    assignee_id: string | null;
    milestone_id: string | null;
    start_date?: string | null;
    due_date: string | null;
    parent_id?: string | null;
  },
  workspaceSlug?: string | null,
): Promise<PmsTask> {
  return request<PmsTask>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/tasks`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function getTaskDetail(
  api: AppApi,
  taskId: string,
  workspaceSlug?: string | null,
): Promise<PmsTaskDetail> {
  return request<PmsTaskDetail>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}`,
    api,
    {},
    workspaceSlug,
  );
}

export function updateTask(
  api: AppApi,
  taskId: string,
  payload: Record<string, unknown>,
  workspaceSlug?: string | null,
): Promise<PmsTask> {
  return request<PmsTask>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function createTaskComment(
  api: AppApi,
  taskId: string,
  body: string,
  bodyBlocks?: Record<string, unknown>[] | null,
  workspaceSlug?: string | null,
): Promise<PmsComment> {
  return request<PmsComment>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/comments`,
    api,
    {
      method: 'POST',
      body: JSON.stringify({ body, body_blocks: bodyBlocks ?? null }),
    },
    workspaceSlug,
  );
}

export function listTaskListLabels(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<PmsLabelsResponse> {
  return request<PmsLabelsResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/labels?page=1&page_size=100`,
    api,
    {},
    workspaceSlug,
  );
}

export function createTaskListLabel(
  api: AppApi,
  taskListId: string,
  payload: { name: string; color: string },
  workspaceSlug?: string | null,
): Promise<PmsLabel> {
  return request<PmsLabel>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/labels`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateLabel(
  api: AppApi,
  labelId: string,
  payload: { name?: string; color?: string },
  workspaceSlug?: string | null,
): Promise<PmsLabel> {
  return request<PmsLabel>(
    `/v1/pms/labels/${encodeURIComponent(labelId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteLabel(
  api: AppApi,
  labelId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/labels/${encodeURIComponent(labelId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

export function deleteTask(
  api: AppApi,
  taskId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

export function listTaskActivityLogs(
  api: AppApi,
  taskId: string,
  workspaceSlug?: string | null,
): Promise<PmsActivityLogsResponse> {
  return request<PmsActivityLogsResponse>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/activity-logs?page=1&page_size=50`,
    api,
    {},
    workspaceSlug,
  );
}

/** miy limit for one attachment (platform small files, 10 MiB). */
export const PMS_ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;

type PmsPendingAttachment = {
  id: string;
  task_id: string;
  filename: string;
  content_type: string;
  size_bytes: number;
  status: 'pending';
};

/** The attachment's bytes on the platform file gateway (`runtime.fileApi`). */
export function attachmentContentPath(attachmentId: string): string {
  return `/pms/attachments/${encodeURIComponent(attachmentId)}/content`;
}

/**
 * miy: AI-DO posted the file as multipart form data. Here the server first
 * records the attachment (`POST .../attachments` with name, type and size),
 * then the bytes go through the platform's bounded file gateway
 * (`runtime.fileApi`, `PUT /pms/attachments/<id>/content`), which answers the
 * finished `AttachmentItem`. A failed or cancelled upload deletes the pending
 * record. Files over 10 MiB are refused before any request.
 */
export async function uploadAttachment(
  api: AppApi,
  fileApi: AppFileApi,
  taskId: string,
  file: File,
  workspaceSlug?: string | null,
  signal?: AbortSignal,
): Promise<PmsAttachment> {
  if (file.size > PMS_ATTACHMENT_MAX_BYTES) {
    throw new PmsApiError(
      413,
      i18n.t('errors:pms_file_size_limit_exceeded', {
        limit_mb: PMS_ATTACHMENT_MAX_BYTES / (1024 * 1024),
      }),
    );
  }
  const pending = await request<PmsPendingAttachment>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/attachments`,
    api,
    {
      method: 'POST',
      body: JSON.stringify({
        // miy: crypto.randomUUID() needs a secure context; the common UI helper does not.
        request_id: requestId(),
        filename: file.name,
        content_type: file.type || 'application/octet-stream',
        size_bytes: file.size,
      }),
    },
    workspaceSlug,
  );
  try {
    const init: RequestInit = {
      method: 'PUT',
      body: file,
      headers: { 'Content-Type': 'application/octet-stream' },
      signal,
    };
    const response = await checkedResponse(
      await fileApi(attachmentContentPath(pending.id), init),
      init,
    );
    return (await response.json()) as PmsAttachment;
  } catch (error) {
    await deleteAttachment(api, pending.id, workspaceSlug).catch(
      () => undefined,
    );
    throw mapPmsTransportError(error);
  }
}

/** Reads an attachment's bytes through the file gateway (download, preview). */
export async function fetchAttachmentBlob(
  fileApi: AppFileApi,
  attachmentId: string,
): Promise<Blob> {
  try {
    const response = await checkedResponse(
      await fileApi(attachmentContentPath(attachmentId)),
      {},
    );
    return await response.blob();
  } catch (error) {
    throw mapPmsTransportError(error);
  }
}

/** Saves a blob under `filename` through a temporary object URL. */
export function saveBlobAs(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

export async function downloadAttachment(
  fileApi: AppFileApi,
  attachment: Pick<PmsAttachment, 'id' | 'filename'>,
): Promise<void> {
  saveBlobAs(
    await fetchAttachmentBlob(fileApi, attachment.id),
    attachment.filename,
  );
}

export function deleteAttachment(
  api: AppApi,
  attachmentId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/attachments/${encodeURIComponent(attachmentId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

// ── Bulk Operations ─────────────────────────────────────────────────

export type BulkUpdatePayload = Omit<
  ApiSchema<'BulkUpdateRequest'>,
  'delete' | 'priority' | 'status'
> & {
  status?: string;
  priority?: string;
  delete?: boolean;
};

export type BulkUpdateResult = ApiSchema<'BulkUpdateResponse'>;

export type TaskCopyPayload = ApiSchema<'TaskCopyRequest'>;

export type TaskCopyResult = ApiSchema<'TaskCopyResponse'>;

export type TaskCopyUnmatched = ApiSchema<'TaskCopyUnmatchedCounts'>;

export type TaskReorderPayload = Omit<
  ApiSchema<'TaskReorderRequest'>,
  'items'
> & {
  items: ApiSchema<'TaskReorderItem'>[];
};

export type TaskReorderResult = Omit<
  ApiSchema<'TaskReorderResponse'>,
  'items'
> & {
  items: PmsTask[];
};

export type TaskTitleOrderResult = ApiSchema<'TaskTitleOrderResponse'>;

export function bulkUpdateTasks(
  api: AppApi,
  taskListId: string,
  payload: BulkUpdatePayload,
  workspaceSlug?: string | null,
): Promise<BulkUpdateResult> {
  return request<BulkUpdateResult>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/tasks/bulk`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

/** Copy tasks of one list, with their subtasks, into a list the user can edit. */
export function copyTasksToList(
  api: AppApi,
  sourceTaskListId: string,
  payload: TaskCopyPayload,
  workspaceSlug?: string | null,
): Promise<TaskCopyResult> {
  return request<TaskCopyResult>(
    `/v1/pms/lists/${encodeURIComponent(sourceTaskListId)}/tasks/copy`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function reorderTaskListTasks(
  api: AppApi,
  taskListId: string,
  payload: TaskReorderPayload,
  workspaceSlug?: string | null,
): Promise<TaskReorderResult> {
  return request<TaskReorderResult>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/tasks/reorder`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

/** Saves name order as the list's custom order, hidden tasks included. */
export function reorderTaskListTasksByTitle(
  api: AppApi,
  taskListId: string,
  direction: PmsTaskSortDirection,
  workspaceSlug?: string | null,
): Promise<TaskTitleOrderResult> {
  return request<TaskTitleOrderResult>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/tasks/reorder-by-title`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify({ sort_dir: direction }),
    },
    workspaceSlug,
  );
}

// ── Checklist ───────────────────────────────────────────────────────

export function createChecklistItem(
  api: AppApi,
  taskId: string,
  payload: { text: string; sort_order?: number },
  workspaceSlug?: string | null,
): Promise<PmsChecklistItem> {
  return request<PmsChecklistItem>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/checklist`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateChecklistItem(
  api: AppApi,
  itemId: string,
  payload: { text?: string; completed?: boolean; sort_order?: number },
  workspaceSlug?: string | null,
): Promise<PmsChecklistItem> {
  return request<PmsChecklistItem>(
    `/v1/pms/checklist/${encodeURIComponent(itemId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteChecklistItem(
  api: AppApi,
  itemId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/checklist/${encodeURIComponent(itemId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

// ── Task List Statuses (Custom Workflow) ───────────────────────────

export function listTaskListStatuses(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<PmsTaskListStatusesResponse> {
  return request<PmsTaskListStatusesResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/statuses`,
    api,
    {},
    workspaceSlug,
  );
}

export function updateTaskListStatusMode(
  api: AppApi,
  taskListId: string,
  mode: 'inherit' | 'custom',
  workspaceSlug?: string | null,
): Promise<PmsTaskListStatusesResponse> {
  return request<PmsTaskListStatusesResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/status-mode`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify({ mode }),
    },
    workspaceSlug,
  );
}

export function listSpaceStatuses(
  api: AppApi,
  spaceId: string,
  workspaceSlug?: string | null,
): Promise<PmsSpaceStatusesResponse> {
  return request<PmsSpaceStatusesResponse>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/statuses`,
    api,
    {},
    workspaceSlug,
  );
}

export function createSpaceStatus(
  api: AppApi,
  spaceId: string,
  payload: {
    name: string;
    color?: string;
    category?: PmsStatusCategory;
    sort_order?: number;
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskListStatus> {
  return request<PmsTaskListStatus>(
    `/v1/pms/spaces/${encodeURIComponent(spaceId)}/statuses`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateSpaceStatus(
  api: AppApi,
  statusId: string,
  payload: {
    name?: string;
    color?: string;
    category?: PmsStatusCategory;
    sort_order?: number;
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskListStatus> {
  return request<PmsTaskListStatus>(
    `/v1/pms/space-statuses/${encodeURIComponent(statusId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteSpaceStatus(
  api: AppApi,
  statusId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/space-statuses/${encodeURIComponent(statusId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

export function createTaskListStatus(
  api: AppApi,
  taskListId: string,
  payload: {
    name: string;
    color?: string;
    category?: PmsStatusCategory;
    sort_order?: number;
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskListStatus> {
  return request<PmsTaskListStatus>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/statuses`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateTaskListStatus(
  api: AppApi,
  statusId: string,
  payload: {
    name?: string;
    color?: string;
    category?: PmsStatusCategory;
    sort_order?: number;
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskListStatus> {
  return request<PmsTaskListStatus>(
    `/v1/pms/task-list-statuses/${encodeURIComponent(statusId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteTaskListStatus(
  api: AppApi,
  statusId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/task-list-statuses/${encodeURIComponent(statusId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

// ── Export ──────────────────────────────────────────────────────────

const FILENAME_UNSAFE = '\\/:*?"<>|';

/**
 * The name AI-DO's server gave the file (`task_export.content_disposition`):
 * the list name with characters Windows refuses replaced by spaces, or the
 * list key when nothing is left, plus " (필터 적용)" for a filtered file. miy
 * reads the export through `runtime.apiBlob`, which returns no headers, so the
 * same name is computed here.
 */
export function taskExportFilename(
  taskList: Pick<PmsTaskList, 'key' | 'name'>,
  filtered: boolean,
): string {
  const name = Array.from(taskList.name)
    .map((ch) => (FILENAME_UNSAFE.includes(ch) ? ' ' : ch))
    .join('')
    .trim();
  return `${name || taskList.key}${filtered ? ' (필터 적용)' : ''}.xlsx`;
}

/** The server treats any filter or a non-blank search as a filtered file. */
export function isFilteredTaskExport(search: URLSearchParams): boolean {
  return Array.from(search.keys()).some(
    (key) => key !== 'q' || (search.get('q') ?? '').trim() !== '',
  );
}

/** Without `filter` the file holds the whole list, archived and completed tasks too.
 *  With it, the file holds the rows that filter shows. */
export async function exportTaskListXlsx(
  api: AppApi,
  apiBlob: AppApiBlob,
  taskListId: string,
  filter?: TaskFilterParams,
  workspaceSlug?: string | null,
): Promise<void> {
  const search = new URLSearchParams();
  if (filter) appendTaskFilterSearchParams(search, filter);
  const query = search.toString();
  let blob: Blob;
  let taskList: PmsTaskList;
  try {
    [taskList, blob] = await Promise.all([
      getPmsTaskList(api, taskListId, workspaceSlug),
      apiBlob(
        `/v1/pms/lists/${encodeURIComponent(taskListId)}/export${query ? `?${query}` : ''}`,
      ),
    ]);
  } catch (error) {
    if (error instanceof ApiError && error.kind === 'abort') throw error;
    const status =
      error instanceof ApiError
        ? (error.status ?? 0)
        : error instanceof PmsApiError
          ? error.status
          : 0;
    throw new PmsApiError(status, i18n.t('apps:pms.errors.exportFailed'));
  }
  saveBlobAs(blob, taskExportFilename(taskList, isFilteredTaskExport(search)));
}

// ── Task Templates ─────────────────────────────────────────────────

export function listTaskTemplates(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<PmsTaskTemplatesResponse> {
  return request<PmsTaskTemplatesResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/templates`,
    api,
    {},
    workspaceSlug,
  );
}

export function createTaskTemplate(
  api: AppApi,
  taskListId: string,
  payload: {
    name: string;
    description?: string;
    default_status?: string;
    default_priority?: string;
    checklist_items?: { text: string }[];
  },
  workspaceSlug?: string | null,
): Promise<PmsTaskTemplate> {
  return request<PmsTaskTemplate>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/templates`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteTaskTemplate(
  api: AppApi,
  templateId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/templates/${encodeURIComponent(templateId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

// ── Custom Fields ──────────────────────────────────────────────────

export function listCustomFields(
  api: AppApi,
  taskListId: string,
  workspaceSlug?: string | null,
): Promise<PmsCustomFieldsResponse> {
  return request<PmsCustomFieldsResponse>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/custom-fields`,
    api,
    {},
    workspaceSlug,
  );
}

export function createCustomField(
  api: AppApi,
  taskListId: string,
  payload: {
    name: string;
    field_type: string;
    options?: string[];
    sort_order?: number;
  },
  workspaceSlug?: string | null,
): Promise<PmsCustomField> {
  return request<PmsCustomField>(
    `/v1/pms/lists/${encodeURIComponent(taskListId)}/custom-fields`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteCustomField(
  api: AppApi,
  fieldId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/custom-fields/${encodeURIComponent(fieldId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}

export function listTaskCustomFieldValues(
  api: AppApi,
  taskId: string,
  workspaceSlug?: string | null,
): Promise<PmsCustomFieldValue[]> {
  return request<PmsCustomFieldValue[]>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/custom-field-values`,
    api,
    {},
    workspaceSlug,
  );
}

export function setTaskCustomFieldValue(
  api: AppApi,
  taskId: string,
  payload: { field_id: string; value: string },
  workspaceSlug?: string | null,
): Promise<PmsCustomFieldValue> {
  return request<PmsCustomFieldValue>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/custom-field-values`,
    api,
    {
      method: 'PUT',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

// ── Task Assignees (Multiple) ─────────────────────────────────────

export type PmsTaskAssignee = ApiSchema<'TaskAssigneeItem'>;

export function setTaskAssignees(
  api: AppApi,
  taskId: string,
  userIds: string[],
  workspaceSlug?: string | null,
): Promise<PmsTaskAssignee[]> {
  return request<PmsTaskAssignee[]>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/assignees`,
    api,
    {
      method: 'PUT',
      body: JSON.stringify({ user_ids: userIds }),
    },
    workspaceSlug,
  );
}

export type PmsTaskFollower = ApiSchema<'TaskFollowerItem'>;

export function setTaskFollowers(
  api: AppApi,
  taskId: string,
  userIds: string[],
  workspaceSlug?: string | null,
): Promise<PmsTaskFollower[]> {
  return request<PmsTaskFollower[]>(
    `/v1/pms/tasks/${encodeURIComponent(taskId)}/followers`,
    api,
    {
      method: 'PUT',
      body: JSON.stringify({ user_ids: userIds }),
    },
    workspaceSlug,
  );
}

// ── Folders ─────────────────────────────────────────────────────────

export function listFolders(
  api: AppApi,
  teamId?: string,
  workspaceSlug?: string | null,
): Promise<PmsFoldersResponse> {
  const params = teamId ? `?team_id=${encodeURIComponent(teamId)}` : '';
  return request<PmsFoldersResponse>(
    `/v1/pms/folders${params}`,
    api,
    {},
    workspaceSlug,
  );
}

export function createFolder(
  api: AppApi,
  payload: { name: string; team_id?: string | null; sort_order?: number },
  workspaceSlug?: string | null,
): Promise<PmsFolder> {
  return request<PmsFolder>(
    '/v1/pms/folders',
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function updateFolder(
  api: AppApi,
  folderId: string,
  payload: { name?: string; sort_order?: number },
  workspaceSlug?: string | null,
): Promise<PmsFolder> {
  return request<PmsFolder>(
    `/v1/pms/folders/${encodeURIComponent(folderId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
    workspaceSlug,
  );
}

export function deleteFolder(
  api: AppApi,
  folderId: string,
  workspaceSlug?: string | null,
): Promise<void> {
  return request<void>(
    `/v1/pms/folders/${encodeURIComponent(folderId)}`,
    api,
    {
      method: 'DELETE',
    },
    workspaceSlug,
  );
}
