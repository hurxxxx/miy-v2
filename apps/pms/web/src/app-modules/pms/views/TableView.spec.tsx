import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n/i18n';
import type { PmsTask } from '../api/pms-api';
import { TableView } from './TableView';

function task(overrides: Partial<PmsTask> = {}): PmsTask {
  return {
    archived: false,
    assignee_id: null,
    assignee_ids: [],
    assignee_name: null,
    assignee_names: [],
    board_position: 1000,
    checklist_done: 0,
    checklist_total: 0,
    comments_count: 0,
    completed_date: null,
    description: '',
    description_blocks: null,
    due_date: null,
    follower_ids: [],
    follower_names: [],
    id: 'task-1',
    labels: [],
    list_id: 'list-1',
    milestone_id: null,
    milestone_title: null,
    parent_id: null,
    priority: 'medium',
    priority_label: 'Medium',
    recurrence_rule: null,
    reference: 'TASK-1',
    reporter_id: 'reporter-1',
    reporter_name: 'Riley Park',
    start_date: null,
    status: 'done',
    status_label: 'Done',
    task_number: 1,
    title: 'Completed task',
    updated_at: '2026-06-18T00:00:00Z',
    ...overrides,
  } as PmsTask;
}

describe('TableView', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
  });

  it('shows completion dates in the desktop table and mobile task card', () => {
    render(
      <TableView
        onSelectIssue={vi.fn()}
        tasks={[task({ completed_date: '2026-06-18' })]}
      />,
    );

    const table = within(screen.getByRole('table'));
    expect(
      table.getByRole('columnheader', { name: '완료일' }),
    ).not.toBeNull();
    expect(table.getByText('2026. 6. 18.')).not.toBeNull();

    const mobileCard = within(screen.getByTestId('mobile-task-card'));
    expect(mobileCard.getByText('완료일')).not.toBeNull();
    expect(mobileCard.getByText('2026. 6. 18.')).not.toBeNull();
  });

  it('updates the completion date from the desktop table', () => {
    const onUpdateIssue = vi.fn();
    render(
      <TableView
        canEdit
        onSelectIssue={vi.fn()}
        onUpdateIssue={onUpdateIssue}
        tasks={[task({ completed_date: '2026-06-18' })]}
      />,
    );

    const table = within(screen.getByRole('table'));
    const input = table.getByRole('textbox', { name: '완료일' });
    fireEvent.change(input, { target: { value: '2026. 6. 20.' } });
    fireEvent.blur(input);

    expect(onUpdateIssue).toHaveBeenCalledWith('task-1', {
      completed_date: '2026-06-20',
    });
  });

  it('selects a row from anywhere in its checkbox cell without opening it', () => {
    const onSelectIssue = vi.fn();
    const onToggleSelect = vi.fn();
    render(
      <TableView
        onSelectIssue={onSelectIssue}
        onToggleSelect={onToggleSelect}
        selectedIds={new Set()}
        tasks={[task({ id: 'a', reference: 'TASK-A', title: 'A' })]}
      />,
    );

    const checkbox = within(screen.getByRole('table')).getByRole('checkbox', {
      name: 'TASK-A 선택',
    });
    fireEvent.click(checkbox.closest('td') as HTMLTableCellElement);
    expect(onToggleSelect).toHaveBeenCalledTimes(1);
    expect(onToggleSelect).toHaveBeenCalledWith('a');

    fireEvent.click(checkbox);
    expect(onToggleSelect).toHaveBeenCalledTimes(2);
    expect(onSelectIssue).not.toHaveBeenCalled();
  });

  it('selects every row from the header checkbox and clears them once all are selected', () => {
    const onSelectTasks = vi.fn();
    const tasks = [
      task({ id: 'a', reference: 'TASK-A', title: 'A' }),
      task({ board_position: 2000, id: 'b', reference: 'TASK-B', title: 'B' }),
    ];
    const { rerender } = render(
      <TableView
        onSelectIssue={vi.fn()}
        onSelectTasks={onSelectTasks}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['a'])}
        tasks={tasks}
      />,
    );

    const table = within(screen.getByRole('table'));
    const header = table.getByRole('checkbox', {
      name: '이 표의 태스크 2개 모두 선택',
    }) as HTMLInputElement;
    expect(header.indeterminate).toBe(true);

    fireEvent.click(header);
    expect(onSelectTasks).toHaveBeenLastCalledWith(['a', 'b'], true);

    rerender(
      <TableView
        onSelectIssue={vi.fn()}
        onSelectTasks={onSelectTasks}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['a', 'b'])}
        tasks={tasks}
      />,
    );
    expect(header.checked).toBe(true);
    expect(header.indeterminate).toBe(false);

    fireEvent.click(header);
    expect(onSelectTasks).toHaveBeenLastCalledWith(['a', 'b'], false);
  });

  it('shows custom field columns, sorts by them and edits a value (miy)', () => {
    const onSortChange = vi.fn();
    const onUpdateCustomField = vi.fn(async () => true);
    render(
      <TableView
        canEdit
        customFields={[
          {
            id: 'f-points',
            list_id: 'list-1',
            name: 'Points',
            field_type: 'number',
            options: null,
            sort_order: 0,
          },
        ]}
        onSelectIssue={vi.fn()}
        onSortChange={onSortChange}
        onUpdateCustomField={onUpdateCustomField}
        sort={{ field: 'cf_f-points', direction: 'asc' }}
        tasks={[task({ id: 'a', custom_fields: { 'f-points': '3' } })]}
      />,
    );
    const header = screen.getByRole('columnheader', { name: /Points/ });
    expect(header.getAttribute('aria-sort')).toBe('ascending');
    fireEvent.click(within(header).getByRole('button'));
    expect(onSortChange).toHaveBeenCalledWith({
      field: 'cf_f-points',
      direction: 'desc',
    });
    const cell = screen.getByRole('textbox', { name: 'Points' }) as HTMLInputElement;
    expect(cell.value).toBe('3');
    fireEvent.change(cell, { target: { value: '5' } });
    fireEvent.blur(cell);
    expect(onUpdateCustomField).toHaveBeenCalledWith('a', 'f-points', '5');
  });
});
