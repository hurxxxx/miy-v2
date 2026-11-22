import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { PmsTask, PmsTaskListMember } from '../api/pms-api';
import { ListView } from './ListView';

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
    parent_id: null,
    priority: 'medium',
    priority_label: 'Medium',
    recurrence_rule: null,
    reference: 'TASK-1',
    reporter_id: 'reporter-1',
    reporter_name: 'Riley Park',
    start_date: null,
    status: 'todo',
    status_label: 'To Do',
    task_number: 1,
    title: 'Task with reporter',
    ...overrides,
  } as PmsTask;
}

function member(overrides: Partial<PmsTaskListMember> = {}): PmsTaskListMember {
  return {
    email: 'riley@example.test',
    full_name: 'Riley Park',
    is_admin: false,
    joined_at: '2026-06-10T00:00:00.000Z',
    primary_org_unit_name: 'Product',
    role: 'member',
    user_id: 'reporter-1',
    ...overrides,
  };
}

function dataTransfer() {
  const store = new Map<string, string>();
  return {
    effectAllowed: '',
    getData: vi.fn((type: string) => store.get(type) ?? ''),
    setData: vi.fn((type: string, value: string) => {
      store.set(type, value);
    }),
  };
}

describe('ListView', () => {
  it('keeps the desktop horizontal scrollbar on the bounded list viewport', () => {
    render(<ListView onSelectIssue={vi.fn()} tasks={[task()]} />);

    const scrollRegion = screen.getByTestId('task-list-scroll-region');
    expect(scrollRegion.className).toContain('min-h-0');
    expect(scrollRegion.className).toContain('min-w-0');
    expect(scrollRegion.className).toContain('flex-1');
    expect(scrollRegion.className).toContain('overflow-auto');

    const tableContainer =
      screen.getByTestId('desktop-task-table').parentElement;
    expect(tableContainer?.className).not.toContain('overflow-x-auto');
  });

  it('pins the desktop column header while the list scrolls', () => {
    render(<ListView onSelectIssue={vi.fn()} tasks={[task()]} />);

    const headerCells = within(
      screen.getByTestId('desktop-task-table-header'),
    ).getAllByRole('columnheader');
    expect(headerCells.length).toBeGreaterThan(5);
    for (const cell of headerCells) {
      expect(cell.className).toContain('sticky');
      expect(cell.className).toContain('top-0');
    }
  });

  it('switches between status, assignee, and ungrouped modes without a duplicate add button', () => {
    const onGroupByChange = vi.fn();
    render(
      <ListView
        onGroupByChange={onGroupByChange}
        onSelectIssue={vi.fn()}
        tasks={[task()]}
      />,
    );

    expect(screen.queryByText('업무 추가')).toBeNull();
    const statusButton = screen.getByRole('button', { name: '그룹: 상태' });
    const userButton = screen.getByRole('button', { name: '그룹: 사용자' });
    expect(statusButton.getAttribute('aria-pressed')).toBe('true');

    fireEvent.click(userButton);
    expect(userButton.getAttribute('aria-pressed')).toBe('true');
    expect(statusButton.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(userButton);

    expect(onGroupByChange).toHaveBeenNthCalledWith(1, 'assignee');
    expect(onGroupByChange).toHaveBeenNthCalledWith(2, 'none');
  });

  it('selects date sorting, toggles direction, and resets to custom order', () => {
    const onSortChange = vi.fn();
    const { rerender } = render(
      <ListView
        onSelectIssue={vi.fn()}
        onSortChange={onSortChange}
        tasks={[task()]}
      />,
    );

    fireEvent.click(
      screen.getByRole('button', { name: '정렬: 사용자 지정 순서' }),
    );
    fireEvent.click(screen.getByRole('button', { name: '마감일' }));
    expect(onSortChange).toHaveBeenLastCalledWith({
      direction: 'asc',
      field: 'due_date',
    });

    rerender(
      <ListView
        onSelectIssue={vi.fn()}
        onSortChange={onSortChange}
        sort={{ direction: 'asc', field: 'due_date' }}
        tasks={[task()]}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: '정렬: 마감일' }));
    fireEvent.click(screen.getByRole('button', { name: /^마감일/ }));
    expect(onSortChange).toHaveBeenLastCalledWith({
      direction: 'desc',
      field: 'due_date',
    });

    fireEvent.click(screen.getByRole('button', { name: '정렬 초기화' }));
    expect(onSortChange).toHaveBeenLastCalledWith({
      direction: 'asc',
      field: 'board_position',
    });
  });

  it('sorts by name and saves that order as the custom order', async () => {
    const onSortChange = vi.fn();
    const onApplyTitleOrder = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <ListView
        canEdit
        onApplyTitleOrder={onApplyTitleOrder}
        onSelectIssue={vi.fn()}
        onSortChange={onSortChange}
        tasks={[task()]}
      />,
    );

    expect(
      screen.queryByRole('button', { name: '사용자 지정 순서로 저장' }),
    ).toBeNull();
    fireEvent.click(
      screen.getByRole('button', { name: '정렬: 사용자 지정 순서' }),
    );
    fireEvent.click(screen.getByRole('button', { name: '이름' }));
    expect(onSortChange).toHaveBeenLastCalledWith({
      direction: 'asc',
      field: 'title',
    });

    rerender(
      <ListView
        canEdit
        onApplyTitleOrder={onApplyTitleOrder}
        onSelectIssue={vi.fn()}
        onSortChange={onSortChange}
        sort={{ direction: 'desc', field: 'title' }}
        tasks={[task()]}
      />,
    );
    fireEvent.click(
      screen.getByRole('button', { name: '사용자 지정 순서로 저장' }),
    );
    await waitFor(() => expect(onApplyTitleOrder).toHaveBeenCalledWith('desc'));
  });

  it('hides saving name order from members who cannot edit the list', () => {
    render(
      <ListView
        onApplyTitleOrder={vi.fn()}
        onSelectIssue={vi.fn()}
        onSortChange={vi.fn()}
        sort={{ direction: 'asc', field: 'title' }}
        tasks={[task()]}
      />,
    );

    expect(screen.getByRole('button', { name: '정렬: 이름' })).toBeTruthy();
    expect(
      screen.queryByRole('button', { name: '사용자 지정 순서로 저장' }),
    ).toBeNull();
  });

  it('toggles completed items from the grouping and sorting toolbar', () => {
    const onShowCompletedItemsChange = vi.fn();
    const { rerender } = render(
      <ListView
        onSelectIssue={vi.fn()}
        onShowCompletedItemsChange={onShowCompletedItemsChange}
        showCompletedItems={false}
        tasks={[task()]}
      />,
    );

    fireEvent.click(screen.getByRole('checkbox', { name: '완료항목 표시' }));
    expect(onShowCompletedItemsChange).toHaveBeenCalledWith(true);

    rerender(
      <ListView
        onSelectIssue={vi.fn()}
        onShowCompletedItemsChange={onShowCompletedItemsChange}
        showCompletedItems
        tasks={[task()]}
      />,
    );
    expect(
      (
        screen.getByRole('checkbox', {
          name: '완료항목 표시',
        }) as HTMLInputElement
      ).checked,
    ).toBe(true);

    fireEvent.click(screen.getByRole('checkbox', { name: '완료항목 표시' }));
    expect(onShowCompletedItemsChange).toHaveBeenLastCalledWith(false);
  });

  it('disables the completed-items control until status metadata is ready', () => {
    render(
      <ListView
        completedItemsControlDisabled
        onSelectIssue={vi.fn()}
        onShowCompletedItemsChange={vi.fn()}
        tasks={[task()]}
      />,
    );

    expect(
      (
        screen.getByRole('checkbox', {
          name: '완료항목 표시',
        }) as HTMLInputElement
      ).disabled,
    ).toBe(true);
  });

  it('groups a task hierarchy by the root task primary assignee', () => {
    render(
      <ListView
        groupBy="assignee"
        members={[
          member({ full_name: 'Alice Kim', user_id: 'user-1' }),
          member({ full_name: 'Bob Lee', user_id: 'user-2' }),
        ]}
        onSelectIssue={vi.fn()}
        tasks={[
          task({
            assignee_id: 'user-1',
            assignee_ids: ['user-1'],
            assignee_name: 'Alice Kim',
            assignee_names: ['Alice Kim'],
            id: 'root-1',
            title: 'Alice root',
          }),
          task({
            assignee_id: 'user-2',
            assignee_ids: ['user-2'],
            assignee_name: 'Bob Lee',
            assignee_names: ['Bob Lee'],
            board_position: 2000,
            id: 'child-1',
            parent_id: 'root-1',
            title: 'Child assigned to Bob',
          }),
          task({
            assignee_id: 'user-2',
            assignee_ids: ['user-2'],
            assignee_name: 'Bob Lee',
            assignee_names: ['Bob Lee'],
            board_position: 3000,
            id: 'root-2',
            title: 'Bob root',
          }),
          task({
            board_position: 4000,
            id: 'root-3',
            title: 'Unassigned root',
          }),
        ]}
      />,
    );

    const aliceGroup = within(screen.getByTestId('assignee-task-group-user-1'));
    expect(aliceGroup.getAllByText('Alice root')).not.toHaveLength(0);
    expect(aliceGroup.getAllByText('Child assigned to Bob')).not.toHaveLength(
      0,
    );
    expect(
      within(screen.getByTestId('assignee-task-group-user-2')).queryByText(
        'Child assigned to Bob',
      ),
    ).toBeNull();
    const unassignedGroup = within(
      screen.getByTestId('assignee-task-group-unassigned'),
    );
    expect(unassignedGroup.getAllByText('Unassigned root')).not.toHaveLength(0);
    expect(unassignedGroup.queryByText('pms.unassigned')).toBeNull();
  });

  it('groups tasks by label and switches the grouping from the toolbar', () => {
    const onGroupByChange = vi.fn();
    render(
      <ListView
        groupBy="label"
        onGroupByChange={onGroupByChange}
        onSelectIssue={vi.fn()}
        tasks={[
          task({
            id: 'leave-1',
            labels: [{ color: '#22c55e', id: 'leave', name: '휴가' }],
            title: '[휴가] 연차',
          }),
          task({
            board_position: 2000,
            id: 'trip-1',
            labels: [{ color: '#3b82f6', id: 'trip', name: '출장' }],
            title: '[출장] 부산',
          }),
          task({
            board_position: 3000,
            id: 'plain-1',
            labels: [],
            title: '주간 회의',
          }),
        ]}
      />,
    );

    expect(
      within(screen.getByTestId('label-task-group-휴가')).getAllByText(
        '[휴가] 연차',
      ),
    ).not.toHaveLength(0);
    expect(
      within(screen.getByTestId('label-task-group-휴가')).queryByText(
        '[출장] 부산',
      ),
    ).toBeNull();
    const noLabelGroup = within(screen.getByTestId('label-task-group-none'));
    expect(noLabelGroup.getAllByText('라벨 없음')).not.toHaveLength(0);
    expect(noLabelGroup.getAllByText('주간 회의')).not.toHaveLength(0);

    const labelButton = screen.getByRole('button', { name: '그룹: 라벨' });
    expect(labelButton.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(labelButton);
    expect(onGroupByChange).toHaveBeenCalledWith('none');
  });

  it('opens the task on a single title click and renames on a double click', () => {
    vi.useFakeTimers();
    try {
      const onSelectIssue = vi.fn();
      render(
        <ListView
          canEdit
          onSelectIssue={onSelectIssue}
          onUpdateIssue={vi.fn().mockResolvedValue(undefined)}
          tasks={[task()]}
        />,
      );
      const table = within(screen.getByTestId('desktop-task-table'));
      const title = table.getByRole('button', { name: 'Task with reporter' });

      fireEvent.click(title);
      expect(onSelectIssue).not.toHaveBeenCalled();
      vi.advanceTimersByTime(300);
      expect(onSelectIssue).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'task-1' }),
      );

      onSelectIssue.mockClear();
      fireEvent.click(title);
      fireEvent.click(title);
      fireEvent.doubleClick(title);
      vi.advanceTimersByTime(300);
      expect(onSelectIssue).not.toHaveBeenCalled();
      expect(screen.getByRole('textbox', { name: '이름' })).toHaveProperty(
        'value',
        'Task with reporter',
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it('opens the task immediately from the title when the list is read-only', () => {
    const onSelectIssue = vi.fn();
    render(<ListView onSelectIssue={onSelectIssue} tasks={[task()]} />);

    const table = within(screen.getByTestId('desktop-task-table'));
    fireEvent.click(table.getByRole('button', { name: 'Task with reporter' }));
    expect(onSelectIssue).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'task-1' }),
    );
  });

  it('shows task reporter names in the desktop reporter column', () => {
    render(
      <ListView
        members={[member()]}
        onSelectIssue={vi.fn()}
        tasks={[task()]}
      />,
    );

    const table = within(screen.getByTestId('desktop-task-table'));
    expect(table.getByText('Riley Park')).not.toBeNull();
  });

  it('shows completion dates in the desktop table and mobile task card', () => {
    render(
      <ListView
        onSelectIssue={vi.fn()}
        tasks={[task({ completed_date: '2026-06-18' })]}
      />,
    );

    const table = within(screen.getByTestId('desktop-task-table'));
    expect(table.getByRole('columnheader', { name: '완료일' })).not.toBeNull();
    expect(table.getByText('2026. 6. 18.')).not.toBeNull();

    const mobileCard = within(screen.getByTestId('mobile-task-card'));
    expect(mobileCard.getByText('완료일')).not.toBeNull();
    expect(mobileCard.getByText('2026. 6. 18.')).not.toBeNull();
  });

  it('updates the completion date from the desktop list', async () => {
    const onUpdateIssue = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onSelectIssue={vi.fn()}
        onUpdateIssue={onUpdateIssue}
        tasks={[task({ completed_date: '2026-06-18' })]}
      />,
    );

    const table = within(screen.getByTestId('desktop-task-table'));
    fireEvent.click(table.getByRole('button', { name: '완료일' }));
    const input = screen.getByRole('textbox', { name: '완료일' });
    fireEvent.change(input, { target: { value: '2026. 6. 20.' } });
    fireEvent.blur(input);

    await waitFor(() => {
      expect(onUpdateIssue).toHaveBeenCalledWith('task-1', {
        completed_date: '2026-06-20',
      });
    });
  });

  it('uses the batch reorder callback instead of per-task updates when provided', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    const onUpdateIssue = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        onUpdateIssue={onUpdateIssue}
        tasks={[
          task({
            board_position: 1000,
            id: 'task-1',
            reference: 'TASK-1',
            title: 'First',
          }),
          task({
            board_position: 2000,
            id: 'task-2',
            reference: 'TASK-2',
            title: 'Second',
          }),
          task({
            board_position: 3000,
            id: 'task-3',
            reference: 'TASK-3',
            title: 'Third',
          }),
        ]}
      />,
    );

    const dragData = dataTransfer();
    const firstDragHandle = screen.getAllByRole('button', {
      name: '드래그해서 순서 이동',
    })[0];
    const table = within(screen.getByTestId('desktop-task-table'));
    const thirdRow = table.getByRole('button', { name: 'Third' }).closest('tr');
    expect(thirdRow).not.toBeNull();

    fireEvent.dragStart(firstDragHandle, { dataTransfer: dragData });
    fireEvent.drop(thirdRow as HTMLTableRowElement, { dataTransfer: dragData });

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 2500, taskId: 'task-1' },
    ]);
    expect(onUpdateIssue).not.toHaveBeenCalled();
  });

  it('drags every selected row together when a selected row is dragged', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['task-1', 'task-3'])}
        tasks={[1, 2, 3, 4].map((number) =>
          task({
            board_position: number * 1000,
            id: `task-${number}`,
            reference: `TASK-${number}`,
            title: `Task ${number}`,
          }),
        )}
      />,
    );

    const dragData = dataTransfer();
    const dragHandles = screen.getAllByRole('button', {
      name: '드래그해서 순서 이동',
    });
    expect(dragHandles[2].getAttribute('title')).toBe(
      '선택한 2개를 함께 드래그해서 이동',
    );
    expect(dragHandles[1].getAttribute('title')).toBe('드래그해서 순서 이동');
    const table = within(screen.getByTestId('desktop-task-table'));
    const lastRow = table.getByRole('button', { name: 'Task 4' }).closest('tr');

    fireEvent.dragStart(dragHandles[2], { dataTransfer: dragData });
    fireEvent.drop(lastRow as HTMLTableRowElement, { dataTransfer: dragData });

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 2666, taskId: 'task-1' },
      { boardPosition: 3332, taskId: 'task-3' },
    ]);
  });

  it('drags only the grabbed row when it is not one of the selected rows', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['task-1', 'task-3'])}
        tasks={[1, 2, 3, 4].map((number) =>
          task({
            board_position: number * 1000,
            id: `task-${number}`,
            reference: `TASK-${number}`,
            title: `Task ${number}`,
          }),
        )}
      />,
    );

    const dragData = dataTransfer();
    const dragHandles = screen.getAllByRole('button', {
      name: '드래그해서 순서 이동',
    });
    const table = within(screen.getByTestId('desktop-task-table'));
    const lastRow = table.getByRole('button', { name: 'Task 4' }).closest('tr');

    fireEvent.dragStart(dragHandles[1], { dataTransfer: dragData });
    fireEvent.drop(lastRow as HTMLTableRowElement, { dataTransfer: dragData });

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 3500, taskId: 'task-2' },
    ]);
  });

  it('leaves a selected row hidden under a collapsed parent out of the drag', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['child', 'a', 'b'])}
        tasks={[
          task({ board_position: 1000, id: 'parent', title: 'Parent' }),
          task({
            board_position: 1000,
            id: 'child',
            parent_id: 'parent',
            title: 'Child',
          }),
          task({ board_position: 2000, id: 'a', title: 'A' }),
          task({ board_position: 3000, id: 'b', title: 'B' }),
        ]}
      />,
    );

    const table = within(screen.getByTestId('desktop-task-table'));
    fireEvent.click(table.getByRole('button', { name: '하위 태스크 접기' }));
    expect(table.queryByRole('button', { name: 'Child' })).toBeNull();

    const dragData = dataTransfer();
    const aRow = table.getByRole('button', { name: 'A' }).closest('tr');
    const parentRow = table
      .getByRole('button', { name: 'Parent' })
      .closest('tr');
    const aHandle = within(aRow as HTMLTableRowElement).getByRole('button', {
      name: '드래그해서 순서 이동',
    });

    fireEvent.dragStart(aHandle, { dataTransfer: dragData });
    fireEvent.drop(parentRow as HTMLTableRowElement, {
      dataTransfer: dragData,
    });

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 333, taskId: 'a' },
      { boardPosition: 666, taskId: 'b' },
    ]);
  });

  function fourTasks() {
    return [1, 2, 3, 4].map((number) =>
      task({
        board_position: number * 1000,
        id: `task-${number}`,
        reference: `TASK-${number}`,
        title: `Task ${number}`,
      }),
    );
  }

  function startMoveFromRow(title: string, menuItemName: string) {
    const table = within(screen.getByTestId('desktop-task-table'));
    const row = table.getByRole('button', { name: title }).closest('tr');
    fireEvent.click(
      within(row as HTMLTableRowElement).getByRole('button', { name: '작업' }),
    );
    fireEvent.click(screen.getByRole('button', { name: menuItemName }));
  }

  it('moves a task straight to a spot far down the list without dragging', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        tasks={fourTasks()}
      />,
    );

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    expect(screen.getByTestId('task-move-pending').textContent).toContain(
      '옮길 태스크 1개를 넣을 자리를 고르세요.',
    );
    // The moving row offers no spot for itself.
    expect(
      screen.queryByRole('button', { name: 'TASK-1 위에 넣기' }),
    ).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'TASK-4 위에 넣기' }));

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 3500, taskId: 'task-1' },
    ]);
    await waitFor(() => {
      expect(screen.queryByTestId('task-move-pending')).toBeNull();
    });
  });

  it('moves every selected row together and cancels with Escape', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['task-1', 'task-3'])}
        tasks={fourTasks()}
      />,
    );

    startMoveFromRow('Task 1', '선택한 2개 다른 자리로 옮기기');
    expect(screen.getByTestId('task-move-pending').textContent).toContain(
      '옮길 태스크 2개',
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('task-move-pending')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'TASK-4 아래에 넣기' }),
    ).toBeNull();

    startMoveFromRow('Task 3', '선택한 2개 다른 자리로 옮기기');
    fireEvent.click(screen.getByRole('button', { name: 'TASK-4 아래에 넣기' }));

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 5000, taskId: 'task-1' },
      { boardPosition: 6000, taskId: 'task-3' },
    ]);
  });

  it('disables a spot that would leave the order unchanged', () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        tasks={fourTasks()}
      />,
    );

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    const aboveSecond = screen.getByRole('button', {
      name: 'TASK-2 위에 넣기',
    }) as HTMLButtonElement;
    expect(aboveSecond.disabled).toBe(true);
    fireEvent.click(aboveSecond);
    expect(onReorderIssues).not.toHaveBeenCalled();
    expect(
      (
        screen.getByRole('button', {
          name: 'TASK-2 아래에 넣기',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });

  it('disables the unchanged spot for tasks numbered 1, 2, 3', () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        tasks={fourTasks().map((item, index) => ({
          ...item,
          board_position: index + 1,
        }))}
      />,
    );

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    const aboveSecond = screen.getByRole('button', {
      name: 'TASK-2 위에 넣기',
    }) as HTMLButtonElement;
    expect(aboveSecond.disabled).toBe(true);
    fireEvent.click(aboveSecond);
    expect(onReorderIssues).not.toHaveBeenCalled();
    expect(
      (
        screen.getByRole('button', {
          name: 'TASK-3 위에 넣기',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });

  it('drops a pending move when the list leaves custom order', () => {
    const tasks = fourTasks();
    const { rerender } = render(
      <ListView
        canEdit
        onReorderIssues={vi.fn()}
        onSelectIssue={vi.fn()}
        onSortChange={vi.fn()}
        sort={{ direction: 'asc', field: 'board_position' }}
        tasks={tasks}
      />,
    );

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    expect(screen.getByTestId('task-move-pending')).not.toBeNull();

    rerender(
      <ListView
        canEdit
        onReorderIssues={vi.fn()}
        onSelectIssue={vi.fn()}
        onSortChange={vi.fn()}
        sort={{ direction: 'asc', field: 'title' }}
        tasks={tasks}
      />,
    );
    expect(screen.queryByTestId('task-move-pending')).toBeNull();

    rerender(
      <ListView
        canEdit
        onReorderIssues={vi.fn()}
        onSelectIssue={vi.fn()}
        onSortChange={vi.fn()}
        sort={{ direction: 'asc', field: 'board_position' }}
        tasks={tasks}
      />,
    );
    expect(screen.queryByTestId('task-move-pending')).toBeNull();
  });

  it('offers no move outside the custom order', () => {
    render(
      <ListView
        canEdit
        onReorderIssues={vi.fn()}
        onSelectIssue={vi.fn()}
        onSortChange={vi.fn()}
        onUpdateIssue={vi.fn()}
        sort={{ direction: 'asc', field: 'title' }}
        tasks={fourTasks()}
      />,
    );

    const table = within(screen.getByTestId('desktop-task-table'));
    const row = table.getByRole('button', { name: 'Task 1' }).closest('tr');
    fireEvent.click(
      within(row as HTMLTableRowElement).getByRole('button', { name: '작업' }),
    );
    expect(screen.getByRole('button', { name: '이름 변경' })).not.toBeNull();
    expect(
      screen.queryByRole('button', { name: '다른 자리로 옮기기' }),
    ).toBeNull();
  });

  it('places and cancels a move from the row menu in the ungrouped list', async () => {
    const onReorderIssues = vi.fn().mockResolvedValue(undefined);
    render(
      <ListView
        canEdit
        groupBy="none"
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        tasks={fourTasks()}
      />,
    );

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    const table = within(screen.getByTestId('desktop-task-table'));
    const firstRow = table
      .getByRole('button', { name: 'Task 1' })
      .closest('tr');
    fireEvent.click(
      within(firstRow as HTMLTableRowElement).getByRole('button', {
        name: '작업',
      }),
    );
    // The banner has its own cancel button; the menu (a portal) comes last.
    const cancelButtons = screen.getAllByRole('button', {
      name: '옮기기 취소',
    });
    expect(cancelButtons).toHaveLength(2);
    fireEvent.click(cancelButtons[cancelButtons.length - 1]);
    expect(screen.queryByTestId('task-move-pending')).toBeNull();

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    startMoveFromRow('Task 3', '이 태스크 아래에 넣기');

    await waitFor(() => {
      expect(onReorderIssues).toHaveBeenCalledTimes(1);
    });
    expect(onReorderIssues).toHaveBeenCalledWith([
      { boardPosition: 3500, taskId: 'task-1' },
    ]);
  });

  it('keeps a pending move when Escape is pressed inside a field', () => {
    render(
      <ListView
        canEdit
        onReorderIssues={vi.fn()}
        onSelectIssue={vi.fn()}
        tasks={fourTasks()}
      />,
    );

    startMoveFromRow('Task 1', '다른 자리로 옮기기');
    fireEvent.keyDown(
      within(screen.getByTestId('desktop-task-table')).getByRole('checkbox', {
        name: 'TASK-2 선택',
      }),
      { key: 'Escape' },
    );
    expect(screen.getByTestId('task-move-pending')).not.toBeNull();
  });

  it('drops a moving row that leaves the list for good', () => {
    const tasks = fourTasks();
    const props = {
      canEdit: true,
      onReorderIssues: vi.fn(),
      onSelectIssue: vi.fn(),
      onToggleSelect: vi.fn(),
      selectedIds: new Set(['task-1', 'task-3']),
    };
    const { rerender } = render(<ListView {...props} tasks={tasks} />);

    startMoveFromRow('Task 1', '선택한 2개 다른 자리로 옮기기');
    rerender(
      <ListView {...props} tasks={tasks.filter(({ id }) => id !== 'task-3')} />,
    );
    expect(screen.getByTestId('task-move-pending').textContent).toContain(
      '옮길 태스크 1개',
    );

    rerender(<ListView {...props} tasks={tasks} />);
    expect(screen.getByTestId('task-move-pending').textContent).toContain(
      '옮길 태스크 1개',
    );
  });

  it('says so when the chosen rows have no place to go together', () => {
    const onReorderIssues = vi.fn();
    render(
      <ListView
        canEdit
        onReorderIssues={onReorderIssues}
        onSelectIssue={vi.fn()}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['task-1', 'task-2'])}
        tasks={[
          task({ id: 'task-1', reference: 'TASK-1', title: 'Task 1' }),
          task({
            board_position: 2000,
            id: 'task-2',
            reference: 'TASK-2',
            status: 'in_progress',
            title: 'Task 2',
          }),
          task({
            board_position: 3000,
            id: 'task-3',
            reference: 'TASK-3',
            title: 'Task 3',
          }),
        ]}
      />,
    );

    const firstRow = screen
      .getAllByRole('button', { name: 'Task 1' })
      .map((button) => button.closest('tr'))
      .find(Boolean);
    fireEvent.click(
      within(firstRow as HTMLTableRowElement).getByRole('button', {
        name: '작업',
      }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: '선택한 2개 다른 자리로 옮기기' }),
    );

    expect(screen.getByTestId('task-move-pending').textContent).toContain(
      '넣을 수 있는 자리가 없습니다',
    );
    const placeAbove = screen.getByRole('button', {
      name: 'TASK-3 위에 넣기',
    }) as HTMLButtonElement;
    expect(placeAbove.disabled).toBe(true);
    fireEvent.click(placeAbove);
    expect(onReorderIssues).not.toHaveBeenCalled();
  });

  it('selects a row from anywhere in its checkbox cell without opening it', () => {
    const onSelectIssue = vi.fn();
    render(
      <ListView
        onSelectIssue={onSelectIssue}
        tasks={[task({ id: 'a', reference: 'TASK-A', title: 'A' })]}
      />,
    );

    const table = within(screen.getByTestId('desktop-task-table'));
    const checkbox = table.getByRole('checkbox', {
      name: 'TASK-A 선택',
    }) as HTMLInputElement;
    const cell = checkbox.closest('td') as HTMLTableCellElement;

    fireEvent.click(cell);
    expect(checkbox.checked).toBe(true);
    fireEvent.click(checkbox);
    expect(checkbox.checked).toBe(false);
    fireEvent.click(
      table.getByRole('button', { name: '드래그해서 순서 이동' }),
    );
    expect(checkbox.checked).toBe(false);
    expect(onSelectIssue).not.toHaveBeenCalled();
  });

  it('selects every row of one assignee group from that table header', () => {
    const onSelectTasks = vi.fn();
    render(
      <ListView
        groupBy="assignee"
        members={[
          member({ full_name: 'Alice Kim', user_id: 'user-1' }),
          member({ full_name: 'Bob Lee', user_id: 'user-2' }),
        ]}
        onSelectIssue={vi.fn()}
        onSelectTasks={onSelectTasks}
        onToggleSelect={vi.fn()}
        selectedIds={new Set(['root-1'])}
        tasks={[
          task({
            assignee_id: 'user-1',
            assignee_ids: ['user-1'],
            assignee_names: ['Alice Kim'],
            id: 'root-1',
            title: 'Alice root',
          }),
          task({
            board_position: 2000,
            id: 'child-1',
            parent_id: 'root-1',
            title: 'Alice child',
          }),
          task({
            assignee_id: 'user-2',
            assignee_ids: ['user-2'],
            assignee_names: ['Bob Lee'],
            board_position: 3000,
            id: 'root-2',
            title: 'Bob root',
          }),
        ]}
      />,
    );

    const aliceHeader = within(
      screen.getByTestId('assignee-task-group-user-1'),
    ).getByRole('checkbox', {
      name: '이 표의 태스크 2개 모두 선택',
    }) as HTMLInputElement;
    expect(aliceHeader.indeterminate).toBe(true);

    fireEvent.click(aliceHeader);
    expect(onSelectTasks).toHaveBeenCalledWith(['root-1', 'child-1'], true);

    fireEvent.click(
      within(screen.getByTestId('assignee-task-group-user-2')).getByRole(
        'checkbox',
        { name: '이 표의 태스크 1개 모두 선택' },
      ),
    );
    expect(onSelectTasks).toHaveBeenLastCalledWith(['root-2'], true);
  });

  it('selects and clears every row when the list owns its selection', () => {
    render(
      <ListView
        groupBy="none"
        onSelectIssue={vi.fn()}
        tasks={[
          task({ id: 'a', reference: 'TASK-A', title: 'A' }),
          task({
            board_position: 2000,
            id: 'b',
            reference: 'TASK-B',
            title: 'B',
          }),
        ]}
      />,
    );

    const header = screen.getByRole('checkbox', {
      name: '이 표의 태스크 2개 모두 선택',
    });
    const rowChecked = (reference: string) =>
      screen
        .getAllByRole('checkbox', { name: `${reference} 선택` })
        .every((checkbox) => (checkbox as HTMLInputElement).checked);

    fireEvent.click(header);
    expect(rowChecked('TASK-A')).toBe(true);
    expect(rowChecked('TASK-B')).toBe(true);

    fireEvent.click(header);
    expect(rowChecked('TASK-A')).toBe(false);
    expect(rowChecked('TASK-B')).toBe(false);
  });

  it('hides the header checkbox when the selection owner cannot select many rows', () => {
    render(
      <ListView
        groupBy="none"
        onSelectIssue={vi.fn()}
        onToggleSelect={vi.fn()}
        selectedIds={new Set()}
        tasks={[task()]}
      />,
    );

    expect(
      screen.queryByRole('checkbox', { name: '이 표의 태스크 1개 모두 선택' }),
    ).toBeNull();
  });
});
