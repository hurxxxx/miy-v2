import type {
  BulkUpdatePayload,
  PmsLabel,
  PmsTaskListStatus,
} from '../api/pms-api';

/** Most tasks one bulk update accepts (`BulkUpdateRequest.task_ids` on the server). */
export const BULK_UPDATE_MAX_TASKS = 200;

export function exceedsBulkUpdateLimit(selectedCount: number): boolean {
  return selectedCount > BULK_UPDATE_MAX_TASKS;
}

export type TaskSelectionState = 'all' | 'none' | 'some';

/** How much of `taskIds` is selected, for a header checkbox over those rows. */
export function getTaskSelectionState(
  taskIds: readonly string[],
  selectedIds: ReadonlySet<string>,
): TaskSelectionState {
  let selectedCount = 0;
  for (const taskId of taskIds) {
    if (selectedIds.has(taskId)) selectedCount += 1;
  }
  if (selectedCount === 0) return 'none';
  return selectedCount === taskIds.length ? 'all' : 'some';
}

/** Selects or clears `taskIds`, keeping the selection outside them as is. */
export function setTasksSelected(
  selectedIds: ReadonlySet<string>,
  taskIds: readonly string[],
  selected: boolean,
): Set<string> {
  const next = new Set(selectedIds);
  for (const taskId of taskIds) {
    if (selected) next.add(taskId);
    else next.delete(taskId);
  }
  return next;
}

export const BULK_DEFAULT_STATUS_OPTIONS = [
  { value: 'todo', labelKey: 'pms.filter.status.todo' },
  { value: 'in_progress', labelKey: 'pms.filter.status.inProgress' },
  { value: 'review', labelKey: 'pms.filter.status.review' },
  { value: 'done', labelKey: 'pms.filter.status.done' },
  { value: 'complete', labelKey: 'pms.filter.status.complete' },
] as const;

export type BulkStatusOption = {
  value: string;
  label: string;
};

type TranslateBulkActionLabel = (key: string) => string;

export type BulkUpdateActionPayload = Omit<BulkUpdatePayload, 'task_ids'>;

export type BulkUpdateAction =
  | { type: 'archive' }
  | { type: 'restore' }
  | { type: 'delete' }
  | { type: 'status'; status: string }
  | { type: 'priority'; priority: string }
  | { type: 'assignee'; assigneeId: string | null }
  // `undefined` keeps a date, `null` clears it, a date string sets it.
  | { type: 'dates'; startDate?: string | null; dueDate?: string | null }
  | { type: 'add-label'; labelId: string }
  | { type: 'remove-label'; labelId: string };

export type BulkLabelAction = {
  key: string;
  label: PmsLabel;
  payload: BulkUpdateActionPayload;
};

export type BulkLabelActions = {
  add: BulkLabelAction[];
  remove: BulkLabelAction[];
};

export function buildBulkStatusOptions({
  taskListStatuses,
  translate,
}: {
  taskListStatuses?: readonly PmsTaskListStatus[];
  translate: TranslateBulkActionLabel;
}): BulkStatusOption[] {
  if (taskListStatuses && taskListStatuses.length > 0) {
    return taskListStatuses.map((status) => ({
      value: status.slug,
      label: status.name,
    }));
  }

  return BULK_DEFAULT_STATUS_OPTIONS.map((option) => ({
    value: option.value,
    label: translate(option.labelKey),
  }));
}

export function selectedTaskIdsToArray(
  selectedIds: ReadonlySet<string>,
): string[] {
  return Array.from(selectedIds);
}

export function buildBulkActionPayload(
  action: BulkUpdateAction,
): BulkUpdateActionPayload {
  switch (action.type) {
    case 'archive':
      return { archived: true };
    case 'restore':
      return { archived: false };
    case 'delete':
      return { delete: true };
    case 'status':
      return { status: action.status };
    case 'priority':
      return { priority: action.priority };
    case 'assignee':
      return { assignee_id: action.assigneeId };
    case 'dates': {
      // Only the fields being changed go out; a missing field keeps its date.
      const payload: BulkUpdateActionPayload = {};
      if (action.startDate !== undefined) payload.start_date = action.startDate;
      if (action.dueDate !== undefined) payload.due_date = action.dueDate;
      return payload;
    }
    case 'add-label':
      return { add_label_ids: [action.labelId] };
    case 'remove-label':
      return { remove_label_ids: [action.labelId] };
  }
}

/** One date field of the bulk date form. */
export type BulkDateField = { clear: boolean; value: string };

function bulkDateChange(field: BulkDateField): string | null | undefined {
  if (field.clear) return null;
  return field.value || undefined;
}

/** The start date would land after the due date for every selected task. */
export function isBulkDateRangeInverted(
  start: BulkDateField,
  due: BulkDateField,
): boolean {
  const startDate = bulkDateChange(start);
  const dueDate = bulkDateChange(due);
  return Boolean(startDate && dueDate && startDate > dueDate);
}

/** The bulk date action, or null when the form changes nothing. */
export function buildBulkDatesAction(
  start: BulkDateField,
  due: BulkDateField,
): Extract<BulkUpdateAction, { type: 'dates' }> | null {
  const startDate = bulkDateChange(start);
  const dueDate = bulkDateChange(due);
  if (startDate === undefined && dueDate === undefined) return null;
  return { type: 'dates', startDate, dueDate };
}

export function buildBulkUpdateRequest(
  taskIds: readonly string[],
  payload: BulkUpdateActionPayload,
): BulkUpdatePayload {
  return {
    task_ids: [...taskIds],
    ...payload,
  };
}

export function buildBulkLabelActions(
  labels: readonly PmsLabel[],
): BulkLabelActions {
  return {
    add: labels.map((label) => ({
      key: `add-${label.id}`,
      label,
      payload: buildBulkActionPayload({
        type: 'add-label',
        labelId: label.id,
      }),
    })),
    remove: labels.map((label) => ({
      key: `rm-${label.id}`,
      label,
      payload: buildBulkActionPayload({
        type: 'remove-label',
        labelId: label.id,
      }),
    })),
  };
}
