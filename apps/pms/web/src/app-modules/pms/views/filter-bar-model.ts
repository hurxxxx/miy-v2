import type {
  PmsCustomField,
  PmsLabel,
  PmsMilestone,
  PmsTaskListMember,
  PmsTaskListStatus,
  TaskFilterParams,
} from '../api/pms-api';
import {
  createDefaultTaskFilterParams,
  DEFAULT_ISSUE_ARCHIVED_STATE,
  withEffectiveTaskStatusFilter,
} from '../api/pms-filters';
import {
  customFieldFilterCount,
  formatCustomFieldValue,
  withCustomFieldFilter,
} from './custom-field-model';

export type FilterBarStatusOption = {
  value: string;
  label: string;
};

export type TaskFilterPill = {
  label: string;
  clearParams: TaskFilterParams;
};

type TranslateFilterLabel = (
  key: string,
  options?: Record<string, unknown>,
) => string;

export function isFilterActive(params: TaskFilterParams): boolean {
  return !!(
    (params.status && params.status.length > 0) ||
    params.priority ||
    params.assignee_id ||
    params.label_id ||
    params.milestone_id ||
    params.start_date_from ||
    params.start_date_to ||
    params.due_date_from ||
    params.due_date_to ||
    (params.archived_state &&
      params.archived_state !== DEFAULT_ISSUE_ARCHIVED_STATE) ||
    customFieldFilterCount(params) > 0
  );
}

/** What the Excel export carries. Nothing while the bar is clear, so the file is the
 *  whole list as before; otherwise the conditions the list query sends, search
 *  included, so the file holds the rows on screen. */
export function taskExportFilterParams(
  params: TaskFilterParams,
  taskListStatuses: readonly Pick<PmsTaskListStatus, 'category' | 'slug'>[],
): TaskFilterParams | undefined {
  if (!isFilterActive(params) && !params.q?.trim()) return undefined;
  return withEffectiveTaskStatusFilter(params, taskListStatuses);
}

export function createClearedTaskFilterParams(
  params: TaskFilterParams,
): TaskFilterParams {
  return createDefaultTaskFilterParams({ q: params.q });
}

export function createSavedTaskFilterParams(
  params: TaskFilterParams,
): TaskFilterParams {
  return createDefaultTaskFilterParams({
    ...params,
    q: undefined,
  });
}

/** miy: saved filters live on the server (`usePmsSavedFilters`); applying one
 *  keeps the search text being typed. */
export function createLoadedSavedTaskFilterParams(
  savedParams: TaskFilterParams,
  currentParams: TaskFilterParams,
): TaskFilterParams {
  return createDefaultTaskFilterParams({
    ...savedParams,
    q: currentParams.q,
  });
}

export function buildTaskFilterPills({
  customFields = [],
  labels,
  members,
  milestones,
  params,
  statusOptions,
  translate,
}: {
  customFields?: readonly PmsCustomField[];
  labels: readonly PmsLabel[];
  members: readonly PmsTaskListMember[];
  milestones: readonly PmsMilestone[];
  params: TaskFilterParams;
  statusOptions: readonly FilterBarStatusOption[];
  translate: TranslateFilterLabel;
}): TaskFilterPill[] {
  const pills: TaskFilterPill[] = [];
  if (params.status && params.status.length > 0) {
    const statusLabels = params.status
      .map((status) => statusOptions.find((option) => option.value === status)?.label ?? status)
      .join(', ');
    pills.push({
      label: translate('pms.filter.pillStatus', { value: statusLabels }),
      clearParams: { ...params, status: undefined },
    });
  }
  if (params.priority) {
    pills.push({
      label: translate('pms.filter.pillPriority', {
        value: priorityLabel(params.priority, translate),
      }),
      clearParams: { ...params, priority: undefined },
    });
  }
  if (params.assignee_id) {
    const name =
      members.find((member) => member.user_id === params.assignee_id)?.full_name ??
      translate('common:feedback.unknown');
    pills.push({
      label: translate('pms.filter.pillAssignee', { value: name }),
      clearParams: { ...params, assignee_id: undefined },
    });
  }
  if (params.label_id) {
    const name =
      labels.find((label) => label.id === params.label_id)?.name ??
      translate('common:feedback.unknown');
    pills.push({
      label: translate('pms.filter.pillLabel', { value: name }),
      clearParams: { ...params, label_id: undefined },
    });
  }
  if (params.milestone_id) {
    const name =
      milestones.find((milestone) => milestone.id === params.milestone_id)?.title ??
      translate('common:feedback.unknown');
    pills.push({
      label: translate('pms.filter.pillMilestone', { value: name }),
      clearParams: { ...params, milestone_id: undefined },
    });
  }
  if (params.start_date_from || params.start_date_to) {
    pills.push({
      label: translate('pms.filter.pillStart', {
        from: params.start_date_from ?? '...',
        to: params.start_date_to ?? '...',
      }),
      clearParams: {
        ...params,
        start_date_from: undefined,
        start_date_to: undefined,
      },
    });
  }
  if (params.due_date_from || params.due_date_to) {
    pills.push({
      label: translate('pms.filter.pillDue', {
        from: params.due_date_from ?? '...',
        to: params.due_date_to ?? '...',
      }),
      clearParams: {
        ...params,
        due_date_from: undefined,
        due_date_to: undefined,
      },
    });
  }
  if (
    params.archived_state &&
    params.archived_state !== DEFAULT_ISSUE_ARCHIVED_STATE
  ) {
    pills.push({
      label: translate('pms.filter.pillArchive', {
        value: archiveLabel(params.archived_state, translate),
      }),
      clearParams: {
        ...params,
        archived_state: DEFAULT_ISSUE_ARCHIVED_STATE,
      },
    });
  }
  // miy: one pill per custom field condition (a field deleted since keeps a
  // pill with its id until cleared, since the server ignores it).
  for (const [fieldId, value] of Object.entries(params.custom_fields ?? {})) {
    if (!value.trim()) continue;
    const field = customFields.find((item) => item.id === fieldId);
    pills.push({
      label: translate('pms.customFields.pill', {
        name: field?.name ?? translate('common:feedback.unknown'),
        value: field
          ? formatCustomFieldValue(
              field.field_type === 'multi_select'
                ? { field_type: 'select' }
                : field,
              value,
              members,
            )
          : value,
      }),
      clearParams: withCustomFieldFilter(params, fieldId, ''),
    });
  }
  return pills;
}

function priorityLabel(
  priority: NonNullable<TaskFilterParams['priority']>,
  translate: TranslateFilterLabel,
): string {
  const priorityLabelKeys: Record<string, string> = {
    critical: 'pms.priorityCritical',
    high: 'pms.priorityHigh',
    low: 'pms.priorityLow',
    medium: 'pms.priorityMedium',
  };
  return priorityLabelKeys[priority]
    ? translate(priorityLabelKeys[priority])
    : priority;
}

function archiveLabel(
  archivedState: NonNullable<TaskFilterParams['archived_state']>,
  translate: TranslateFilterLabel,
): string {
  const archiveLabelKeys: Record<string, string> = {
    active: 'pms.filter.archive.active',
    all: 'pms.filter.all',
    archived: 'pms.filter.archive.archived',
  };
  return archiveLabelKeys[archivedState]
    ? translate(archiveLabelKeys[archivedState])
    : archivedState;
}
