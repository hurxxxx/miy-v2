import type { PmsTask, PmsTaskListMember } from '../api/pms-api';
import {
  getTaskAssigneeGroupKey,
  getTaskAssigneeIds,
  getTaskLabelGroupKey,
  getTaskLabelNames,
  getTaskRoot,
  type IssueHierarchyInfo,
} from './pms-task-hierarchy';

export type AssigneeTaskGroup = {
  assigneeIds: string[];
  key: string;
  issues: PmsTask[];
  label: string;
  rootCount: number;
};

function taskAssigneeName(
  task: PmsTask,
  assigneeId: string,
  members: readonly PmsTaskListMember[],
): string {
  const member = members.find((item) => item.user_id === assigneeId);
  if (member?.full_name) return member.full_name;
  const assigneeIndex = task.assignee_ids?.indexOf(assigneeId) ?? -1;
  if (assigneeIndex >= 0 && task.assignee_names?.[assigneeIndex]) {
    return task.assignee_names[assigneeIndex];
  }
  if (task.assignee_id === assigneeId && task.assignee_name) {
    return task.assignee_name;
  }
  return assigneeId;
}

export function buildAssigneeTaskGroups({
  hierarchy,
  members,
  orderedTasks,
  unassignedLabel,
  visibleTasks,
}: {
  hierarchy: Map<string, IssueHierarchyInfo>;
  members: readonly PmsTaskListMember[];
  orderedTasks: readonly PmsTask[];
  unassignedLabel: string;
  visibleTasks: readonly PmsTask[];
}): AssigneeTaskGroup[] {
  const groups = new Map<string, AssigneeTaskGroup>();
  for (const root of orderedTasks) {
    if (getTaskRoot(root, hierarchy).id !== root.id) continue;
    const assigneeIds = getTaskAssigneeIds(root);
    const key = getTaskAssigneeGroupKey(root);
    const current = groups.get(key);
    if (current) {
      current.rootCount += 1;
      continue;
    }
    groups.set(key, {
      assigneeIds,
      key,
      issues: [],
      label:
        assigneeIds.length > 0
          ? assigneeIds
              .map((assigneeId) => taskAssigneeName(root, assigneeId, members))
              .join(' · ')
          : unassignedLabel,
      rootCount: 1,
    });
  }
  for (const task of visibleTasks) {
    const root = getTaskRoot(task, hierarchy);
    const key = getTaskAssigneeGroupKey(root);
    groups.get(key)?.issues.push(task);
  }
  return Array.from(groups.values());
}

export type LabelTaskGroup = {
  /** One dot per label, in the same order as `labelNames`. */
  colors: string[];
  key: string;
  issues: PmsTask[];
  label: string;
  labelNames: string[];
  rootCount: number;
};

/** Groups by the root task's labels, like the assignee grouping. A subtask stays under
 *  its root's group so the tree is not split. Unlabelled tasks come last. */
export function buildLabelTaskGroups({
  hierarchy,
  noLabelLabel,
  orderedTasks,
  visibleTasks,
}: {
  hierarchy: Map<string, IssueHierarchyInfo>;
  noLabelLabel: string;
  orderedTasks: readonly PmsTask[];
  visibleTasks: readonly PmsTask[];
}): LabelTaskGroup[] {
  const groups = new Map<string, LabelTaskGroup>();
  for (const root of orderedTasks) {
    if (getTaskRoot(root, hierarchy).id !== root.id) continue;
    const key = getTaskLabelGroupKey(root);
    const current = groups.get(key);
    if (current) {
      current.rootCount += 1;
      continue;
    }
    const labelNames = getTaskLabelNames(root);
    groups.set(key, {
      colors: labelNames.map(
        (name) =>
          root.labels?.find((label) => label.name.trim() === name)?.color ?? '',
      ),
      key,
      issues: [],
      label: labelNames.length > 0 ? labelNames.join(' · ') : noLabelLabel,
      labelNames,
      rootCount: 1,
    });
  }
  for (const task of visibleTasks) {
    const root = getTaskRoot(task, hierarchy);
    groups.get(getTaskLabelGroupKey(root))?.issues.push(task);
  }
  const all = Array.from(groups.values());
  return [
    ...all.filter((group) => group.labelNames.length > 0),
    ...all.filter((group) => group.labelNames.length === 0),
  ];
}
