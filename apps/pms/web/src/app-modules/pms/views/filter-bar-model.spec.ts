import { describe, expect, it } from 'vitest';

import type {
  PmsCustomField,
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
  createSavedTaskFilterParams,
  isFilterActive,
  taskExportFilterParams,
} from './filter-bar-model';

function t(key: string, options?: Record<string, unknown>): string {
  if (!options) return key;
  return `${key}:${Object.entries(options)
    .map(([name, value]) => `${name}=${String(value)}`)
    .join(',')}`;
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

  // miy: saved filters live on the server (usePmsSavedFilters.spec.tsx);
  // AI-DO's browser-storage cases (index removal, storage keys, the default
  // star, invalid payloads, opening from storage) went with that storage.
  it('counts custom field conditions as filters and saves them', () => {
    const params: TaskFilterParams = {
      archived_state: 'active',
      custom_fields: { 'field-1': 'A' },
    };
    expect(isFilterActive(params)).toBe(true);
    expect(isFilterActive({ custom_fields: { 'field-1': '  ' } })).toBe(false);
    expect(createSavedTaskFilterParams({ ...params, q: 'x' })).toEqual({
      ...params,
      q: undefined,
    });
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

  it('names custom field pills by field and value and clears one at a time', () => {
    const customFields = [
      { id: 'owner', name: 'Owner', field_type: 'user', sort_order: 0 },
      { id: 'tags', name: 'Tags', field_type: 'multi_select', options: ['UI'], sort_order: 1 },
    ] as PmsCustomField[];
    const pills = buildTaskFilterPills({
      customFields,
      labels,
      members,
      milestones,
      params: {
        archived_state: 'active',
        custom_fields: { owner: 'user-1', tags: 'UI', gone: 'x' },
      },
      statusOptions: [],
      translate: t,
    });
    expect(pills.map((pill) => pill.label)).toEqual([
      'pms.customFields.pill:name=Owner,value=Ada Lovelace',
      'pms.customFields.pill:name=Tags,value=UI',
      'pms.customFields.pill:name=common:feedback.unknown,value=x',
    ]);
    expect(pills[0].clearParams.custom_fields).toEqual({ tags: 'UI', gone: 'x' });
  });
});
