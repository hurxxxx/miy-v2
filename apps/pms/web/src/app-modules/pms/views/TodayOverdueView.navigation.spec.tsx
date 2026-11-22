import { cleanup, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  createTestRuntime,
  renderWithRuntime,
} from '../../../test-utils/runtime';
import { PMS_WORKSPACE_KEY } from '../../../platform/runtime/app-location';
import {
  listAllPmsTaskLists,
  listAllTodayOverdueTasks,
  type PmsTask,
  type PmsTaskList,
} from '../api/pms-api';
import { TodayOverdueView } from './TodayOverdueView';

vi.mock('../api/pms-api', () => ({
  listAllPmsTaskLists: vi.fn(),
  listAllTodayOverdueTasks: vi.fn(),
}));
vi.mock('./usePmsTaskListChangeSubscription', () => ({
  usePmsTaskListChangeSubscription: () => undefined,
}));

const taskList = (id: string, workspaceKey: string | null): PmsTaskList => ({
  id,
  key: id,
  name: id,
  description: '',
  status: 'active',
  status_mode: 'custom',
  workspace_id: workspaceKey,
  workspace_key: workspaceKey,
  team_id: workspaceKey,
  team_name: workspaceKey,
  folder_id: null,
  folder_name: null,
  sort_order: 0,
  role: 'viewer',
  archived: false,
  progress: 0,
  member_count: 1,
  milestone_count: 0,
  task_count: 1,
  active_task_count: 1,
  open_task_count: 1,
  overdue_task_count: 0,
  created_at: '2026-11-12T00:00:00Z',
  updated_at: '2026-11-12T00:00:00Z',
});

const task = (listId: string, dueDate: string): PmsTask => ({
  id: `task-${listId}`,
  list_id: listId,
  title: `Task ${listId}`,
  reference: listId,
  archived: false,
  assignee_id: null,
  assignee_ids: [],
  assignee_name: null,
  assignee_names: [],
  board_position: 0,
  checklist_done: 0,
  checklist_total: 0,
  comments_count: 0,
  description: '',
  description_blocks: null,
  due_date: dueDate,
  follower_ids: [],
  follower_names: [],
  labels: [],
  milestone_id: null,
  milestone_title: null,
  parent_id: null,
  priority: 'medium',
  priority_label: 'Medium',
  progress: 0,
  recurrence_rule: null,
  reporter_id: 'user',
  reporter_name: 'User',
  start_date: null,
  status: 'open',
  status_label: 'Open',
  subtask_count: 0,
  updated_at: '2026-11-12T00:00:00Z',
  completed_date: null,
});

function setResults(lists: PmsTaskList[], tasks: PmsTask[]) {
  vi.mocked(listAllPmsTaskLists).mockResolvedValue({
    items: lists,
    page: 1,
    page_size: 100,
    total: lists.length,
  });
  vi.mocked(listAllTodayOverdueTasks).mockResolvedValue({
    items: tasks,
    page: 1,
    page_size: 100,
    total: tasks.length,
  });
}

function renderToday() {
  // AI-DO read the user's profile time zone (UTC in this case); miy reads the
  // portal display setting.
  const runtime = createTestRuntime({
    path: '/today',
    settings: { timeZone: 'UTC' },
  });
  return renderWithRuntime(<TodayOverdueView />, { runtime });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date('2026-11-12T12:00:00Z'));
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('TodayOverdueView task navigation', () => {
  it('keeps aggregate reads global and opens today/overdue tasks on their lists', async () => {
    setResults(
      [taskList('list-b', 'pms-b'), taskList('list-a', 'pms-a')],
      [task('list-a', '2026-11-12'), task('list-b', '2026-11-11')],
    );
    const { runtime } = renderToday();

    expect(
      (await screen.findByRole('link', { name: /Task list-a/ })).getAttribute(
        'href',
      ),
    ).toBe('/lists/list-a?task=task-list-a');
    expect(
      screen.getByRole('link', { name: /Task list-b/ }).getAttribute('href'),
    ).toBe('/lists/list-b?task=task-list-b');
    expect(listAllPmsTaskLists).toHaveBeenCalledWith(
      runtime.api,
      undefined,
      PMS_WORKSPACE_KEY,
    );
    expect(listAllTodayOverdueTasks).toHaveBeenCalledWith(
      runtime.api,
      '2026-11-12',
      PMS_WORKSPACE_KEY,
    );
  });

  it('links a readable task by its list id even without the list in the catalog', async () => {
    setResults([], [task('list-a', '2026-11-12')]);
    renderToday();
    expect(
      (await screen.findByRole('link', { name: /Task list-a/ })).getAttribute(
        'href',
      ),
    ).toBe('/lists/list-a?task=task-list-a');
  });
});
