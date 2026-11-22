import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import type { ComponentProps } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  PersonalWidgetHost,
  PersonalWidgetPanelRoute,
} from './PersonalWidgetHost';
import {
  getPersonalMemo,
  listPersonalTodos,
  updatePersonalTodo,
  type PersonalTodoItem,
} from './personal-widgets-api';
import { PERSONAL_WIDGET_STORAGE_KEY } from './personal-widget-state';

function translate(key: string, values?: { title?: string }) {
  return values?.title ? `${key}:${values.title}` : key;
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: translate,
  }),
}));

// miy: the app's API client (runtime.api) replaces AI-DO's auth token.
const api = vi.hoisted(() => async () => undefined);
vi.mock('../runtime/runtime-context', () => ({
  useAppApi: () => api,
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
const getPersonalMemoMock = vi.mocked(getPersonalMemo);
const updatePersonalTodoMock = vi.mocked(updatePersonalTodo);

function todo(title: string): PersonalTodoItem {
  return {
    completed: false,
    completedAt: null,
    createdAt: '2026-07-19T00:00:00Z',
    id: 'todo-1',
    sortOrder: 1000,
    title,
    updatedAt: '2026-07-19T00:00:00Z',
  };
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
      expect(updatePersonalTodoMock).toHaveBeenCalledWith(api, 'todo-1', {
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
});

describe('PersonalWidgetPanelRoute (miy panel routes)', () => {
  it('renders the memo panel on its own without the dock', async () => {
    getPersonalMemoMock.mockResolvedValue({
      body: 'Saved memo',
      createdAt: '2026-07-19T00:00:00',
      id: 'memo-1',
      updatedAt: '2026-07-19T00:00:00',
    });
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

  it('renders the to-do panel and adds a to-do', async () => {
    render(<PersonalWidgetPanelRoute widget="todo" />);

    expect(await screen.findByText('Original todo')).toBeTruthy();
    expect(
      screen.getByRole('heading', { name: 'personalWidgets.todo.title' }),
    ).toBeTruthy();
  });
});
