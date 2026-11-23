import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { AppRuntime, AppSettings } from '@miy/sdk';
import { ApiError } from '@miy/sdk';

import AppRoot from './AppRoot';
import { syncLocale } from './platform/i18n';
import { createTestRuntime } from './test-utils/runtime';

// Smoke tests of the frame entry with a fake runtime (miy). The screens are
// lazy-loaded modules, which takes a while on a busy test run.
const LAZY = { timeout: 10_000 };
const ME = {
  id: 'user-1',
  name: '김테스트',
  primary_org_unit_id: null,
  primary_org_unit_name: null,
};
const SPACE = {
  id: 'space-1',
  workspace_id: 'space-1',
  workspace_key: 'delivery',
  key: 'delivery',
  name: '납품 프로젝트',
  description: '',
  member_count: 1,
  current_user_role: 'owner',
  created_at: '2026-11-01T00:00:00Z',
  updated_at: '2026-11-01T00:00:00Z',
};
const TASK = {
  id: 'task-1',
  list_id: 'list-1',
  reference: 'DL-1',
  title: '자재 검수',
  description: '',
  description_blocks: null,
  parent_id: null,
  subtask_count: 0,
  status: 'todo',
  status_label: 'To Do',
  priority: 'medium',
  priority_label: 'Medium',
  assignee_id: null,
  assignee_name: null,
  assignee_ids: [],
  assignee_names: [],
  follower_ids: [],
  follower_names: [],
  reporter_id: 'user-1',
  reporter_name: '김테스트',
  milestone_id: null,
  milestone_title: null,
  start_date: null,
  due_date: null,
  completed_date: null,
  board_position: 0,
  archived: false,
  progress: null,
  comments_count: 0,
  checklist_total: 0,
  checklist_done: 0,
  recurrence_rule: null,
  labels: [],
  updated_at: '2026-11-01T00:00:00Z',
};
const TASK_DETAIL = {
  task: TASK,
  comments: [],
  linked_docs: [],
  subtasks: [],
  attachments: [],
  checklist_items: [],
};
const EMPTY_PAGE = { items: [], total: 0, page: 1, page_size: 100 };

function fakeApi(overrides: Record<string, unknown> = {}) {
  return vi.fn(async (path: string) => {
    for (const [prefix, value] of Object.entries(overrides)) {
      if (path.startsWith(prefix)) {
        if (value instanceof Error) throw value;
        return value;
      }
    }
    if (path === '/v1/pms/me') return ME;
    if (path === '/v1/pms/spaces') return [SPACE];
    if (path.startsWith('/v1/pms/lists?')) return EMPTY_PAGE;
    if (path.startsWith('/v1/pms/folders')) return { items: [], total: 0 };
    if (path.startsWith('/v1/pms/space-preferences')) return { items: [] };
    if (path.startsWith('/v1/pms/tasks/assigned')) return EMPTY_PAGE;
    throw new ApiError('http', 'http_404', 404);
  }) as unknown as AppRuntime['api'] & ReturnType<typeof vi.fn>;
}

function runtime(
  path: string,
  api = fakeApi(),
  settings: Partial<AppSettings> = {},
): AppRuntime {
  return createTestRuntime({ path, api, settings });
}

// Mantine (BlockNote's UI, in the task detail) reads the colour-scheme media
// query; jsdom has no matchMedia.
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }),
  });
});

afterEach(() => {
  cleanup();
  syncLocale('ko-KR');
});

describe('AppRoot', { timeout: 20_000 }, () => {
  it('opens the PMS overview with the sidebar at the app root', async () => {
    const api = fakeApi();
    const value = runtime('/', api);
    const { rerender } = render(<AppRoot runtime={value} />);
    expect(
      await screen.findByRole('heading', { name: 'PMS 개요' }, LAZY),
    ).toBeTruthy();
    const sidebar = screen.getByRole('complementary', { name: 'PMS 탐색' });
    expect(screen.getByRole('link', { name: '내게 배정됨' })).toBeTruthy();
    expect(api).toHaveBeenCalledWith('/v1/pms/me', { signal: expect.any(AbortSignal) });
    expect(api).toHaveBeenCalledWith('/v1/pms/spaces', {});
    expect(value.setNavigationState).toHaveBeenCalledWith({
      dirty: false,
      message: '저장되지 않은 작업이 있습니다. 이동하시겠습니까?',
    });

    rerender(<AppRoot runtime={{ ...value, location: { ...value.location, pathname: '/assigned' } }} />);
    await waitFor(() => expect(api).toHaveBeenCalledWith(
      expect.stringMatching(/^\/v1\/pms\/tasks\/assigned/),
      expect.anything(),
    ), LAZY);
    expect(screen.getByRole('complementary', { name: 'PMS 탐색' })).toBe(sidebar);
    expect(api.mock.calls.filter(([path]) => path === '/v1/pms/me')).toHaveLength(1);
  });

  it('renders the my-tasks panel route in English without the sidebar', async () => {
    const value = runtime('/widgets/my-tasks', fakeApi(), { locale: 'en-US' });
    render(<AppRoot runtime={value} />);
    expect(
      await screen.findByRole('heading', { name: 'My Active Tasks' }, LAZY),
    ).toBeTruthy();
    expect(
      await screen.findByText('No active tasks are assigned to you.', {}, LAZY),
    ).toBeTruthy();
    expect(screen.queryByRole('complementary')).toBeNull();
    expect(value.setNavigationState).toHaveBeenCalledWith({
      dirty: false,
      message: 'You have unsaved changes. Leave anyway?',
    });
  });

  it('sends a task link to its list when the list is readable', async () => {
    const api = fakeApi({
      '/v1/pms/tasks/task-1': TASK_DETAIL,
      '/v1/pms/lists/list-1': { id: 'list-1' },
    });
    const value = runtime('/tasks/task-1', api);
    render(<AppRoot runtime={value} />);
    await waitFor(
      () =>
        expect(value.navigate).toHaveBeenCalledWith('/lists/list-1?task=task-1', {
          replace: true,
        }),
      LAZY,
    );
  });

  it('shows a shared task read-only when its list is not readable', async () => {
    const api = fakeApi({
      '/v1/pms/tasks/task-1/activity-logs': EMPTY_PAGE,
      '/v1/pms/tasks/task-1': TASK_DETAIL,
      '/v1/pms/lists/list-1': new ApiError('http', 'pms_task_list_access_required', 403),
    });
    const value = runtime('/tasks/task-1', api);
    render(<AppRoot runtime={value} />);
    expect(
      (await screen.findAllByText('자재 검수', {}, LAZY)).length,
    ).toBeGreaterThan(0);
    // Read-only: AI-DO's single-task share renders the detail without editing.
    expect(screen.queryByRole('textbox', { name: '자재 검수' })).toBeNull();
    expect(value.navigate).not.toHaveBeenCalled();
  });
});
