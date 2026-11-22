import { describe, expect, it } from 'vitest';

import type {
  PmsLabel,
  PmsMilestone,
  PmsTaskListMember,
  TaskFilterParams,
} from '../api/pms-api';
import { createDefaultTaskFilterParams } from '../api/pms-filters';
import {
  buildTaskFilterPills,
  createClearedTaskFilterParams,
  createLoadedSavedTaskFilterParams,
  createOpeningTaskFilterParams,
  createSavedFilter,
  createSavedTaskFilterParams,
  isFilterActive,
  loadSavedFilters,
  paramsFromSavedFilter,
  removeSavedFilterAt,
  saveSavedFilters,
  spaceTasksFilterScopeKey,
  taskExportFilterParams,
  toggleDefaultSavedFilterAt,
  type SavedFilter,
} from './filter-bar-model';

function t(key: string, options?: Record<string, unknown>): string {
  if (!options) return key;
  return `${key}:${Object.entries(options)
    .map(([name, value]) => `${name}=${String(value)}`)
    .join(',')}`;
}

function storage(initial: Record<string, string> = {}) {
  const values = { ...initial };
  return {
    getItem: (key: string) => values[key] ?? null,
    setItem: (key: string, value: string) => {
      values[key] = value;
    },
    values,
  };
}

const members: PmsTaskListMember[] = [
  {
    user_id: 'user-1',
    full_name: 'Ada Lovelace',
    email: 'ada@example.test',
  } as PmsTaskListMember,
];
const labels: PmsLabel[] = [
  { id: 'label-1', name: 'Bug', color: '#ef4444' } as PmsLabel,
];
const milestones: PmsMilestone[] = [
  { id: 'milestone-1', title: 'M1' } as PmsMilestone,
];

describe('filter bar model', () => {
  it('treats q-only and default archive filters as inactive', () => {
    expect(isFilterActive({ q: 'needle', archived_state: 'active' })).toBe(
      false,
    );
    expect(isFilterActive({ archived_state: 'archived' })).toBe(true);
    expect(isFilterActive({ status: ['todo'] })).toBe(true);
  });

  it('exports the whole list while the filter bar is clear', () => {
    const statuses = [
      { category: 'active', slug: 'todo' },
      { category: 'done', slug: 'done' },
    ] as const;

    expect(
      taskExportFilterParams(createDefaultTaskFilterParams(), statuses),
    ).toBeUndefined();
    expect(
      taskExportFilterParams({ archived_state: 'active', q: '  ' }, statuses),
    ).toBeUndefined();
  });

  it('exports what the list shows once a search or filter is on', () => {
    const statuses = [
      { category: 'active', slug: 'todo' },
      { category: 'done', slug: 'done' },
    ] as const;

    // A search alone counts, and the completed tasks the list hides stay out.
    expect(
      taskExportFilterParams(
        { archived_state: 'active', q: '[휴가]' },
        statuses,
      ),
    ).toEqual({ archived_state: 'active', q: '[휴가]', status: ['todo'] });
    expect(
      taskExportFilterParams(
        {
          archived_state: 'active',
          label_id: 'leave',
          status: ['todo', 'done'],
        },
        statuses,
      ),
    ).toEqual({
      archived_state: 'active',
      label_id: 'leave',
      status: ['todo', 'done'],
    });
  });

  it('clears all filters while preserving search text', () => {
    expect(
      createClearedTaskFilterParams({
        q: 'needle',
        status: ['todo'],
        priority: 'high',
        archived_state: 'archived',
      }),
    ).toEqual({ archived_state: 'active', q: 'needle' });
  });

  it('strips search text when saving and restores current search when loading', () => {
    const savedParams = createSavedTaskFilterParams({
      q: 'draft search',
      label_id: 'label-1',
    });

    expect(savedParams).toEqual({
      archived_state: 'active',
      label_id: 'label-1',
      q: undefined,
    });
    expect(
      createLoadedSavedTaskFilterParams(savedParams, { q: 'current search' }),
    ).toEqual({
      archived_state: 'active',
      label_id: 'label-1',
      q: 'current search',
    });
  });

  it('normalizes saved filters and removes filters by index', () => {
    const saved = createSavedFilter('  Mine  ', {
      q: 'hidden',
      assignee_id: 'user-1',
    });
    expect(saved).toEqual({
      name: 'Mine',
      params: {
        archived_state: 'active',
        assignee_id: 'user-1',
        q: undefined,
      },
    });
    expect(createSavedFilter('   ', {})).toBeNull();

    const filters: SavedFilter[] = [
      saved as SavedFilter,
      { name: 'Other', params: { priority: 'low' } },
    ];
    expect(removeSavedFilterAt(filters, 0)).toEqual([filters[1]]);
    expect(paramsFromSavedFilter(filters[1], { q: 'search' })).toEqual({
      archived_state: 'active',
      priority: 'low',
      q: 'search',
    });
  });

  it('persists saved filters under the task list key', () => {
    const fakeStorage = storage();
    const filters: SavedFilter[] = [{ name: 'Mine', params: { status: ['todo'] } }];

    saveSavedFilters('list-1', filters, fakeStorage);

    expect(loadSavedFilters('list-1', fakeStorage)).toEqual(filters);
    expect(loadSavedFilters('list-2', fakeStorage)).toEqual([]);
  });

  it('keeps at most one default saved filter and toggles it off again', () => {
    const filters: SavedFilter[] = [
      { name: 'Mine', params: { assignee_id: 'user-1' } },
      { name: 'Todo', params: { status: ['todo'] }, isDefault: true },
    ];

    const mineDefault = toggleDefaultSavedFilterAt(filters, 0);
    expect(mineDefault).toEqual([
      { name: 'Mine', params: { assignee_id: 'user-1' }, isDefault: true },
      { name: 'Todo', params: { status: ['todo'] } },
    ]);
    expect(toggleDefaultSavedFilterAt(mineDefault, 0)).toEqual([
      { name: 'Mine', params: { assignee_id: 'user-1' } },
      { name: 'Todo', params: { status: ['todo'] } },
    ]);
  });

  it('opens a list with its default saved filter, never with saved search text', () => {
    const fakeStorage = storage();
    saveSavedFilters(
      'list-1',
      [
        { name: 'Mine', params: { assignee_id: 'user-1' } },
        {
          name: 'Todo',
          params: { q: 'stale search', status: ['todo'] },
          isDefault: true,
        },
      ],
      fakeStorage,
    );

    expect(createOpeningTaskFilterParams('list-1', fakeStorage)).toEqual(
      createSavedTaskFilterParams({ status: ['todo'] }),
    );
    expect(createOpeningTaskFilterParams('list-2', fakeStorage)).toEqual(
      createDefaultTaskFilterParams(),
    );
    expect(spaceTasksFilterScopeKey('space-1')).toBe('space-tasks:space-1');
  });

  it('returns no saved filters for invalid storage payloads', () => {
    const fakeStorage = storage({ pms_saved_filters_list: '{' });

    expect(loadSavedFilters('list', fakeStorage)).toEqual([]);
  });

  it('builds localized pills and clear params', () => {
    const params: TaskFilterParams = {
      status: ['todo', 'custom'],
      priority: 'high',
      assignee_id: 'user-1',
      label_id: 'label-1',
      milestone_id: 'milestone-1',
      start_date_from: '2026-05-01',
      due_date_to: '2026-05-30',
      archived_state: 'archived',
      q: 'needle',
    };

    const pills = buildTaskFilterPills({
      labels,
      members,
      milestones,
      params,
      statusOptions: [{ value: 'todo', label: 'To do' }],
      translate: t,
    });

    expect(pills.map((pill) => pill.label)).toEqual([
      'pms.filter.pillStatus:value=To do, custom',
      'pms.filter.pillPriority:value=pms.priorityHigh',
      'pms.filter.pillAssignee:value=Ada Lovelace',
      'pms.filter.pillLabel:value=Bug',
      'pms.filter.pillMilestone:value=M1',
      'pms.filter.pillStart:from=2026-05-01,to=...',
      'pms.filter.pillDue:from=...,to=2026-05-30',
      'pms.filter.pillArchive:value=pms.filter.archive.archived',
    ]);
    expect(pills[0].clearParams).toMatchObject({
      q: 'needle',
      status: undefined,
    });
    expect(pills[pills.length - 1].clearParams).toMatchObject({
      archived_state: 'active',
      q: 'needle',
    });
  });
});
