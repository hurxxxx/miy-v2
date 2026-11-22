import { describe, expect, it } from 'vitest';

import type { PmsTask, PmsTaskListMember } from '../api/pms-api';
import {
  buildAssigneeTaskGroups,
  buildLabelTaskGroups,
} from './list-view-grouping-model';
import { buildTaskHierarchy, sortTasksByHierarchy } from './pms-task-hierarchy';

function task(id: string, overrides: Partial<PmsTask> = {}): PmsTask {
  return {
    assignee_id: null,
    assignee_ids: [],
    assignee_name: null,
    assignee_names: [],
    board_position: 1000,
    id,
    parent_id: null,
    status: 'todo',
    title: id,
    ...overrides,
  } as PmsTask;
}

describe('list view grouping model', () => {
  it('groups a task hierarchy by the normalized root assignee set', () => {
    const root = task('root', {
      assignee_id: 'user-1',
      assignee_ids: ['user-2', 'user-1'],
      assignee_name: 'User One',
      assignee_names: ['User Two', 'User One'],
    });
    const child = task('child', {
      assignee_id: 'user-1',
      assignee_ids: ['user-1'],
      board_position: 2000,
      parent_id: root.id,
    });
    const sameCombinationRoot = task('same-combination-root', {
      assignee_id: 'user-2',
      assignee_ids: ['user-1', 'user-2'],
      assignee_name: 'User Two',
      assignee_names: ['User One', 'User Two'],
      board_position: 3000,
    });
    const tasks = [child, sameCombinationRoot, root];
    const hierarchy = buildTaskHierarchy(tasks);

    const groups = buildAssigneeTaskGroups({
      hierarchy,
      members: [
        {
          full_name: 'User Two',
          user_id: 'user-2',
        } as PmsTaskListMember,
      ],
      orderedTasks: sortTasksByHierarchy(tasks),
      unassignedLabel: 'Unassigned',
      visibleTasks: sortTasksByHierarchy(tasks),
    });

    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject({
      assigneeIds: ['user-1', 'user-2'],
      issues: [root, child, sameCombinationRoot],
      label: 'User One · User Two',
      rootCount: 2,
    });
  });

  it('groups by root label names across lists and puts unlabelled tasks last', () => {
    const unlabelled = task('unlabelled', { labels: [] });
    const leave = task('leave', {
      board_position: 2000,
      labels: [{ color: '#22c55e', id: 'list-a-leave', name: '휴가' }],
    });
    const child = task('child', {
      board_position: 3000,
      labels: [{ color: '#3b82f6', id: 'list-a-trip', name: '출장' }],
      parent_id: leave.id,
    });
    // Another list's own 휴가 label has another id but the same name.
    const otherListLeave = task('other-list-leave', {
      board_position: 4000,
      labels: [{ color: '#16a34a', id: 'list-b-leave', name: ' 휴가 ' }],
    });
    const tripAndTraining = task('trip-and-training', {
      board_position: 5000,
      labels: [
        { color: '#f97316', id: 'training', name: '교육' },
        { color: '#3b82f6', id: 'list-a-trip', name: '출장' },
      ],
    });
    const tasks = [unlabelled, leave, child, otherListLeave, tripAndTraining];
    const hierarchy = buildTaskHierarchy(tasks);

    const groups = buildLabelTaskGroups({
      hierarchy,
      noLabelLabel: 'No label',
      orderedTasks: sortTasksByHierarchy(tasks),
      visibleTasks: sortTasksByHierarchy(tasks),
    });

    expect(
      groups.map(({ colors, issues, label, rootCount }) => ({
        colors,
        issueIds: issues.map((issue) => issue.id),
        label,
        rootCount,
      })),
    ).toEqual([
      {
        colors: ['#22c55e'],
        // The subtask stays under its root's label, not its own.
        issueIds: ['leave', 'child', 'other-list-leave'],
        label: '휴가',
        rootCount: 2,
      },
      {
        colors: ['#f97316', '#3b82f6'],
        issueIds: ['trip-and-training'],
        label: '교육 · 출장',
        rootCount: 1,
      },
      {
        colors: [],
        issueIds: ['unlabelled'],
        label: 'No label',
        rootCount: 1,
      },
    ]);
  });
});
