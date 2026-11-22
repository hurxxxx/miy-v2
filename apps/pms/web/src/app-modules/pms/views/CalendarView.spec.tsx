import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n/i18n';
import type { PmsTask } from '../api/pms-api';
import { CalendarView } from './CalendarView';

describe('CalendarView', () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-15T00:00:00+09:00'));
    await i18n.changeLanguage('ko-KR');
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('moves to the next month from the calendar toolbar', () => {
    render(
      <CalendarView
        tasks={[
          task({ id: 'may-task', title: 'May task', due_date: '2026-05-20' }),
          task({
            id: 'june-task',
            title: 'June task',
            due_date: '2026-06-15',
          }),
        ]}
      />,
    );

    expect(screen.queryByText('May task')).not.toBeNull();
    expect(screen.queryByText('June task')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '다음 달' }));

    expect(screen.queryByText('May task')).toBeNull();
    expect(screen.queryByText('June task')).not.toBeNull();
  });

  it('moves to the previous year from the calendar toolbar', () => {
    render(
      <CalendarView
        tasks={[
          task({
            id: 'current-year-task',
            title: 'Current year task',
            due_date: '2026-05-20',
          }),
          task({
            id: 'previous-year-task',
            title: 'Previous year task',
            due_date: '2025-05-20',
          }),
        ]}
      />,
    );

    expect(screen.queryByText('Current year task')).not.toBeNull();
    expect(screen.queryByText('Previous year task')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '이전 해' }));

    expect(screen.queryByText('Current year task')).toBeNull();
    expect(screen.queryByText('Previous year task')).not.toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '다음 해' }));

    expect(screen.queryByText('Current year task')).not.toBeNull();
    expect(screen.queryByText('Previous year task')).toBeNull();
  });

  it('shows only this week after switching to the week range', () => {
    render(
      <CalendarView
        tasks={[
          task({ id: 'this-week', title: 'This week', due_date: '2026-05-14' }),
          task({ id: 'next-week', title: 'Next week', due_date: '2026-05-20' }),
          task({
            id: 'long-task',
            title: 'Long task',
            start_date: '2026-05-01',
            due_date: '2026-05-31',
          }),
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '주간' }));

    expect(
      screen.getByRole('button', { name: '주간' }).getAttribute('aria-pressed'),
    ).toBe('true');
    expect(screen.queryByText('This week')).not.toBeNull();
    expect(screen.queryByText('Long task')).not.toBeNull();
    expect(screen.queryByText('Next week')).toBeNull();
    expect(screen.queryByRole('button', { name: '이전 해' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '다음 주' }));

    expect(screen.queryByText('This week')).toBeNull();
    expect(screen.queryByText('Next week')).not.toBeNull();
  });

  it('shows only one day in the day range and says when it is empty', () => {
    render(
      <CalendarView
        tasks={[
          task({ id: 'today', title: 'Today task', due_date: '2026-05-15' }),
          task({
            id: 'tomorrow',
            title: 'Tomorrow task',
            due_date: '2026-05-16',
          }),
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '일간' }));

    expect(screen.queryByText('Today task')).not.toBeNull();
    expect(screen.queryByText('Tomorrow task')).toBeNull();
    // The message slot stays mounted with a fixed height, so the calendar never shifts.
    const slot = screen.getByRole('status');
    expect(slot.getAttribute('data-state')).toBe('empty');

    fireEvent.click(screen.getByRole('button', { name: '다음 날' }));
    fireEvent.click(screen.getByRole('button', { name: '다음 날' }));

    expect(screen.queryByText('Tomorrow task')).toBeNull();
    expect(screen.getByRole('status')).toBe(slot);
    expect(slot.textContent).toBe('이 기간에 태스크가 없습니다.');

    fireEvent.click(screen.getByRole('button', { name: '오늘' }));

    expect(screen.queryByText('Today task')).not.toBeNull();
  });

  it('switches to the first week of the shown month when today is not in it', () => {
    render(
      <CalendarView
        tasks={[
          task({ id: 'may', title: 'May task', due_date: '2026-05-14' }),
          task({ id: 'june', title: 'June task', due_date: '2026-06-02' }),
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '다음 달' }));
    fireEvent.click(screen.getByRole('button', { name: '주간' }));

    expect(screen.queryByText('June task')).not.toBeNull();
    expect(screen.queryByText('May task')).toBeNull();
  });

  it('disables the today button while today is already shown', () => {
    render(<CalendarView tasks={[]} />);
    const todayButton = () =>
      screen.getByRole('button', { name: '오늘' }) as HTMLButtonElement;

    expect(todayButton().disabled).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: '다음 달' }));
    expect(todayButton().disabled).toBe(false);

    fireEvent.click(todayButton());
    expect(todayButton().disabled).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: '일간' }));
    expect(todayButton().disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '이전 날' }));
    expect(todayButton().disabled).toBe(false);
  });

  it('marks today on the calendar grid', () => {
    render(<CalendarView tasks={[]} />);

    const todayNumber = document.querySelector(
      '.fc-daygrid-day.fc-day-today .fc-daygrid-day-number',
    );

    expect(todayNumber?.textContent).toContain('15');
  });

  it('expands busy weeks vertically and keeps every task visible', () => {
    render(
      <CalendarView
        tasks={Array.from({ length: 5 }, (_, index) =>
          task({
            id: `task-${index}`,
            title: `Task ${index}`,
            due_date: '2026-05-20',
          }),
        )}
      />,
    );

    expect(screen.queryByText('Task 0')).not.toBeNull();
    expect(screen.queryByText('Task 3')).not.toBeNull();
    expect(screen.queryByText('Task 4')).not.toBeNull();
    expect(screen.queryByText('+1개')).toBeNull();
  });

  it('renders a same-week multi-day task once as a spanning bar', () => {
    render(
      <CalendarView
        tasks={[
          task({
            id: 'span-task',
            title: 'Span task',
            start_date: '2026-05-07',
            due_date: '2026-05-09',
          }),
        ]}
      />,
    );

    expect(screen.getAllByText('Span task')).toHaveLength(1);
  });

  it('colors task bars from their workflow status', () => {
    render(
      <CalendarView
        tasks={[
          task({
            id: 'todo-task',
            title: 'Todo task',
            due_date: '2026-05-20',
            status: 'todo',
          }),
          task({
            id: 'progress-task',
            title: 'Progress task',
            due_date: '2026-05-21',
            status: 'in_progress',
          }),
        ]}
      />,
    );

    const todoTask = getCalendarEventElement('Todo task');
    const progressTask = getCalendarEventElement('Progress task');

    expect(todoTask.style.backgroundColor).not.toBe(
      progressTask.style.backgroundColor,
    );
  });

  it('shows task details on event hover', () => {
    render(
      <CalendarView
        tasks={[
          task({
            id: 'tooltip-task',
            title: 'Tooltip task',
            due_date: '2026-05-20',
            assignee_names: ['Kim'],
          }),
        ]}
      />,
    );

    const eventElement = getCalendarEventElement('Tooltip task');
    fireEvent.mouseEnter(eventElement);

    const tooltip = screen.getByRole('tooltip');
    expect(tooltip.textContent).toContain('Todo');
    expect(tooltip.textContent).toContain('Kim');
    expect(tooltip.textContent).toContain('2026. 5. 20.');

    fireEvent.mouseLeave(eventElement);

    expect(screen.queryByRole('tooltip')).toBeNull();
  });
});

function getCalendarEventElement(title: string): HTMLElement {
  const eventElement = screen.getByText(title).closest('.fc-event');
  if (!(eventElement instanceof HTMLElement)) {
    throw new Error(`Calendar event not found: ${title}`);
  }
  return eventElement;
}

function task(overrides: Partial<PmsTask> = {}): PmsTask {
  return {
    custom_fields: {},
    archived: false,
    assignee_id: null,
    assignee_ids: [],
    assignee_name: null,
    assignee_names: [],
    board_position: 1,
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
    progress: 0,
    recurrence_rule: null,
    reference: 'AID-1',
    reporter_id: 'user-1',
    reporter_name: 'Reporter',
    start_date: null,
    status: 'todo',
    status_label: 'Todo',
    subtask_count: 0,
    title: 'Task title',
    updated_at: '2026-05-21T00:00:00Z',
    ...overrides,
  };
}
