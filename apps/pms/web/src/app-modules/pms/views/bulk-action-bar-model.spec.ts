import { describe, expect, it } from 'vitest';

import type { PmsLabel, PmsTaskListStatus } from '../api/pms-api';
import {
  BULK_DEFAULT_STATUS_OPTIONS,
  BULK_UPDATE_MAX_TASKS,
  buildBulkActionPayload,
  buildBulkDatesAction,
  buildBulkLabelActions,
  buildBulkStatusOptions,
  buildBulkUpdateRequest,
  exceedsBulkUpdateLimit,
  getTaskSelectionState,
  isBulkDateRangeInverted,
  selectedTaskIdsToArray,
  setTasksSelected,
  type BulkUpdateAction,
} from './bulk-action-bar-model';

function t(key: string): string {
  return `translated:${key}`;
}

describe('bulk action bar model', () => {
  it('allows up to the server bulk limit of 200 tasks', () => {
    expect(BULK_UPDATE_MAX_TASKS).toBe(200);
    expect(exceedsBulkUpdateLimit(200)).toBe(false);
    expect(exceedsBulkUpdateLimit(201)).toBe(true);
  });

  it('uses custom statuses when they are available', () => {
    const statuses: PmsTaskListStatus[] = [
      { slug: 'blocked', name: 'Blocked' } as PmsTaskListStatus,
      { slug: 'qa', name: 'QA' } as PmsTaskListStatus,
    ];

    expect(
      buildBulkStatusOptions({
        taskListStatuses: statuses,
        translate: () => {
          throw new Error(
            'custom statuses should not translate fallback labels',
          );
        },
      }),
    ).toEqual([
      { value: 'blocked', label: 'Blocked' },
      { value: 'qa', label: 'QA' },
    ]);
  });

  it('falls back to default status options with translated labels', () => {
    const expectedOptions = BULK_DEFAULT_STATUS_OPTIONS.map((option) => ({
      value: option.value,
      label: t(option.labelKey),
    }));

    expect(
      buildBulkStatusOptions({
        taskListStatuses: [],
        translate: t,
      }),
    ).toEqual(expectedOptions);
    expect(buildBulkStatusOptions({ translate: t })).toEqual(expectedOptions);
  });

  it('serializes selected task ids from the Set iteration order', () => {
    const selectedIds = new Set(['task-2', 'task-1']);

    const taskIds = selectedTaskIdsToArray(selectedIds);
    selectedIds.add('task-3');

    expect(taskIds).toEqual(['task-2', 'task-1']);
  });

  it('builds bulk update requests for task actions without changing payload shapes', () => {
    const taskIds = ['task-2', 'task-1'];
    const requestFor = (action: BulkUpdateAction) =>
      buildBulkUpdateRequest(taskIds, buildBulkActionPayload(action));

    expect(requestFor({ type: 'archive' })).toEqual({
      task_ids: taskIds,
      archived: true,
    });
    expect(requestFor({ type: 'restore' })).toEqual({
      task_ids: taskIds,
      archived: false,
    });
    expect(requestFor({ type: 'delete' })).toEqual({
      task_ids: taskIds,
      delete: true,
    });
    expect(requestFor({ type: 'status', status: 'in_progress' })).toEqual({
      task_ids: taskIds,
      status: 'in_progress',
    });
    expect(requestFor({ type: 'priority', priority: 'critical' })).toEqual({
      task_ids: taskIds,
      priority: 'critical',
    });
    expect(requestFor({ type: 'assignee', assigneeId: 'user-1' })).toEqual({
      task_ids: taskIds,
      assignee_id: 'user-1',
    });
    expect(requestFor({ type: 'assignee', assigneeId: null })).toEqual({
      task_ids: taskIds,
      assignee_id: null,
    });
  });

  it('builds label add and remove actions with the existing payload shapes', () => {
    const labels: PmsLabel[] = [
      { id: 'label-1', name: 'Bug', color: '#ef4444' } as PmsLabel,
    ];

    const actions = buildBulkLabelActions(labels);

    expect(actions.add).toEqual([
      {
        key: 'add-label-1',
        label: labels[0],
        payload: { add_label_ids: ['label-1'] },
      },
    ]);
    expect(actions.remove).toEqual([
      {
        key: 'rm-label-1',
        label: labels[0],
        payload: { remove_label_ids: ['label-1'] },
      },
    ]);
  });

  it('builds label action payloads for direct add and remove actions', () => {
    expect(
      buildBulkUpdateRequest(
        ['task-1'],
        buildBulkActionPayload({ type: 'add-label', labelId: 'label-1' }),
      ),
    ).toEqual({
      task_ids: ['task-1'],
      add_label_ids: ['label-1'],
    });
    expect(
      buildBulkUpdateRequest(
        ['task-1'],
        buildBulkActionPayload({ type: 'remove-label', labelId: 'label-1' }),
      ),
    ).toEqual({
      task_ids: ['task-1'],
      remove_label_ids: ['label-1'],
    });
  });

  it('reports how much of a table is selected for its header checkbox', () => {
    expect(getTaskSelectionState(['a', 'b'], new Set())).toBe('none');
    expect(getTaskSelectionState(['a', 'b'], new Set(['a', 'x']))).toBe('some');
    expect(getTaskSelectionState(['a', 'b'], new Set(['a', 'b']))).toBe('all');
    expect(getTaskSelectionState([], new Set(['a']))).toBe('none');
  });

  it('selects or clears one table without touching rows outside it', () => {
    const selected = new Set(['other']);

    expect(setTasksSelected(selected, ['a', 'b'], true)).toEqual(
      new Set(['other', 'a', 'b']),
    );
    expect(
      setTasksSelected(new Set(['other', 'a']), ['a', 'b'], false),
    ).toEqual(new Set(['other']));
    expect(selected).toEqual(new Set(['other']));
  });

  it('sends only the dates being set or cleared', () => {
    const keep = { clear: false, value: '' };
    const set = (value: string) => ({ clear: false, value });
    const clear = { clear: true, value: '2026-11-02' };

    expect(buildBulkDatesAction(keep, keep)).toBeNull();
    expect(
      buildBulkActionPayload(
        buildBulkDatesAction(set('2026-11-02'), keep) as BulkUpdateAction,
      ),
    ).toEqual({ start_date: '2026-11-02' });
    expect(
      buildBulkActionPayload(
        buildBulkDatesAction(keep, clear) as BulkUpdateAction,
      ),
    ).toEqual({ due_date: null });
  });

  it('flags a start date after the due date, ignoring cleared fields', () => {
    const set = (value: string) => ({ clear: false, value });

    expect(isBulkDateRangeInverted(set('2026-11-06'), set('2026-11-02'))).toBe(
      true,
    );
    expect(isBulkDateRangeInverted(set('2026-11-02'), set('2026-11-06'))).toBe(
      false,
    );
    expect(
      isBulkDateRangeInverted(set('2026-11-06'), {
        clear: true,
        value: '2026-11-02',
      }),
    ).toBe(false);
  });
});
