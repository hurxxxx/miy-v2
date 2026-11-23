import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { AppRuntime, AppSettings } from '@miy/sdk';
import { ApiError } from '@miy/sdk';
import { FeedbackProvider } from '@miy/ui';

import AppRoot from './AppRoot';
import { SpaceOrderEditorModal } from './app-modules/pms/sidebar/SpaceOrderEditorModal';
import { CustomFieldSettingsSection } from './app-modules/pms/views/CustomFieldSettingsSection';
import type { PmsTaskList } from './app-modules/pms/api/pms-api';
import { syncLocale } from './platform/i18n';
import { NavigationStateProvider } from './platform/runtime/navigation-state';
import { RuntimeProvider } from './platform/runtime/runtime-context';
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

  it('updates the open my-tasks panel date without fetching the tasks again', async () => {
    const api = fakeApi({
      '/v1/pms/lists?': { ...EMPTY_PAGE, items: [{ id: 'list-1', team_id: SPACE.id }], total: 1 },
      '/v1/pms/tasks/assigned': {
        ...EMPTY_PAGE, items: [{ ...TASK, due_date: '2026-11-23' }], total: 1,
      },
    });
    const value = runtime('/widgets/my-tasks', api, { dateFormat: 'iso' });
    const { rerender } = render(<AppRoot runtime={value} />);
    expect(await screen.findByText('2026-11-23', {}, LAZY)).toBeTruthy();
    const assignedReads = () => api.mock.calls.filter(([path]) => path.startsWith('/v1/pms/tasks/assigned'));

    rerender(<AppRoot runtime={{ ...value, settings: { ...value.settings, dateFormat: 'us' } }} />);

    expect(screen.getByText('11/23/2026')).toBeTruthy();
    expect(screen.queryByText('2026-11-23')).toBeNull();
    expect(assignedReads()).toHaveLength(1);
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

  it('loads only the selected list and keeps it on query or display setting changes', async () => {
    const api = fakeApi({
      '/v1/pms/lists/list-1/statuses': { items: [] },
      '/v1/pms/lists/list-1': { id: 'list-1', name: '납품 일정', role: 'admin' },
    });
    const value = runtime('/', api);
    const { rerender } = render(<AppRoot runtime={value} />);
    await screen.findByRole('heading', { name: 'PMS 개요' }, LAZY);
    api.mockClear();

    const listLocation = { ...value.location, pathname: '/lists/list-1' };
    rerender(<AppRoot runtime={{ ...value, location: listLocation }} />);
    await screen.findByText('납품 일정', {}, LAZY);
    const boardLocation = { ...listLocation, search: '?tab=board' };
    rerender(<AppRoot runtime={{ ...value, location: boardLocation }} />);
    rerender(
      <AppRoot runtime={{
        ...value,
        location: boardLocation,
        settings: { ...value.settings, locale: 'en-US' },
      }} />,
    );

    const paths = api.mock.calls.map(([path]) => path);
    expect(paths.filter((path) => path === '/v1/pms/lists/list-1')).toHaveLength(1);
    expect(paths.some((path) =>
      path === '/v1/pms/me' || path === '/v1/pms/spaces' || path.startsWith('/v1/pms/lists?'),
    )).toBe(false);
  });

  it('keeps a deep-link read on query changes and ignores it after leaving', async () => {
    let finishTask!: (value: typeof TASK_DETAIL) => void;
    const pending = new Promise<typeof TASK_DETAIL>((resolve) => {
      finishTask = resolve;
    });
    const api = fakeApi({ '/v1/pms/tasks/task-1': pending });
    const value = runtime('/tasks/task-1', api);
    const { rerender } = render(<AppRoot runtime={value} />);
    const taskReads = () => api.mock.calls.filter(([path]) => path === '/v1/pms/tasks/task-1');
    await waitFor(() => expect(taskReads()).toHaveLength(1), LAZY);

    rerender(<AppRoot runtime={{ ...value, location: { ...value.location, search: '?detail=1' } }} />);
    expect(taskReads()).toHaveLength(1);

    rerender(<AppRoot runtime={{ ...value, location: { ...value.location, pathname: '/' } }} />);
    await screen.findByRole('heading', { name: 'PMS 개요' }, LAZY);
    await act(async () => { finishTask(TASK_DETAIL); });
    expect(api.mock.calls.some(([path]) => path === '/v1/pms/lists/list-1')).toBe(false);
    expect(value.navigate).not.toHaveBeenCalled();
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

it('refreshes changed custom field options without silently rebasing the draft', async () => {
  const field = { id: 'field-1', name: '분류', field_type: 'select', options: ['기존'] };
  let options = field.options;
  const api = vi.fn(async (_path: string, init?: RequestInit) => {
    if (init?.method === 'PATCH') {
      options = ['기존', '동시 변경'];
      throw new ApiError('http', 'pms_custom_field_options_changed', 409);
    }
    return { items: [{ ...field, options }] };
  });
  const value = createTestRuntime({ api: api as unknown as AppRuntime['api'] });
  render(
    <FeedbackProvider labels={{ close: '닫기', item: '알림', region: '피드백' }}>
      <RuntimeProvider runtime={value}>
        <NavigationStateProvider report={value.setNavigationState} message="Unsaved work">
          <CustomFieldSettingsSection taskListId="list-1" canManage />
        </NavigationStateProvider>
      </RuntimeProvider>
    </FeedbackProvider>,
  );

  fireEvent.click(await screen.findByRole('button', { name: '분류 수정' }));
  fireEvent.change(screen.getByRole('textbox', { name: '기존 선택지 이름' }), {
    target: { value: '내 변경' },
  });
  fireEvent.click(screen.getByRole('button', { name: '저장' }));
  expect((await screen.findByRole('alert')).textContent).toContain('다른 변경으로 선택지가 바뀌었습니다');
  expect((screen.getByRole('textbox', { name: '기존 선택지 이름' }) as HTMLInputElement).value).toBe('내 변경');
  expect((screen.getByRole('button', { name: '저장' }) as HTMLButtonElement).disabled).toBe(true);
  await waitFor(() => expect(api).toHaveBeenCalledTimes(3));

  fireEvent.click(screen.getByRole('button', { name: '취소' }));
  const reopen = screen.getByRole('button', { name: '분류 수정' }) as HTMLButtonElement;
  await waitFor(() => expect(reopen.disabled).toBe(false));
  fireEvent.click(reopen);
  expect(screen.getByRole('textbox', { name: '동시 변경 선택지 이름' })).toBeTruthy();
  expect((screen.getByRole('button', { name: '저장' }) as HTMLButtonElement).disabled).toBe(false);
});

it('reports unsaved space ordering until its save finishes', async () => {
  syncLocale('en-US');
  const report = vi.fn();
  let finishSave!: () => void;
  const onSave = vi.fn(() => new Promise<void>((resolve) => {
    finishSave = resolve;
  }));
  const folder = {
    id: 'folder-1', team_id: SPACE.id, name: 'Materials', sort_order: 0, list_count: 0,
  };
  const list = {
    id: TASK.list_id, name: 'Inspections', folder_id: null, sort_order: 0,
  } as PmsTaskList;
  function Editor() {
    const [open, setOpen] = useState(true);
    return (
      <SpaceOrderEditorModal
        isOpen={open}
        onClose={() => setOpen(false)}
        spaceName={SPACE.name}
        folders={[folder]}
        lists={[list]}
        onSave={onSave}
      />
    );
  }
  render(
    <NavigationStateProvider report={report} message="Unsaved order">
      <Editor />
    </NavigationStateProvider>,
  );
  expect(report).toHaveBeenLastCalledWith({ dirty: false, message: 'Unsaved order' });

  fireEvent.change(screen.getByRole('combobox'), { target: { value: folder.id } });
  expect(report).toHaveBeenLastCalledWith({ dirty: true, message: 'Unsaved order' });
  fireEvent.click(screen.getByRole('button', { name: /^Save$/ }));
  expect(onSave).toHaveBeenCalledOnce();
  expect(report).toHaveBeenLastCalledWith({ dirty: true, message: 'Unsaved order' });

  await act(async () => { finishSave(); });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(report).toHaveBeenLastCalledWith({ dirty: false, message: 'Unsaved order' });
});
