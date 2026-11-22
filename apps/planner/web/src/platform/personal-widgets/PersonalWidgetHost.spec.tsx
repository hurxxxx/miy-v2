import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiRequestError } from '../api/client';
import {
  PersonalWidgetHost,
  PersonalWidgetPanelRoute,
} from './PersonalWidgetHost';
import {
  createPersonalTodo,
  deletePersonalTodo,
  getPersonalMemo,
  listPersonalTodos,
  savePersonalMemo,
  updatePersonalTodo,
  type PersonalMemo,
  type PersonalTodoItem,
} from './personal-widgets-api';
import { PERSONAL_WIDGET_STORAGE_KEY } from './personal-widget-state';

function translate(key: string, values?: { title?: string }) {
  return values?.title ? `${key}:${values.title}` : key;
}

vi.mock('react-i18next', async (original) => ({
  ...await original<typeof import('react-i18next')>(),
  useTranslation: () => ({
    t: translate,
  }),
}));

// miy: the app's API client (runtime.api) replaces AI-DO's auth token.
const api = vi.hoisted(() => ({ current: async () => undefined }));
vi.mock('../runtime/runtime-context', () => ({
  useAppApi: () => api.current,
}));

vi.mock('./personal-widgets-api', () => ({
  createPersonalTodo: vi.fn(),
  deletePersonalTodo: vi.fn(),
  getPersonalMemo: vi.fn(),
  listPersonalTodos: vi.fn(),
  savePersonalMemo: vi.fn(),
  updatePersonalTodo: vi.fn(),
}));

const listPersonalTodosMock = vi.mocked(listPersonalTodos);
const createPersonalTodoMock = vi.mocked(createPersonalTodo);
const deletePersonalTodoMock = vi.mocked(deletePersonalTodo);
const getPersonalMemoMock = vi.mocked(getPersonalMemo);
const savePersonalMemoMock = vi.mocked(savePersonalMemo);
const updatePersonalTodoMock = vi.mocked(updatePersonalTodo);

function todo(title: string, id = 'todo-1'): PersonalTodoItem {
  return {
    completed: false,
    completedAt: null,
    createdAt: '2026-07-19T00:00:00Z',
    id,
    sortOrder: 1000,
    title,
    updatedAt: '2026-07-19T00:00:00Z',
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => { resolve = fulfill; });
  return { promise, resolve };
}

function memo(body: string, updatedAt = '2026-07-19T00:00:00'): PersonalMemo {
  return { body, id: 'memo-1', createdAt: '2026-07-19T00:00:00', updatedAt };
}

function renderTodoWidget(
  props: ComponentProps<typeof PersonalWidgetHost> = {},
) {
  return render(<PersonalWidgetHost {...props} />);
}

beforeEach(() => {
  window.localStorage.setItem(
    PERSONAL_WIDGET_STORAGE_KEY,
    JSON.stringify({ activeWidget: 'todo', mode: 'panel' }),
  );
  listPersonalTodosMock.mockResolvedValue({ items: [todo('Original todo')] });
  getPersonalMemoMock.mockResolvedValue({
    body: '',
    createdAt: null,
    id: null,
    updatedAt: null,
  });
  updatePersonalTodoMock.mockResolvedValue(todo('Updated todo'));
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  window.localStorage.clear();
});

describe('PersonalWidgetHost todo editing', () => {
  // miy: the PMS conversion action is not part of this release.
  it('describes the edit and delete actions on hover', async () => {
    renderTodoWidget();

    const editButton = await screen.findByRole('button', {
      name: 'personalWidgets.todo.edit:Original todo',
    });
    const deleteButton = screen.getByRole('button', {
      name: 'personalWidgets.todo.delete:Original todo',
    });

    expect(editButton.getAttribute('title')).toBe(
      'personalWidgets.todo.edit:Original todo',
    );
    expect(deleteButton.getAttribute('title')).toBe(
      'personalWidgets.todo.delete:Original todo',
    );
    expect(
      screen.queryByRole('button', {
        name: 'personalWidgets.todo.convertToPms:Original todo',
      }),
    ).toBeNull();
  });

  it('edits a todo title inline and applies the server response', async () => {
    renderTodoWidget();

    fireEvent.click(
      await screen.findByRole('button', {
        name: 'personalWidgets.todo.edit:Original todo',
      }),
    );
    const input = screen.getByRole('textbox', {
      name: 'personalWidgets.todo.editInputLabel:Original todo',
    });

    expect(input.getAttribute('maxlength')).toBe('240');
    expect(document.activeElement).toBe(input);
    fireEvent.change(input, { target: { value: '  Updated todo  ' } });
    fireEvent.click(
      screen.getByRole('button', {
        name: 'personalWidgets.todo.saveEdit',
      }),
    );

    await waitFor(() =>
      expect(updatePersonalTodoMock).toHaveBeenCalledWith(api.current, 'todo-1', {
        title: 'Updated todo',
      }),
    );
    expect(await screen.findByText('Updated todo')).toBeTruthy();
  });

  it('cancels editing with Escape without collapsing the widget', async () => {
    renderTodoWidget();

    fireEvent.click(
      await screen.findByRole('button', {
        name: 'personalWidgets.todo.edit:Original todo',
      }),
    );
    const input = screen.getByRole('textbox', {
      name: 'personalWidgets.todo.editInputLabel:Original todo',
    });
    fireEvent.change(input, { target: { value: 'Discard this change' } });
    fireEvent.keyDown(input, { key: 'Escape' });

    expect(updatePersonalTodoMock).not.toHaveBeenCalled();
    expect(screen.getByText('Original todo')).toBeTruthy();
    expect(document.activeElement).toBe(
      screen.getByRole('button', {
        name: 'personalWidgets.todo.edit:Original todo',
      }),
    );
    expect(screen.getByLabelText('personalWidgets.panelLabel')).toBeTruthy();
  });

  it('blocks blank titles and skips an unchanged trimmed title', async () => {
    renderTodoWidget();

    fireEvent.click(
      await screen.findByRole('button', {
        name: 'personalWidgets.todo.edit:Original todo',
      }),
    );
    const input = screen.getByRole('textbox', {
      name: 'personalWidgets.todo.editInputLabel:Original todo',
    });
    const saveButton = screen.getByRole('button', {
      name: 'personalWidgets.todo.saveEdit',
    });

    fireEvent.change(input, { target: { value: '   ' } });
    expect(saveButton.hasAttribute('disabled')).toBe(true);

    fireEvent.change(input, { target: { value: '  Original todo  ' } });
    fireEvent.click(saveButton);

    expect(updatePersonalTodoMock).not.toHaveBeenCalled();
    expect(screen.getByText('Original todo')).toBeTruthy();
  });

  it('keeps the edited title available for retry after an update failure', async () => {
    updatePersonalTodoMock.mockRejectedValueOnce(new Error('update failed'));
    renderTodoWidget();

    fireEvent.click(
      await screen.findByRole('button', {
        name: 'personalWidgets.todo.edit:Original todo',
      }),
    );
    const input = screen.getByRole('textbox', {
      name: 'personalWidgets.todo.editInputLabel:Original todo',
    });
    fireEvent.change(input, { target: { value: 'Retry title' } });
    fireEvent.click(
      screen.getByRole('button', {
        name: 'personalWidgets.todo.saveEdit',
      }),
    );

    expect(
      await screen.findByText('personalWidgets.errors.updateFailed'),
    ).toBeTruthy();
    expect(screen.getByRole('alert')).toBeTruthy();
    expect(screen.getByDisplayValue('Retry title')).toBeTruthy();

    updatePersonalTodoMock.mockResolvedValueOnce(todo('Retry title'));
    fireEvent.click(
      screen.getByRole('button', {
        name: 'personalWidgets.todo.saveEdit',
      }),
    );
    expect(await screen.findByText('Retry title')).toBeTruthy();
  });

  it('keeps the create input locked until its own response arrives', async () => {
    const created = deferred<PersonalTodoItem>();
    createPersonalTodoMock.mockReturnValueOnce(created.promise);
    updatePersonalTodoMock.mockResolvedValueOnce({ ...todo('Original todo'), completed: true });
    renderTodoWidget();

    const toggle = await screen.findByRole('button', {
      name: 'personalWidgets.todo.toggle:Original todo',
    });
    const input = screen.getByRole<HTMLInputElement>('textbox', {
      name: 'personalWidgets.todo.inputLabel',
    });
    fireEvent.change(input, { target: { value: 'New todo' } });
    fireEvent.click(screen.getByRole('button', { name: 'personalWidgets.todo.add' }));
    expect(input.disabled).toBe(true);

    await act(async () => fireEvent.click(toggle));
    expect(updatePersonalTodoMock).toHaveBeenCalledExactlyOnceWith(api.current, 'todo-1', {
      completed: true,
    });
    expect(input.disabled).toBe(true);
    expect(input.value).toBe('New todo');

    await act(async () => created.resolve(todo('New todo', 'todo-2')));
    expect(input.disabled).toBe(false);
    expect(input.value).toBe('');
    expect(screen.getByText('New todo')).toBeTruthy();
  });

  it('ignores a stale reload that finishes after a successful delete', async () => {
    const stale = deferred<{ items: PersonalTodoItem[] }>();
    listPersonalTodosMock.mockResolvedValueOnce({ items: [todo('Original todo')] })
      .mockReturnValueOnce(stale.promise);
    deletePersonalTodoMock.mockResolvedValueOnce(undefined);
    renderTodoWidget();

    const remove = await screen.findByRole('button', {
      name: 'personalWidgets.todo.delete:Original todo',
    });
    const reload = screen.getByRole('button', { name: 'personalWidgets.reload' });
    fireEvent.click(reload);
    expect(listPersonalTodosMock).toHaveBeenCalledTimes(2);

    await act(async () => fireEvent.click(remove));
    expect(screen.queryByText('Original todo')).toBeNull();
    await act(async () => stale.resolve({ items: [todo('Original todo')] }));
    expect(screen.queryByText('Original todo')).toBeNull();
  });
});

describe('PersonalWidgetPanelRoute (miy panel routes)', () => {
  it('renders the memo panel on its own without the dock', async () => {
    getPersonalMemoMock.mockResolvedValue(memo('Saved memo'));
    render(<PersonalWidgetPanelRoute widget="memo" />);

    expect(await screen.findByDisplayValue('Saved memo')).toBeTruthy();
    expect(
      screen.getByRole('heading', { name: 'personalWidgets.memo.title' }),
    ).toBeTruthy();
    expect(screen.queryByLabelText('personalWidgets.dockLabel')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'personalWidgets.close' }),
    ).toBeNull();
  });

  it('preserves a draft when refreshed or when display changes replace the runtime API', async () => {
    const saved = memo('Saved memo');
    getPersonalMemoMock.mockResolvedValue(saved);
    savePersonalMemoMock.mockResolvedValue(memo('Draft memo', '2026-07-19T00:00:01'));
    const view = render(<PersonalWidgetPanelRoute widget="memo" />);
    const input = await screen.findByDisplayValue(saved.body);
    fireEvent.change(input, { target: { value: 'Draft memo' } });

    api.current = vi.fn(async () => undefined);
    view.rerender(<PersonalWidgetPanelRoute widget="memo" />);
    expect(getPersonalMemoMock).toHaveBeenCalledTimes(1);
    const reload = screen.getByRole('button', { name: 'personalWidgets.reload' });
    fireEvent.click(reload);
    await waitFor(() => expect((reload as HTMLButtonElement).disabled).toBe(false));
    expect(getPersonalMemoMock).toHaveBeenCalledTimes(2);
    expect(screen.getByDisplayValue('Draft memo')).toBe(input);
    await waitFor(() => expect(savePersonalMemoMock).toHaveBeenCalledWith(api.current, 'Draft memo', saved.updatedAt), { timeout: 2500 });
  });

  it('keeps a conflicting draft and only replaces the saved copy on an explicit retry', async () => {
    const saved = memo('Original memo');
    const latest = memo('Saved in another view', '2026-07-19T00:00:01');
    getPersonalMemoMock.mockResolvedValueOnce(saved).mockResolvedValue(latest);
    savePersonalMemoMock
      .mockRejectedValueOnce(new ApiRequestError(409, 'Conflict', { code: 'personal_widgets_memo_conflict' }))
      .mockResolvedValue(memo('Draft memo', '2026-07-19T00:00:02'));
    render(<PersonalWidgetPanelRoute widget="memo" />);
    fireEvent.change(await screen.findByDisplayValue(saved.body), { target: { value: 'Draft memo' } });

    await screen.findByText(latest.body, {}, { timeout: 2500 });
    expect(screen.getByDisplayValue('Draft memo')).toBeTruthy();
    expect(screen.getByRole('alert').textContent).toBe('personalWidgets.memo.conflict');
    expect(savePersonalMemoMock).toHaveBeenCalledExactlyOnceWith(api.current, 'Draft memo', saved.updatedAt);
    fireEvent.click(screen.getByRole('button', { name: 'personalWidgets.memo.replaceSavedCopy' }));
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
    expect(savePersonalMemoMock).toHaveBeenLastCalledWith(api.current, 'Draft memo', latest.updatedAt);
  });

  it('renders the to-do panel and adds a to-do', async () => {
    render(<PersonalWidgetPanelRoute widget="todo" />);

    expect(await screen.findByText('Original todo')).toBeTruthy();
    expect(
      screen.getByRole('heading', { name: 'personalWidgets.todo.title' }),
    ).toBeTruthy();
  });
});
