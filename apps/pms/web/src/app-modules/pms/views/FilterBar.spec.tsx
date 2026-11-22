import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n/i18n';
import type { PmsSavedFilter, PmsTaskListStatus } from '../api/pms-api';
import type { PmsSavedFiltersController } from './usePmsSavedFilters';
import { FilterBar } from './FilterBar';

const statuses: PmsTaskListStatus[] = [
  {
    id: 'status-todo',
    category: 'not_started',
    name: '할 일',
    sort_order: 0,
    color: '#94a3b8',
    slug: 'todo',
  },
  {
    id: 'status-doing',
    category: 'active',
    name: '진행 중',
    sort_order: 1,
    color: '#94a3b8',
    slug: 'doing',
  },
  {
    id: 'status-done',
    category: 'done',
    name: '완료됨',
    sort_order: 2,
    color: '#94a3b8',
    slug: 'done',
  },
  {
    id: 'status-closed',
    category: 'closed',
    name: '종료됨',
    sort_order: 3,
    color: '#94a3b8',
    slug: 'closed',
  },
];

function savedFilter(overrides: Partial<PmsSavedFilter>): PmsSavedFilter {
  return {
    id: 'filter',
    name: 'Filter',
    params: { archived_state: 'active' },
    sort: null,
    group_by: null,
    shared: false,
    space_id: 'space-1',
    list_id: 'list-1',
    owner_id: 'user-1',
    owner_name: '홍길동',
    is_mine: true,
    is_default: false,
    can_manage: true,
    created_at: '2026-11-22T00:00:00',
    updated_at: '2026-11-22T00:00:00',
    ...overrides,
  };
}

function savedFilterController(items: PmsSavedFilter[]) {
  return {
    scopeKey: 'list:list-1',
    items,
    loading: false,
    error: null,
    actionError: null,
    clearActionError: vi.fn(),
    create: vi.fn(async () => true),
    update: vi.fn(async () => true),
    remove: vi.fn(async () => true),
    setDefault: vi.fn(async () => true),
  } satisfies PmsSavedFiltersController;
}

describe('FilterBar', () => {
  beforeEach(async () => {
    localStorage.clear();
    await i18n.changeLanguage('ko-KR');
  });

  it('checks non-completion statuses on the initial screen', () => {
    render(
      <FilterBar
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        setFilterParams={vi.fn()}
        taskListId="list-1"
        taskListStatuses={statuses}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '상태' }));

    expect(
      (screen.getByRole('checkbox', { name: '할 일' }) as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      (screen.getByRole('checkbox', { name: '진행 중' }) as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      (screen.getByRole('checkbox', { name: '완료됨' }) as HTMLInputElement)
        .checked,
    ).toBe(false);
    expect(
      (screen.getByRole('checkbox', { name: '종료됨' }) as HTMLInputElement)
        .checked,
    ).toBe(false);
  });

  it('starts a direct status edit from the effective default selection', () => {
    const setFilterParams = vi.fn();
    render(
      <FilterBar
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        setFilterParams={setFilterParams}
        taskListId="list-1"
        taskListStatuses={statuses}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '상태' }));
    fireEvent.click(screen.getByRole('checkbox', { name: '완료됨' }));

    expect(setFilterParams).toHaveBeenCalledWith({
      archived_state: 'active',
      status: ['todo', 'doing', 'done'],
    });
  });

  // miy: saved filters are on the server; the bar gets the view's
  // controller (usePmsSavedFilters) instead of reading browser storage.
  it('makes one saved filter the view default from the saved menu', () => {
    const controller = savedFilterController([
      savedFilter({ id: 'f-1', name: '내 업무', params: { assignee_id: 'user-1' } }),
      savedFilter({ id: 'f-2', name: '이번 주', is_default: true }),
    ]);
    render(
      <FilterBar
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        savedFilters={controller}
        setFilterParams={vi.fn()}
        taskListId="list-1"
        taskListStatuses={statuses}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '저장됨 2' }));
    expect(
      screen.getByText(
        '기본으로 지정한 필터는 이 목록을 열 때 자동으로 걸립니다.',
      ),
    ).not.toBeNull();
    fireEvent.click(
      screen.getByRole('button', { name: '내 업무 필터를 기본으로 지정' }),
    );
    expect(controller.setDefault).toHaveBeenCalledWith(controller.items[0]);
    const unset = screen.getByRole('button', { name: '이번 주 필터 기본 해제' });
    expect(unset.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(unset);
    expect(controller.setDefault).toHaveBeenLastCalledWith(null);
  });

  it('applies a saved filter through the view so its sort comes along', () => {
    const onApply = vi.fn();
    const controller = savedFilterController([
      savedFilter({ id: 'f-1', name: '마감순', sort: { field: 'due_date', direction: 'asc' } }),
    ]);
    render(
      <FilterBar
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        onApplySavedFilter={onApply}
        savedFilters={controller}
        setFilterParams={vi.fn()}
        taskListId="list-1"
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: '저장됨 1' }));
    fireEvent.click(screen.getAllByRole('button', { name: /^마감순/ })[0]);
    expect(onApply).toHaveBeenCalledWith(controller.items[0]);
  });

  it('saves the current filters with the view sort and keeps the name when the save fails', async () => {
    const controller = savedFilterController([]);
    controller.create.mockResolvedValueOnce(false);
    render(
      <FilterBar
        canShareSavedFilters
        captureSavedFilter={() => ({
          sort: { field: 'title', direction: 'desc' },
          group_by: 'none',
        })}
        filterParams={{ archived_state: 'active', priority: 'high', q: '검색' }}
        labels={[]}
        members={[]}
        milestones={[]}
        savedFilters={controller}
        setFilterParams={vi.fn()}
        taskListId="list-1"
      />,
    );
    fireEvent.click(screen.getAllByRole('button', { name: '저장' })[0]);
    const nameBox = screen.getAllByRole('textbox', { name: '필터 이름' })[0];
    fireEvent.change(nameBox, { target: { value: '긴급' } });
    fireEvent.click(screen.getAllByRole('checkbox', { name: '스페이스에 공유' })[0]);
    // The inline "save" action first, then the dialog's own save button.
    const saveButtons = screen.getAllByRole('button', { name: '저장' });
    fireEvent.click(saveButtons[saveButtons.length - 1]);
    await waitFor(() => expect(controller.create).toHaveBeenCalledTimes(1));
    expect(controller.create).toHaveBeenCalledWith({
      name: '긴급',
      shared: true,
      sort: { field: 'title', direction: 'desc' },
      group_by: 'none',
      params: { archived_state: 'active', priority: 'high', q: undefined },
    });
    await waitFor(() =>
      expect(
        (screen.getAllByRole('textbox', { name: '필터 이름' })[0] as HTMLInputElement)
          .value,
      ).toBe('긴급'),
    );
  });

  it('offers rename, share and delete only for filters the person manages', () => {
    const controller = savedFilterController([
      savedFilter({ id: 'f-1', name: '내 필터' }),
      savedFilter({
        id: 'f-2',
        name: '팀 필터',
        shared: true,
        is_mine: false,
        can_manage: false,
        owner_name: '김관리',
      }),
    ]);
    render(
      <FilterBar
        canShareSavedFilters
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        savedFilters={controller}
        setFilterParams={vi.fn()}
        taskListId="list-1"
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: '저장됨 2' }));
    expect(screen.queryByRole('button', { name: '팀 필터 이름 바꾸기' })).toBeNull();
    expect(screen.queryByRole('button', { name: '팀 필터 필터 제거' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '내 필터 스페이스에 공유' }));
    expect(controller.update).toHaveBeenCalledWith(controller.items[0], {
      shared: true,
    });
    fireEvent.click(screen.getByRole('button', { name: '내 필터 필터 제거' }));
    expect(controller.remove).toHaveBeenCalledWith(controller.items[0]);
    fireEvent.click(screen.getByRole('button', { name: '내 필터 이름 바꾸기' }));
    const rename = screen.getByRole('textbox', { name: '내 필터 이름 바꾸기' });
    fireEvent.change(rename, { target: { value: '새 이름' } });
    fireEvent.keyDown(rename, { key: 'Enter' });
    expect(controller.update).toHaveBeenLastCalledWith(controller.items[0], {
      name: '새 이름',
    });
  });

  it('filters by a custom field option', () => {
    const setFilterParams = vi.fn();
    render(
      <FilterBar
        customFields={[
          {
            id: 'field-1',
            list_id: 'list-1',
            name: '고객',
            field_type: 'select',
            options: ['A사', 'B사'],
            sort_order: 0,
          },
        ]}
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        setFilterParams={setFilterParams}
        taskListId="list-1"
      />,
    );
    fireEvent.click(screen.getAllByRole('button', { name: '필드' })[0]);
    fireEvent.change(screen.getAllByRole('combobox', { name: '고객' })[0], {
      target: { value: 'B사' },
    });
    expect(setFilterParams).toHaveBeenCalledWith({
      archived_state: 'active',
      custom_fields: { 'field-1': 'B사' },
    });
  });

  it('layers the assignee menu above sticky gantt headers', () => {
    render(
      <FilterBar
        filterParams={{ archived_state: 'active' }}
        labels={[]}
        members={[]}
        milestones={[]}
        setFilterParams={vi.fn()}
        taskListId="list-1"
        taskListStatuses={statuses}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '담당자' }));

    const menu = screen
      .getByRole('textbox', {
        name: '이름, 이메일 또는 부서로 사용자 검색',
      })
      .closest('.absolute');
    expect(menu?.classList.contains('z-[var(--ui-z-popover)]')).toBe(true);
  });
});
