import type { components, paths } from '../../../api.generated';

type OperationJson<Operation, Status extends number = 200> =
  Operation extends { responses: Record<Status, { content: { 'application/json': infer Body } }> }
    ? Body
    : never;

type OperationInput<Operation> =
  Operation extends { requestBody?: { content: { 'application/json': infer Body } } }
    ? Body
    : never;

type TaskDetail = OperationJson<paths['/v1/pms/tasks/{task_id}']['get']>;
type Dashboard = OperationJson<paths['/v1/pms/dashboard/summary']['get']>;
type TaskPage = OperationJson<paths['/v1/pms/lists/{list_id}/tasks']['get']>;
type TaskReorder = paths['/v1/pms/lists/{list_id}/tasks/reorder']['patch'];
type TaskCopy = paths['/v1/pms/lists/{list_id}/tasks/copy']['post'];
type TaskBulk = paths['/v1/pms/lists/{list_id}/tasks/bulk']['patch'];
type SpaceMembers = paths['/v1/pms/spaces/{space_id}/members']['get'];
type Milestones = paths['/v1/pms/lists/{list_id}/milestones']['get'];

interface PmsApiSchemas {
  ActivityLogListResponse: OperationJson<paths['/v1/pms/tasks/{task_id}/activity-logs']['get']>;
  ActivityLogItem: PmsApiSchemas['ActivityLogListResponse']['items'][number];
  AttachmentItem: OperationJson<paths['/files/pms/attachments/{attachment_id}/content']['put']>;
  BulkUpdateRequest: OperationInput<TaskBulk>;
  BulkUpdateResponse: OperationJson<TaskBulk>;
  ChecklistItemResponse: TaskDetail['checklist_items'][number];
  CustomFieldListResponse: OperationJson<paths['/v1/pms/lists/{list_id}/custom-fields']['get']>;
  CustomFieldItem: PmsApiSchemas['CustomFieldListResponse']['items'][number];
  CustomFieldValueItem: components['schemas']['CustomFieldValue'];
  DashboardSummaryResponse: Dashboard;
  DashboardTaskListItem: Dashboard['lists'][number];
  FolderListResponse: OperationJson<paths['/v1/pms/folders']['get']>;
  FolderItem: PmsApiSchemas['FolderListResponse']['items'][number];
  LabelListResponse: OperationJson<paths['/v1/pms/lists/{list_id}/labels']['get']>;
  LabelItem: PmsApiSchemas['LabelListResponse']['items'][number];
  MilestoneListResponse: OperationJson<Milestones>;
  MilestoneItem: PmsApiSchemas['MilestoneListResponse']['items'][number];
  PmsViewPreferencesResponse: OperationJson<paths['/v1/pms/view-preferences']['get']>;
  PriorityCountItem: Dashboard['priority_counts'][number];
  RecentActivityItem: Dashboard['recent_activity'][number];
  SpaceItem: OperationJson<paths['/v1/pms/spaces']['get']>[number];
  SpaceMemberListResponse: OperationJson<SpaceMembers>;
  SpaceMemberItem: PmsApiSchemas['SpaceMemberListResponse']['items'][number];
  SpaceMembersBulkCreateResponse: OperationJson<paths['/v1/pms/spaces/{space_id}/members/bulk']['post'], 201>;
  SpacePreferenceItem: OperationJson<paths['/v1/pms/space-preferences']['get']>['items'][number];
  SpacePreferenceUpdateRequest: OperationInput<paths['/v1/pms/spaces/{space_id}/preference']['patch']>;
  SpaceUserItem: OperationJson<paths['/v1/pms/users']['get']>[number];
  StatusCountItem: Dashboard['status_counts'][number];
  TaskAssigneeItem: OperationJson<paths['/v1/pms/tasks/{task_id}/assignees']['put']>[number];
  TaskCommentItem: TaskDetail['comments'][number];
  TaskCopyRequest: OperationInput<TaskCopy>;
  TaskCopyResponse: OperationJson<TaskCopy, 201>;
  TaskCopyUnmatchedCounts: PmsApiSchemas['TaskCopyResponse']['unmatched'];
  TaskDetailResponse: TaskDetail;
  TaskFollowerItem: OperationJson<paths['/v1/pms/tasks/{task_id}/followers']['put']>[number];
  TaskItem: TaskPage['items'][number];
  TaskItemsResponse: TaskPage;
  TaskListItem: OperationJson<paths['/v1/pms/lists/{list_id}']['get']>;
  TaskListStatusItem: OperationJson<paths['/v1/pms/lists/{list_id}/statuses']['get']>['items'][number];
  TaskListsResponse: OperationJson<paths['/v1/pms/lists']['get']>;
  TaskReorderItem: OperationInput<TaskReorder>['items'][number];
  TaskReorderRequest: OperationInput<TaskReorder>;
  TaskReorderResponse: OperationJson<TaskReorder>;
  TaskTemplateItem: OperationJson<paths['/v1/pms/templates/{template_id}']['get']>;
  TaskTemplateListResponse: OperationJson<paths['/v1/pms/lists/{list_id}/templates']['get']>;
  TaskTitleOrderResponse: OperationJson<paths['/v1/pms/lists/{list_id}/tasks/reorder-by-title']['patch']>;
}

export type ApiSchema<Key extends keyof PmsApiSchemas> = PmsApiSchemas[Key];

export type PmsInput<Path extends keyof paths, Method extends keyof paths[Path]> =
  OperationInput<paths[Path][Method]>;

export type PmsResponse<Path extends keyof paths, Method extends keyof paths[Path]> =
  OperationJson<paths[Path][Method]>;
