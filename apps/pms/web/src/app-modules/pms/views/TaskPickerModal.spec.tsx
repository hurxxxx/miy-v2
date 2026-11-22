import type { ButtonHTMLAttributes, ReactNode } from 'react';
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
} from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactElement } from 'react';

import { PMS_WORKSPACE_KEY } from '../../../platform/runtime/app-location';
import { renderWithRuntime, TEST_USER } from '../../../test-utils/runtime';
import { TaskPickerModal } from './TaskPickerModal';

// miy: the frame runtime replaces AI-DO's token; the user comes from
// `/v1/pms/me` (no legacy workspace memberships).
const render = (ui: ReactElement) =>
  renderWithRuntime(ui, { user: { ...TEST_USER, id: 'actor' } });

const mocks = vi.hoisted(() => ({
  lists: vi.fn(),
  tasks: vi.fn(),
  spaces: vi.fn(),
  translate: (key: string) => key,
}));
vi.mock('../../../platform/app-runtime/use-app-target-spaces', () => ({
  useAppTargetSpaces: mocks.spaces,
}));
vi.mock('../api/pms-api', () => ({
  listPmsTaskLists: mocks.lists,
  listTaskListTasks: mocks.tasks,
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: mocks.translate, i18n: { language: 'en-US' } }),
}));
vi.mock('@miy/ui', () => ({
  Button: ({ children, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button {...props}>{children}</button>
  ),
  Dialog: ({ children, open }: { children: ReactNode; open: boolean }) =>
    open ? <div>{children}</div> : null,
  ContentState: ({ title }: { title: ReactNode }) => <div>{title}</div>,
}));

const spaces = ['pms-one', 'pms-two'].map((key) => ({
  key,
  name: key,
  kind: 'app_space',
  role: 'member',
  ownerAppId: 'pms',
}));
const task = (key: string) => ({
  id: `task-${key}`,
  title: `Task ${key}`,
  reference: key,
});

beforeEach(() => {
  mocks.spaces.mockReset().mockReturnValue({
    choices: spaces,
    legacy: false,
    loading: false,
    error: false,
  });
  mocks.lists.mockReset().mockImplementation(async (_api, _params, key) => ({
    items: [
      {
        id: `list-${key}`,
        key,
        name: `List ${key}`,
        role: 'member',
        workspace_key: key,
      },
    ],
  }));
  mocks.tasks
    .mockReset()
    .mockImplementation(async (_api, _list, _params, key) => ({
      items: [task(key)],
    }));
});
afterEach(cleanup);

function picker(workspaceSlug: string, fixedTaskListId?: string) {
  return (
    <TaskPickerModal
      isOpen
      onClose={vi.fn()}
      onPick={vi.fn()}
      workspaceSlug={workspaceSlug}
      fixedTaskListId={fixedTaskListId}
    />
  );
}

describe('PMS task picker app-space scope', () => {
  it('loads parent tasks in the explicitly selected PMS space with empty legacy memberships', async () => {
    render(picker('pms-one', 'list-pms-one'));
    await screen.findByRole('button', { name: /Task pms-one/ });
    expect(mocks.tasks).toHaveBeenCalledWith(
      expect.any(Function),
      'list-pms-one',
      expect.any(Object),
      'pms-one',
    );
    expect(mocks.lists).not.toHaveBeenCalled();
    expect(screen.queryByRole('combobox')).toBeNull();
  });

  it.each(['docs-one', 'recording-one', 'meeting-one'])(
    'requires an explicit authorized PMS destination from %s',
    async (source) => {
      const onPick = vi.fn();
      render(
        <TaskPickerModal
          isOpen
          onClose={vi.fn()}
          onPick={onPick}
          workspaceSlug={source}
        />,
      );
      expect(mocks.lists).not.toHaveBeenCalled();
      fireEvent.change(
        screen.getByRole('combobox', { name: 'searchSpace.label' }),
        { target: { value: 'pms-two' } },
      );
      fireEvent.click(
        await screen.findByRole('button', { name: /Task pms-two/ }),
      );
      await waitFor(() =>
        expect(onPick).toHaveBeenCalledWith(
          expect.objectContaining({ id: 'task-pms-two' }),
        ),
      );
      expect(mocks.lists).toHaveBeenCalledWith(
        expect.any(Function),
        undefined,
        'pms-two',
      );
      expect(mocks.tasks).toHaveBeenCalledWith(
        expect.any(Function),
        'list-pms-two',
        expect.any(Object),
        'pms-two',
      );
      expect(mocks.lists.mock.calls.every((call) => call[2] !== source)).toBe(
        true,
      );
    },
  );

  it('clears old tasks synchronously when the selected target space changes', async () => {
    let resolveOld!: (response: { items: ReturnType<typeof task>[] }) => void;
    mocks.tasks.mockImplementation((_api, _list, _params, key) =>
      key === 'pms-one'
        ? new Promise((resolve) => {
            resolveOld = resolve;
          })
        : Promise.resolve({ items: [task(key)] }),
    );
    render(picker('meeting-one'));
    const select = screen.getByRole('combobox', { name: 'searchSpace.label' });
    fireEvent.change(select, { target: { value: 'pms-one' } });
    await waitFor(() => expect(mocks.tasks).toHaveBeenCalledTimes(1));
    fireEvent.change(
      screen.getByRole('combobox', { name: 'searchSpace.label' }),
      { target: { value: 'pms-two' } },
    );
    await screen.findByRole('button', { name: /Task pms-two/ });
    await act(async () => resolveOld({ items: [task('pms-one')] }));
    expect(screen.queryByText('Task pms-one')).toBeNull();
    expect(screen.queryByRole('option', { name: /List pms-one/ })).toBeNull();
  });

  // miy: a fixed list (the parent picker) is read by its id and the server
  // checks access, so no owner-space choice is needed; AI-DO refused to load
  // a fixed list outside the caller's PMS spaces.
  it('loads a fixed list by id without an owner-space choice', async () => {
    mocks.spaces.mockReturnValue({ choices: [], loading: false, error: false });
    render(picker(PMS_WORKSPACE_KEY, 'list-pms-one'));
    await screen.findByRole('button', { name: new RegExp(`Task ${PMS_WORKSPACE_KEY}`) });
    expect(mocks.tasks).toHaveBeenCalledWith(
      expect.any(Function),
      'list-pms-one',
      expect.any(Object),
      PMS_WORKSPACE_KEY,
    );
    expect(mocks.lists).not.toHaveBeenCalled();
    expect(screen.queryByRole('combobox')).toBeNull();
  });

  it('keeps readable viewer spaces available for resource attachment', async () => {
    mocks.spaces.mockReturnValue({
      choices: [{ ...spaces[0], role: 'viewer' }],
      loading: false,
      error: false,
    });
    mocks.lists.mockResolvedValue({
      items: [{ id: 'list-pms-one', name: 'Read-only list', role: 'viewer' }],
    });
    render(picker('docs-one'));
    fireEvent.change(
      screen.getByRole('combobox', { name: 'searchSpace.label' }),
      { target: { value: 'pms-one' } },
    );
    await screen.findByRole('button', { name: /Task pms-one/ });
    expect(mocks.spaces).toHaveBeenCalledWith('pms', { includeViewers: true });
  });

  it('preserves a server denial after a previously authorized space is revoked', async () => {
    mocks.lists.mockRejectedValue(new Error('space access revoked'));
    render(picker('meeting-one'));
    fireEvent.change(
      screen.getByRole('combobox', { name: 'searchSpace.label' }),
      { target: { value: 'pms-two' } },
    );
    await screen.findByText('space access revoked');
    expect(mocks.tasks).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: /Task pms/ })).toBeNull();
  });
});
