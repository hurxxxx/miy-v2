import { act, cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppRuntime } from '@miy/sdk';

import {
  createTestRuntime,
  renderWithRuntime,
} from '../../../test-utils/runtime';
import type { PmsCurrentUser } from '../../../platform/auth/current-user';
import type { PmsSpace, PmsTask, PmsTaskList } from '../api/pms-api';
import { PMSView } from './PMSView';
import { dispatchPmsSpaceChanged } from './pms-events';

// miy: PMSView runs on the app-relative routes (`/lists/<id>`, `/`) with the
// frame runtime; AI-DO's version of this spec ran it in the app-first space
// runtime (`/pms/s/<key>/...`) with workspace-scoped API paths.
const state = vi.hoisted(() => ({
  user: {
    id: 'reader',
    name: 'Reader',
    primary_org_unit_id: null,
    primary_org_unit_name: null,
  } as PmsCurrentUser,
  feedback: { error: vi.fn(), success: vi.fn() },
}));
vi.mock('@miy/ui', async (original) => ({
  ...(await original<typeof import('@miy/ui')>()),
  useFeedback: () => state.feedback,
  useConfirm: () => ({ confirm: vi.fn(), confirmDialog: null }),
  usePrompt: () => ({ prompt: vi.fn(), promptDialog: null }),
}));
vi.mock('./CreateSpaceModal', () => ({
  CreateSpaceModal: ({
    onCreated,
  }: {
    onCreated: (space: PmsSpace) => void;
  }) => (
    <button
      onClick={() =>
        onCreated({
          id: 'created-space',
          workspace_id: 'created-space',
          workspace_key: 'new-pms-owner',
          key: 'new-pms-owner',
          name: 'Created space',
          description: '',
          member_count: 1,
          current_user_role: 'owner',
          created_at: '',
          updated_at: '',
        })
      }
    >
      Complete new space
    </button>
  ),
}));
vi.mock('./FilterBar', () => ({ FilterBar: () => null }));
vi.mock('./TaskDetail', () => ({ TaskDetail: () => null }));
vi.mock('./NewTaskModal', () => ({ NewTaskModal: () => null }));
vi.mock('./TaskListSettingsPanel', () => ({
  TaskListSettingsPanel: () => null,
}));
vi.mock('./SpaceTasksView', () => ({ SpaceTasksView: () => null }));
vi.mock('./SpaceOverviewView', () => ({ SpaceOverviewView: () => null }));
vi.mock('./AssignedToMeView', () => ({ AssignedToMeView: () => null }));
vi.mock('./TodayOverdueView', () => ({ TodayOverdueView: () => null }));
vi.mock('./ListView', () => ({
  ListView: ({
    tasks,
    onUpdateIssue,
  }: {
    tasks: PmsTask[];
    onUpdateIssue: (
      id: string,
      payload: Record<string, unknown>,
    ) => Promise<void>;
  }) => (
    <div>
      {tasks.map((task) => (
        <article key={task.id}>{task.title}</article>
      ))}
      {tasks[0] ? (
        <button
          onClick={() =>
            void onUpdateIssue(tasks[0].id, { title: 'Updated task' })
          }
        >
          Update task
        </button>
      ) : null}
    </div>
  ),
}));

const taskList: PmsTaskList = {
  id: 'list-one',
  key: 'LIST',
  name: 'Canonical list',
  description: '',
  workspace_id: 'space-one',
  workspace_key: 'pms-one',
  team_id: 'space-one',
  team_name: 'One',
  role: 'owner',
  archived: false,
  folder_id: null,
  folder_name: null,
  member_count: 1,
  milestone_count: 0,
  open_task_count: 1,
  overdue_task_count: 0,
  progress: 0,
  sort_order: 1,
  status: 'active',
  status_mode: 'custom',
  task_count: 1,
  active_task_count: 1,
  created_at: '',
  updated_at: '',
};

function task(title: string) {
  return {
    id: 'task-one',
    list_id: 'list-one',
    title,
    status: 'todo',
    progress: 0,
    archived: false,
  };
}

let runtime: AppRuntime;

beforeEach(() => {
  state.feedback.error.mockReset();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function renderPmsView(path: string, api: AppRuntime['api']) {
  runtime = createTestRuntime({ path, api });
  return renderWithRuntime(<PMSView />, { runtime, user: state.user });
}

describe('PMS list route task data scope', () => {
  it('loads the list body and reloads an edited task through the explicit owner space', async () => {
    const requests: string[] = [];
    let title = 'Initial task';
    const fetch = vi.fn(async (input: string, init?: RequestInit) => {
        const url = new URL(input, 'http://localhost');
        requests.push(url.pathname);
        const route = url.pathname.replace('/v1/pms', '');
        let data: unknown;
        if (route === '/view-preferences')
          data = { task_list_group_by: 'status' };
        else if (route === '/lists/list-one') data = taskList;
        else if (route === '/lists/list-one/tasks')
          data = { items: [task(title)], total: 1, page: 1, page_size: 100 };
        else if (route === '/tasks/task-one' && init?.method === 'PATCH') {
          title = 'Updated task';
          data = task(title);
        } else if (
          [
            '/lists/list-one/statuses',
            '/lists/list-one/milestones',
            '/lists/list-one/labels',
            '/spaces/space-one/members',
            // miy reinforcement: the list's custom fields.
            '/lists/list-one/custom-fields',
          ].includes(route)
        )
          data = { items: [] };
        // miy: saved filters of the list, kept on the server.
        else if (route === '/saved-filters')
          data = { items: [], default_filter_id: null };
        else throw new Error(`Unexpected test API path: ${url.pathname}`);
        return data;
      });
    renderPmsView('/lists/list-one', fetch as unknown as AppRuntime['api']);
    await waitFor(() =>
      expect(screen.getByRole('main').textContent).toContain('Initial task'),
    );
    expect(screen.queryByText('EXPLICIT_APP_SPACE_REQUIRED')).toBeNull();
    const taskReads = () =>
      requests.filter((path) => path.endsWith('/lists/list-one/tasks')).length;
    const initialReads = taskReads();
    fireEvent.click(screen.getByRole('button', { name: 'Update task' }));
    await screen.findByText('Updated task');
    await waitFor(() => expect(taskReads()).toBeGreaterThan(initialReads));
    expect(screen.queryByText('EXPLICIT_APP_SPACE_REQUIRED')).toBeNull();
    expect(requests.every((path) => path.startsWith('/v1/pms/'))).toBe(true);
    expect(fetch).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Complete new space' }));
    expect(runtime.navigate).toHaveBeenCalledWith('/spaces/created-space', {
      replace: undefined,
    });
  });
});

it('shows spaces created by the current user in the open global overview', async () => {
  const api = vi.fn(async (input: string) => {
    const path = new URL(input, 'http://localhost').pathname;
    if (path === '/v1/pms/lists') {
      return { items: [], total: 0, page: 1, page_size: 100 };
    }
    if (path === '/v1/pms/spaces') return [];
    throw new Error(`Unexpected test API path: ${path}`);
  });
  renderPmsView('/', api as unknown as AppRuntime['api']);
  await screen.findByRole('main');
  const space: PmsSpace = {
    id: 'new-space',
    workspace_id: 'new-space',
    workspace_key: 'new-project',
    key: 'new-project',
    name: 'New project',
    description: '',
    member_count: 1,
    current_user_role: 'owner',
    created_at: '',
    updated_at: '',
  };

  act(() =>
    dispatchPmsSpaceChanged({
      type: 'created',
      space,
      creatorId: 'other-user',
    }),
  );
  expect(screen.queryByRole('link', { name: 'New project' })).toBeNull();
  act(() =>
    dispatchPmsSpaceChanged({
      type: 'created',
      space,
      creatorId: state.user.id,
    }),
  );
  expect(
    await screen.findByRole('link', { name: 'New project' }),
  ).toHaveProperty('pathname', '/spaces/new-space');
  expect(runtime.navigate).not.toHaveBeenCalled();
});
