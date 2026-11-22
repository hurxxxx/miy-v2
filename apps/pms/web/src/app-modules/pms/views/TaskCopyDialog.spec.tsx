import { fireEvent, screen, waitFor } from '@testing-library/react';
import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithRuntime, TEST_USER } from '../../../test-utils/runtime';
import {
  copyTasksToList,
  listAllPmsTaskLists,
  type PmsTaskList,
  type TaskCopyResult,
} from '../api/pms-api';
import { TaskCopyDialog } from './TaskCopyDialog';

// react-i18next hands out a stable `t`; a fresh one per render would re-run the load effect.
const { feedbackError, targetSpaces, translate } = vi.hoisted(() => ({
  feedbackError: vi.fn(),
  targetSpaces: vi.fn(),
  translate: (key: string, values?: Record<string, unknown>) =>
    values ? `${key}:${JSON.stringify(values)}` : key,
}));

vi.mock('../../../platform/app-runtime/use-app-target-spaces', () => ({
  useAppTargetSpaces: targetSpaces,
}));

vi.mock('@miy/ui', () => ({
  Button: ({ children, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props}>{children}</button>
  ),
  ContentState: ({ title }: { title: ReactNode }) => <p>{title}</p>,
  ContextNote: ({ children }: { children: ReactNode }) => <p>{children}</p>,
  Dialog: ({
    children,
    description,
    open,
    title,
  }: {
    children: ReactNode;
    description?: ReactNode;
    open: boolean;
    title: ReactNode;
  }) =>
    open ? (
      <section role="dialog">
        <h2>{title}</h2>
        <p>{description}</p>
        {children}
      </section>
    ) : null,
  useFeedback: () => ({ error: feedbackError }),
}));

vi.mock('../api/pms-api', () => ({
  copyTasksToList: vi.fn(),
  listAllPmsTaskLists: vi.fn(),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: translate }),
}));

function taskList(overrides: Partial<PmsTaskList>): PmsTaskList {
  return {
    archived: false,
    key: 'KEY',
    name: 'List',
    role: 'member',
    team_name: 'Space',
    ...overrides,
  } as PmsTaskList;
}

const copyResult: TaskCopyResult = {
  target_list_id: 'target',
  copied_count: 3,
  copied_task_ids: ['c1', 'c2', 'c3'],
  unmatched: { assignees: 1, labels: 0, milestones: 0, custom_fields: 0 },
};

function renderDialog() {
  const onClose = vi.fn();
  const onCopied = vi.fn();
  // miy: the frame runtime replaces the token; the user comes from `/v1/pms/me`.
  renderWithRuntime(
    <TaskCopyDialog
      workspaceSlug="pms-space-a"
      isOpen
      onClose={onClose}
      onCopied={onCopied}
      sourceTaskListId="source"
      taskIds={['t1', 't2']}
    />,
    { user: { ...TEST_USER, id: 'me' } },
  );
  return { onClose, onCopied };
}

describe('TaskCopyDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    targetSpaces.mockReturnValue({
      choices: [
        {
          key: 'pms-catalog',
          kind: 'app_global',
          ownerAppId: 'pms',
          role: 'user',
        },
      ],
      loading: false,
      error: false,
    });
    vi.mocked(listAllPmsTaskLists).mockResolvedValue({
      items: [
        taskList({ id: 'source', key: 'SRC', name: 'Source list' }),
        taskList({
          id: 'target',
          key: 'TGT',
          name: 'Target list',
          team_name: 'Other space',
        }),
        taskList({ id: 'viewer', name: 'Read only list', role: 'viewer' }),
        taskList({ id: 'archived', name: 'Old list', archived: true }),
      ],
      total: 4,
      page: 1,
      page_size: 100,
    });
  });

  it('offers only lists the user can edit and marks the current one', async () => {
    renderDialog();

    expect(await screen.findByText('Target list')).toBeTruthy();
    expect(screen.getByText('Source list')).toBeTruthy();
    expect(screen.getByText('pms.bulk.copy.currentList')).toBeTruthy();
    expect(screen.getByText('Other space · TGT')).toBeTruthy();
    expect(screen.queryByText('Read only list')).toBeNull();
    expect(screen.queryByText('Old list')).toBeNull();
    expect(listAllPmsTaskLists).toHaveBeenCalledWith(
      expect.any(Function),
      undefined,
      'pms-catalog',
    );
  });

  it('includes another space from the authorized global catalog while submitting in the source space', async () => {
    vi.mocked(listAllPmsTaskLists).mockImplementation(
      async (_api, _params, key) => ({
        items:
          key === 'pms-catalog'
            ? [
                taskList({
                  id: 'target',
                  name: 'Foreign-space destination',
                  workspace_key: 'pms-space-b',
                }),
              ]
            : [
                taskList({
                  id: 'source',
                  name: 'Source only',
                  workspace_key: 'pms-space-a',
                }),
              ],
        total: 1,
        page: 1,
        page_size: 100,
      }),
    );
    vi.mocked(copyTasksToList).mockResolvedValue(copyResult);
    const { onClose } = renderDialog();
    fireEvent.click(await screen.findByText('Foreign-space destination'));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(copyTasksToList).toHaveBeenCalledWith(
      expect.any(Function),
      'source',
      expect.objectContaining({ target_list_id: 'target' }),
      'pms-space-a',
    );
  });

  it('does not substitute the current space when target app access is absent', () => {
    targetSpaces.mockReturnValue({ choices: [], loading: false, error: false });
    renderDialog();
    expect(listAllPmsTaskLists).not.toHaveBeenCalled();
    expect(copyTasksToList).not.toHaveBeenCalled();
  });

  it('copies the selected tasks into the picked list with the chosen progress option', async () => {
    vi.mocked(copyTasksToList).mockResolvedValue(copyResult);
    const { onClose, onCopied } = renderDialog();

    fireEvent.click(screen.getByLabelText(/pms\.bulk\.copy\.resetProgress/));
    fireEvent.click(await screen.findByText('Target list'));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(copyTasksToList).toHaveBeenCalledWith(
      expect.any(Function),
      'source',
      {
        task_ids: ['t1', 't2'],
        target_list_id: 'target',
        reset_progress: true,
      },
      'pms-space-a',
    );
    expect(onCopied).toHaveBeenCalledWith(
      copyResult,
      expect.objectContaining({ id: 'target', name: 'Target list' }),
    );
  });

  it('keeps the dialog open and reports the error when the copy fails', async () => {
    vi.mocked(copyTasksToList).mockRejectedValue(new Error('copy refused'));
    const { onClose, onCopied } = renderDialog();

    fireEvent.click(await screen.findByText('Target list'));

    await waitFor(() =>
      expect(feedbackError).toHaveBeenCalledWith('copy refused'),
    );
    expect(onClose).not.toHaveBeenCalled();
    expect(onCopied).not.toHaveBeenCalled();
  });

  it('filters the lists by the search text', async () => {
    renderDialog();
    await screen.findByText('Target list');

    fireEvent.change(
      screen.getByPlaceholderText('pms.bulk.copy.searchPlaceholder'),
      {
        target: { value: 'other' },
      },
    );

    expect(screen.getByText('Target list')).toBeTruthy();
    expect(screen.queryByText('Source list')).toBeNull();
  });
});
