import { describe, expect, it } from 'vitest';

import type { PmsTask, PmsTaskList } from '../api/pms-api';
import {
  combineSpaceTasks,
  loadSpaceTasksLayout,
  saveSpaceTasksLayout,
  supportsCombinedSpaceLayout,
} from './space-tasks-layout-model';

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

const list = (id: string, name: string) => ({ id, name }) as PmsTaskList;
const task = (id: string) => ({ id, title: id }) as PmsTask;

describe('space tasks layout model', () => {
  it('remembers the layout per space and starts per list otherwise', () => {
    const fakeStorage = storage({ pms_space_tasks_layout_space_b: 'garbage' });

    expect(loadSpaceTasksLayout('space-a', fakeStorage)).toBe('by-list');
    saveSpaceTasksLayout('space-a', 'combined', fakeStorage);
    expect(loadSpaceTasksLayout('space-a', fakeStorage)).toBe('combined');
    expect(loadSpaceTasksLayout('space-b', fakeStorage)).toBe('by-list');
    expect(loadSpaceTasksLayout('space-a', null)).toBe('by-list');
  });

  it('falls back to per list when browser storage throws', () => {
    const throwing = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };

    expect(loadSpaceTasksLayout('space-a', throwing)).toBe('by-list');
    expect(() =>
      saveSpaceTasksLayout('space-a', 'combined', throwing),
    ).not.toThrow();
  });

  it('only merges lists on the date tabs', () => {
    expect(supportsCombinedSpaceLayout('gantt')).toBe(true);
    expect(supportsCombinedSpaceLayout('calendar')).toBe(true);
    expect(supportsCombinedSpaceLayout('list')).toBe(false);
    expect(supportsCombinedSpaceLayout('board')).toBe(false);
    expect(supportsCombinedSpaceLayout('table')).toBe(false);
  });

  it('keeps list order and remembers which list each task came from', () => {
    const listA = list('list-a', 'List A');
    const listB = list('list-b', 'List B');
    const combined = combineSpaceTasks(
      [listA, listB, list('list-c', 'Empty')],
      new Map([
        ['list-b', [task('b-1')]],
        ['list-a', [task('a-1'), task('a-2')]],
      ]),
    );

    expect(combined.tasks.map((item) => item.id)).toEqual([
      'a-1',
      'a-2',
      'b-1',
    ]);
    expect(combined.listByTaskId.get('b-1')).toBe(listB);
    expect(Array.from(combined.listNameByTaskId.entries())).toEqual([
      ['a-1', 'List A'],
      ['a-2', 'List A'],
      ['b-1', 'List B'],
    ]);
  });
});
