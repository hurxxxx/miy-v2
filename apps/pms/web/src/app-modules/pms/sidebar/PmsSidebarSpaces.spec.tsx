import {
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppRuntime } from '@miy/sdk';

import {
  createTestRuntime,
  renderWithRuntime,
  TEST_USER,
} from '../../../test-utils/runtime';
import { PMS_WORKSPACE_KEY } from '../../../platform/runtime/app-location';
import * as pmsApi from '../api/pms-api';
import type { PmsSpace, PmsTaskList } from '../api/pms-api';
import { PmsSidebarSpaces } from './PmsSidebarSpaces';

// miy: PMS runs without AI-DO's app-first runtime (`useOptionalAppRuntime()`
// is null), on the workspace-route shape with app-relative paths. The cases
// that only exercised the app-first runtime (canonical `/pms/s/<key>` links,
// `can_create_space`, runtime space switches) and the excluded Docs feature
// are not ported; see docs/PORTING.md.
const state = vi.hoisted(() => ({
  prompt: vi.fn(),
  confirm: vi.fn(),
  feedback: { error: vi.fn() },
}));

vi.mock('@miy/ui', async (original) => ({
  ...(await original<typeof import('@miy/ui')>()),
  useFeedback: () => state.feedback,
  usePrompt: () => ({ prompt: state.prompt, promptDialog: null }),
  useConfirm: () => ({ confirm: state.confirm, confirmDialog: null }),
}));
vi.mock('../api/pms-api', async (original) => ({
  ...(await original<typeof import('../api/pms-api')>()),
  listAllPmsTaskLists: vi.fn(),
  listFolders: vi.fn(),
  listSpaces: vi.fn(),
  listSpacePreferences: vi.fn(),
  updateSpacePreference: vi.fn(),
  updatePmsTaskList: vi.fn(),
  updateSpace: vi.fn(),
  updateFolder: vi.fn(),
  deletePmsTaskList: vi.fn(),
  deleteSpace: vi.fn(),
  deleteFolder: vi.fn(),
}));
vi.mock('../views/CreateTaskListModal', () => ({
  CreateTaskListModal: ({
    isOpen,
    workspaceSlug,
    teamId,
  }: {
    isOpen: boolean;
    workspaceSlug: string;
    teamId: string;
  }) =>
    isOpen ? (
      <div data-testid="create-list">
        {workspaceSlug}:{teamId}
      </div>
    ) : null,
}));
vi.mock('../views/CreateFolderModal', () => ({
  CreateFolderModal: ({
    isOpen,
    workspaceSlug,
    teamId,
  }: {
    isOpen: boolean;
    workspaceSlug: string;
    teamId: string;
  }) =>
    isOpen ? (
      <div data-testid="create-folder">
        {workspaceSlug}:{teamId}
      </div>
    ) : null,
}));
vi.mock('../views/CreateSpaceModal', () => ({ CreateSpaceModal: () => null }));
vi.mock('../views/SpaceMembersModal', () => ({
  SpaceMembersModal: ({
    isOpen,
    workspaceSlug,
    spaceId,
  }: {
    isOpen: boolean;
    workspaceSlug: string;
    spaceId: string;
  }) =>
    isOpen ? (
      <div data-testid="members">
        {workspaceSlug}:{spaceId}
      </div>
    ) : null,
}));
vi.mock('./SpaceOrderEditorModal', () => ({
  SpaceOrderEditorModal: () => null,
}));

function space(name: string, role = 'owner'): PmsSpace {
  return {
    id: `id-${name}`,
    key: `pms-${name}`,
    workspace_id: `id-${name}`,
    workspace_key: `pms-${name}`,
    name: `Space ${name}`,
    description: '',
    member_count: 1,
    current_user_role: role,
    created_at: '',
    updated_at: '',
  };
}

function list(name: string, role = 'admin'): PmsTaskList {
  return {
    id: `list-${name}`,
    key: name,
    name: `List ${name}`,
    description: '',
    team_id: `id-${name}`,
    team_name: `Space ${name}`,
    workspace_id: `id-${name}`,
    workspace_key: `pms-${name}`,
    folder_id: null,
    folder_name: null,
    sort_order: 1,
    task_count: 0,
    active_task_count: 0,
    open_task_count: 0,
    overdue_task_count: 0,
    progress: 0,
    updated_at: '',
    created_at: '',
    status_mode: 'inherit',
    status: 'active',
    role,
    archived: false,
    member_count: 1,
    milestone_count: 0,
  };
}

let runtime: AppRuntime;

function sidebar(onNavigate = vi.fn()) {
  return (
    <PmsSidebarSpaces
      activeNavItemId=""
      currentWorkspaceSlug={PMS_WORKSPACE_KEY}
      isExpanded
      onToggle={vi.fn()}
      onNavigate={onNavigate}
    />
  );
}

function renderSidebar(onNavigate = vi.fn()) {
  return renderWithRuntime(sidebar(onNavigate), {
    runtime,
    user: { ...TEST_USER, id: 'actor' },
  });
}

function navigatedTo(): string | undefined {
  return vi.mocked(runtime.navigate).mock.calls.at(-1)?.[0];
}

function spaceTree(name: string) {
  return within(
    screen
      .getByRole('button', { name: `Space ${name}` })
      .closest('.space-y-0\\.5') as HTMLElement,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  runtime = createTestRuntime({ path: '/' });
  state.confirm.mockResolvedValue(true);
  vi.mocked(pmsApi.listSpaces).mockResolvedValue([
    space('a'),
    space('b', 'viewer'),
  ]);
  vi.mocked(pmsApi.listFolders).mockResolvedValue({ items: [] });
  vi.mocked(pmsApi.listAllPmsTaskLists).mockImplementation(
    async (_api, _team, _key, options) => ({
      items: options?.archived ? [] : [list('a'), list('b', 'viewer')],
      total: options?.archived ? 0 : 2,
      page: 1,
      page_size: 100,
    }),
  );
  vi.mocked(pmsApi.listSpacePreferences).mockResolvedValue({ items: [] });
});

afterEach(cleanup);

describe('PMS sidebar spaces', () => {
  it('loads both authorized spaces and navigates each row by its id', async () => {
    const onNavigate = vi.fn();
    renderSidebar(onNavigate);
    await screen.findByText('List b');
    expect(pmsApi.listSpaces).toHaveBeenCalledWith(
      runtime.api,
      PMS_WORKSPACE_KEY,
    );
    fireEvent.click(
      screen.getByText('List b').closest('button') as HTMLButtonElement,
    );
    expect(navigatedTo()).toBe('/lists/list-b');
    fireEvent.click(screen.getByRole('button', { name: 'Space b' }));
    expect(navigatedTo()).toBe('/spaces/id-b');
    expect(onNavigate).toHaveBeenCalledTimes(2);
  });

  it('keeps viewer trees readable and creation/management scoped to each returned role', async () => {
    renderSidebar();
    await screen.findByText('List b');
    expect(spaceTree('a').getByRole('button', { name: '추가' })).toBeTruthy();
    expect(spaceTree('b').queryByRole('button', { name: '추가' })).toBeNull();
    expect(
      spaceTree('b').queryByRole('button', { name: '리스트 관리' }),
    ).toBeNull();
    fireEvent.click(spaceTree('a').getByRole('button', { name: '추가' }));
    fireEvent.click(
      screen.getByText('리스트').closest('button') as HTMLButtonElement,
    );
    expect(screen.getByTestId('create-list').textContent).toBe('pms:id-a');
  });

  it('offers space creation to every PMS user; the server decides', async () => {
    renderSidebar();
    await screen.findByRole('button', { name: 'Space a' });
    expect(
      screen.getByRole('button', { name: '새 워크스페이스' }),
    ).toBeTruthy();
  });

  it('archives a list in its own space from the tree', async () => {
    vi.mocked(pmsApi.updatePmsTaskList).mockResolvedValue({
      ...list('a'),
      archived: true,
    });
    renderSidebar();
    await screen.findByText('List a');
    fireEvent.click(
      spaceTree('a').getByRole('button', { name: '리스트 관리' }),
    );
    fireEvent.click(screen.getByRole('menuitem', { name: '보관' }));
    await waitFor(() =>
      expect(pmsApi.updatePmsTaskList).toHaveBeenCalledWith(
        runtime.api,
        'list-a',
        { archived: true },
        PMS_WORKSPACE_KEY,
      ),
    );
  });

  it('passes the selected row to folder creation and member management', async () => {
    renderSidebar();
    await screen.findByText('List a');
    fireEvent.click(spaceTree('a').getByRole('button', { name: '추가' }));
    fireEvent.click(
      screen.getByText('폴더').closest('button') as HTMLButtonElement,
    );
    expect(screen.getByTestId('create-folder').textContent).toBe('pms:id-a');
    fireEvent.click(spaceTree('b').getByRole('button', { name: '더 보기' }));
    fireEvent.click(screen.getByRole('button', { name: '멤버 보기' }));
    expect(screen.getByTestId('members').textContent).toBe('pms:id-b');
  });
});
