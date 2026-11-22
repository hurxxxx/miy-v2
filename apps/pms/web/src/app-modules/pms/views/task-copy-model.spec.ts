import { describe, expect, it } from 'vitest';

import type { PmsTaskList } from '../api/pms-api';
import {
  buildTaskCopyTargets,
  filterTaskCopyTargets,
  taskCopyUnmatchedParts,
} from './task-copy-model';

function taskList(overrides: Partial<PmsTaskList>): PmsTaskList {
  return {
    id: 'list',
    key: 'KEY',
    name: 'List',
    team_name: 'Space',
    archived: false,
    role: 'member',
    ...overrides,
  } as PmsTaskList;
}

describe('task copy model', () => {
  it('offers editable active lists with the current list first', () => {
    const targets = buildTaskCopyTargets(
      [
        taskList({ id: 'b', key: 'B', name: 'Beta', team_name: 'Zeta' }),
        taskList({ id: 'a', key: 'A', name: 'Alpha', team_name: 'Zeta' }),
        taskList({ id: 'c', key: 'C', name: 'Gamma', team_name: 'Alpha' }),
        taskList({ id: 'cur', key: 'CUR', name: 'Current', team_name: 'Zeta' }),
        taskList({ id: 'v', name: 'Viewer only', role: 'viewer' }),
        taskList({ id: 'x', name: 'Archived', archived: true }),
      ],
      'cur',
    );

    expect(targets.map((target) => target.id)).toEqual(['cur', 'c', 'a', 'b']);
    expect(targets[0]).toEqual({
      id: 'cur',
      key: 'CUR',
      name: 'Current',
      spaceName: 'Zeta',
      isCurrent: true,
    });
  });

  it('keeps lists without a space name', () => {
    const [target] = buildTaskCopyTargets(
      [taskList({ id: 'n', team_name: null })],
      'other',
    );

    expect(target.spaceName).toBe('');
  });

  it('filters by list name, key or space name', () => {
    const targets = buildTaskCopyTargets(
      [
        taskList({
          id: 'a',
          key: 'HEX',
          name: 'Heat exchanger',
          team_name: 'R&D',
        }),
        taskList({
          id: 'b',
          key: 'OPS',
          name: 'Operations',
          team_name: 'Plant',
        }),
      ],
      'none',
    );

    expect(filterTaskCopyTargets(targets, ' hex ').map((t) => t.id)).toEqual([
      'a',
    ]);
    expect(filterTaskCopyTargets(targets, 'plant').map((t) => t.id)).toEqual([
      'b',
    ]);
    expect(filterTaskCopyTargets(targets, '')).toHaveLength(2);
  });

  it('lists only the kinds that lost values, in a fixed order', () => {
    expect(
      taskCopyUnmatchedParts({
        assignees: 0,
        labels: 2,
        milestones: 0,
        custom_fields: 1,
      }),
    ).toEqual([
      { kind: 'labels', count: 2 },
      { kind: 'custom_fields', count: 1 },
    ]);
    expect(
      taskCopyUnmatchedParts({
        assignees: 0,
        labels: 0,
        milestones: 0,
        custom_fields: 0,
      }),
    ).toEqual([]);
  });
});
