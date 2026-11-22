import type { PmsTask, PmsTaskListStatus } from './api/pms-api';

export function pmsTaskFixture(overrides: Partial<PmsTask> = {}): PmsTask {
  return {
    id: 'task-1',
    list_id: 'list-1',
    reference: 'PMS-1',
    title: 'Task', // i18n-exempt-line: Synthetic user-authored task data consumed only by specs.
    description: '',
    description_blocks: null,
    parent_id: null,
    subtask_count: 0,
    status: 'todo',
    status_label: 'Todo',
    priority: 'medium',
    priority_label: 'Medium',
    assignee_id: null,
    assignee_name: null,
    assignee_ids: [],
    assignee_names: [],
    follower_ids: [],
    follower_names: [],
    reporter_id: 'actor',
    reporter_name: 'Actor',
    milestone_id: null,
    milestone_title: null,
    start_date: null,
    due_date: null,
    completed_date: null,
    board_position: 0,
    archived: false,
    progress: 0,
    comments_count: 0,
    checklist_total: 0,
    checklist_done: 0,
    recurrence_rule: null,
    labels: [],
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

export function pmsStatusFixture(
  overrides: Partial<PmsTaskListStatus> = {},
): PmsTaskListStatus {
  return {
    id: 'status-1',
    slug: 'todo',
    name: 'Todo',
    color: '#94a3b8',
    category: 'not_started',
    sort_order: 0,
    ...overrides,
  };
}
