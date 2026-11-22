import { DndContext } from '@dnd-kit/core';
import { SortableContext } from '@dnd-kit/sortable';
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import {
  createTestRuntime,
  renderWithRuntime,
} from '../../../test-utils/runtime';
import type { PmsTaskList } from '../api/pms-api';
import { SortableListLink } from './SortableListLink';

function taskList(overrides: Partial<PmsTaskList> = {}): PmsTaskList {
  return {
    archived: false,
    created_at: '2026-06-17T00:00:00.000Z',
    description: '',
    folder_id: null,
    folder_name: null,
    id: 'list-1',
    key: 'LIST',
    member_count: 1,
    milestone_count: 0,
    name: 'Launch List',
    open_task_count: 2,
    overdue_task_count: 0,
    progress: 0,
    role: 'admin',
    sort_order: 0,
    status: 'active',
    status_mode: 'custom',
    task_count: 3,
    active_task_count: 3,
    team_id: 'space-1',
    team_name: 'Space 1',
    workspace_id: 'space-1',
    workspace_key: 'administrator',
    updated_at: '2026-06-17T00:00:00.000Z',
    ...overrides,
  };
}

function renderLink({
  list = taskList(),
  canDrag = false,
  menuOpen = false,
  onDelete = vi.fn(),
  onArchive,
  onRename = vi.fn(),
  onRestore,
  onSettings = vi.fn(),
  onNavigate = vi.fn(),
}: {
  list?: PmsTaskList;
  canDrag?: boolean;
  menuOpen?: boolean;
  onDelete?: () => void;
  onArchive?: () => void;
  onRename?: () => void;
  onRestore?: () => void;
  onSettings?: () => void;
  onNavigate?: () => void;
} = {}) {
  const menu = {
    buttonRefs: { current: new Map<string, HTMLButtonElement>() },
    isOpen: menuOpen,
    onClose: vi.fn(),
    onToggle: vi.fn(),
  };

  const runtime = createTestRuntime();
  renderWithRuntime(
    <>
      <DndContext>
        <SortableContext items={[list.id]}>
          <SortableListLink
            activeNavItemId=""
            canDrag={canDrag}
            dropZone={null}
            list={list}
            menu={menu}
            onDelete={onDelete}
            onArchive={onArchive}
            onRename={onRename}
            onRestore={onRestore}
            onSettings={onSettings}
            onNavigate={onNavigate}
            workspaceSlug="administrator"
          />
        </SortableContext>
      </DndContext>
    </>,
    { runtime },
  );

  return {
    runtime,
    menu,
    onArchive,
    onDelete,
    onRename,
    onRestore,
    onSettings,
    onNavigate,
  };
}

describe('SortableListLink', () => {
  it.each(['owner', 'viewer'] as const)(
    'keeps a %s list focusable and openable when dragging is disabled',
    (role) => {
      const { onNavigate, runtime } = renderLink({ list: taskList({ role }) });
      // Real useSortable marks its disabled drag wrapper aria-disabled. That
      // drag state must not disable the descendant resource-opening button.
      const open = screen.getByRole('button', { name: /^Launch List/ });
      expect(open.closest('[aria-disabled="true"]')).toBeNull();
      expect(open.parentElement?.getAttribute('role')).toBeNull();
      expect(open.parentElement?.getAttribute('tabindex')).toBeNull();
      open.focus();
      expect(document.activeElement).toBe(open);
      fireEvent.click(open);
      // miy: app-relative path through runtime.navigate (AI-DO: the
      // router location `/w/administrator/pms/lists/list-1`).
      expect(vi.mocked(runtime.navigate).mock.calls[0]?.[0]).toBe(
        '/lists/list-1',
      );
      expect(onNavigate).toHaveBeenCalledTimes(1);
    },
  );

  it('retains sortable keyboard attributes when dragging is allowed', () => {
    renderLink({ canDrag: true });
    const wrapper = screen
      .getByText('Launch List')
      .closest('button')?.parentElement;
    expect(wrapper?.getAttribute('aria-roledescription')).toBe('sortable');
    expect(wrapper?.getAttribute('aria-disabled')).toBe('false');
    expect(wrapper?.getAttribute('tabindex')).toBe('0');
  });
  it('leaves archived tasks out of the total next to the list name', () => {
    renderLink({ list: taskList({ task_count: 5, active_task_count: 3 }) });

    expect(screen.getByText('(2/3)')).toBeTruthy();
    expect(screen.queryByText('(2/5)')).toBeNull();
  });

  it('shows open and total task counts next to the list name', () => {
    renderLink();

    const meta = screen.getByText('(2/3)');
    expect(meta.getAttribute('title')).toContain('2');
    expect(meta.getAttribute('title')).toContain('3');
  });

  it('shows settings, rename, and archive actions for active admin lists', () => {
    const onArchive = vi.fn();
    const onRename = vi.fn();
    const onSettings = vi.fn();
    renderLink({ menuOpen: true, onArchive, onRename, onSettings });

    expect(screen.getByRole('button', { name: '리스트 관리' })).not.toBeNull();

    fireEvent.click(screen.getByRole('menuitem', { name: '설정' }));
    expect(onSettings).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('menuitem', { name: '이름 변경' }));
    expect(onRename).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('menuitem', { name: '보관' }));
    expect(onArchive).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menuitem', { name: '삭제' })).toBeNull();
  });

  it('exposes restore and permanent delete only for archived lists', () => {
    const onDelete = vi.fn();
    const onRestore = vi.fn();
    const archived = renderLink({
      list: taskList({ archived: true, id: 'list-archived' }),
      menuOpen: true,
      onDelete,
      onRename: undefined,
      onRestore,
      onSettings: undefined,
    });
    fireEvent.click(screen.getByRole('menuitem', { name: '복원' }));
    expect(archived.onRestore).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('menuitem', { name: '삭제' }));
    expect(archived.onDelete).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menuitem', { name: '보관' })).toBeNull();
  });

  it('hides list actions for viewer lists', () => {
    renderLink({
      list: taskList({ role: 'viewer' }),
      menuOpen: true,
    });

    expect(screen.queryByRole('button', { name: '리스트 관리' })).toBeNull();
    expect(screen.queryByRole('button', { name: '삭제' })).toBeNull();
  });

  it('supports keyboard focus, arrow navigation, and Escape', () => {
    const onArchive = vi.fn();
    const { menu } = renderLink({ menuOpen: true, onArchive });
    const settings = screen.getByRole('menuitem', { name: '설정' });
    const rename = screen.getByRole('menuitem', { name: '이름 변경' });

    expect(document.activeElement).toBe(settings);
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'ArrowDown' });
    expect(document.activeElement).toBe(rename);
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    expect(menu.onClose).toHaveBeenCalledTimes(1);
    expect(document.activeElement).toBe(
      screen.getByRole('button', { name: '리스트 관리' }),
    );
  });
});
