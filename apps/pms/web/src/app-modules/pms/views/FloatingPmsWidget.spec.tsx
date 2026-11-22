import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n/i18n';
import { renderWithRuntime } from '../../../test-utils/runtime';
import type { PmsTaskList } from '../api/pms-api';
import { FloatingPmsWidget } from './FloatingPmsWidget';
import { dispatchPmsTaskListChanged } from './pms-events';

// miy: the widget is the side panel `/widgets/my-tasks` and renders inside the
// frame runtime (no token). A space's `slug` is its id there, so a space's
// lists are read with `team_id` (`listAllPmsTaskLists(api, slug, slug)`).
const render = renderWithRuntime;
const pmsApiMocks = vi.hoisted(() => ({
  listAllPmsTaskLists: vi.fn(),
  listPersonalPmsAssignedTasks: vi.fn(),
  listTaskListStatuses: vi.fn(),
}));
const newTaskModalSpy = vi.hoisted(() => vi.fn());
const feedbackMock = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
}));

vi.mock('@miy/ui', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@miy/ui')>();
  return { ...actual, useFeedback: () => feedbackMock };
});

vi.mock('../api/pms-api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/pms-api')>();
  return { ...actual, ...pmsApiMocks };
});

vi.mock('./NewTaskModal', () => ({
  NewTaskModal: (props: Record<string, unknown>) => {
    newTaskModalSpy(props);
    const options = props.workspaceOptions as Array<{
      label: string;
      slug: string;
    }>;
    const onWorkspaceSlugChange = props.onWorkspaceSlugChange as (
      workspaceSlug: string,
    ) => void;
    return (
      <div aria-label="new-task-modal" role="dialog">
        <select
          aria-label="new-task-workspace"
          onChange={(event) => onWorkspaceSlugChange(event.target.value)}
          value={props.workspaceSlug as string}
        >
          {options.map((option) => (
            <option key={option.slug} value={option.slug}>
              {option.label}
            </option>
          ))}
        </select>
      </div>
    );
  },
}));

describe('FloatingPmsWidget', () => {
  beforeEach(async () => {
    newTaskModalSpy.mockClear();
    pmsApiMocks.listAllPmsTaskLists.mockReset();
    pmsApiMocks.listPersonalPmsAssignedTasks.mockReset();
    pmsApiMocks.listTaskListStatuses.mockReset();
    pmsApiMocks.listTaskListStatuses.mockResolvedValue({ items: [] });
    await i18n.changeLanguage('ko-KR');
  });

  it('moves workspace selection into the new-task modal', async () => {
    pmsApiMocks.listPersonalPmsAssignedTasks.mockResolvedValue({
      items: [],
      page: 1,
      page_size: 100,
      total: 0,
      workspaces: [
        { id: 'workspace-alpha', name: 'Alpha', slug: 'alpha' },
        { id: 'workspace-beta', name: 'Beta', slug: 'beta' },
      ],
    });
    pmsApiMocks.listAllPmsTaskLists.mockImplementation(
      (_api: unknown, _teamId: string, workspaceSlug: string) =>
        Promise.resolve({
          items: [
            {
              folder_name: null,
              id: `list-${workspaceSlug}`,
              name: 'Backlog',
              role: 'member',
              team_id: `space-${workspaceSlug}`,
              team_name: workspaceSlug === 'alpha' ? 'Alpha' : 'Beta',
            },
          ],
        }),
    );

    render(<FloatingPmsWidget workspaceSlug="alpha" />);

    await waitFor(() => {
      expect(
        (
          screen.getByRole('button', {
            name: '새 태스크',
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false);
    });
    expect(screen.queryByRole('combobox')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: '새 태스크' }));
    const workspaceSelect = await screen.findByRole('combobox', {
      name: 'new-task-workspace',
    });
    fireEvent.change(workspaceSelect, { target: { value: 'beta' } });

    expect(
      screen.getByRole('dialog', { name: 'new-task-modal' }),
    ).not.toBeNull();
    await waitFor(() => {
      expect(pmsApiMocks.listAllPmsTaskLists).toHaveBeenCalledWith(
        expect.any(Function),
        'beta',
        'beta',
      );
      expect(newTaskModalSpy).toHaveBeenLastCalledWith(
        expect.objectContaining({
          taskListId: 'list-beta',
          workspaceSlug: 'beta',
        }),
      );
    });
  });

  it('reloads and removes tasks when their list is archived', async () => {
    const assignedTask = {
      due_date: null,
      id: 'task-1',
      list_id: 'list-alpha',
      priority: 'medium',
      status: 'todo',
      title: 'Assigned archive task',
      workspace: { id: 'workspace-alpha', name: 'Alpha', slug: 'alpha' },
    };
    pmsApiMocks.listPersonalPmsAssignedTasks
      .mockResolvedValueOnce({
        items: [assignedTask],
        page: 1,
        page_size: 100,
        total: 1,
        workspaces: [assignedTask.workspace],
      })
      .mockResolvedValue({
        items: [],
        page: 1,
        page_size: 100,
        total: 0,
        workspaces: [assignedTask.workspace],
      });
    pmsApiMocks.listAllPmsTaskLists.mockResolvedValue({
      items: [
        {
          archived: false,
          id: 'list-alpha',
          name: 'Backlog',
          role: 'member',
          team_id: 'space-alpha',
          team_name: 'Alpha',
        },
      ],
    });

    render(<FloatingPmsWidget workspaceSlug="alpha" />);

    expect(await screen.findByText('Assigned archive task')).not.toBeNull();

    act(() => {
      dispatchPmsTaskListChanged({
        type: 'updated',
        taskList: {
          archived: true,
          id: 'list-alpha',
          name: 'Backlog',
          role: 'member',
          team_id: 'space-alpha',
          team_name: 'Alpha',
        } as PmsTaskList,
      });
    });

    await waitFor(() => {
      expect(screen.queryByText('Assigned archive task')).toBeNull();
      expect(pmsApiMocks.listPersonalPmsAssignedTasks).toHaveBeenCalledTimes(2);
    });
  });

  it('requires an explicit space when several PMS spaces exist and the current app owns neither', async () => {
    pmsApiMocks.listPersonalPmsAssignedTasks.mockResolvedValue({
      items: [],
      total: 0,
      workspaces: [
        { id: 'a', name: 'Alpha', slug: 'pms-alpha' },
        { id: 'b', name: 'Beta', slug: 'pms-beta' },
      ],
    });
    render(<FloatingPmsWidget workspaceSlug="docs-other" />);
    await waitFor(() =>
      expect(
        (screen.getByRole('button', { name: '새 태스크' }) as HTMLButtonElement)
          .disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole('button', { name: '새 태스크' }));
    await screen.findByRole('dialog', { name: 'new-task-modal' });
    expect(newTaskModalSpy).toHaveBeenLastCalledWith(
      expect.objectContaining({ workspaceSlug: '', canCreate: false }),
    );
    expect(pmsApiMocks.listAllPmsTaskLists).not.toHaveBeenCalled();
  });
});
