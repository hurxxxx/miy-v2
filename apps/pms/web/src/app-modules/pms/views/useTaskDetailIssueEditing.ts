import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { TFunction } from 'i18next';
import type { BlockContent } from '../../../components/editor';

import {
  setTaskAssignees,
  setTaskFollowers,
  updateTask,
  type PmsTask,
  type PmsTaskListMember,
  type PmsTaskListStatus,
} from '../api/pms-api';
import {
  applyTaskUserRoleState,
  getErrorMessage,
  resolveSelectedAssigneeIds,
  resolveTaskDetailMemberNames,
  resolveTaskDetailStatusLabel,
  restoreTaskUserRoleState,
  toggleTaskUserRoleId,
  type TaskUserRole,
} from './task-detail-editing-model';
import { type AppApi } from '../../../platform/runtime/runtime-context';

export {
  resolveSelectedAssigneeIds,
  resolveTaskDetailMemberNames,
} from './task-detail-editing-model';

type TaskDetailField =
  | keyof PmsTask
  | 'label_ids'
  | 'description_blocks'
  | 'parent_id'
  | 'archived';

export function useTaskDetailIssueEditing({
  workspaceSlug,
  canEdit,
  members,
  onUpdate,
  task,
  taskListStatuses,
  api,
  t,
}: {
  workspaceSlug?: string | null;
  canEdit: boolean;
  members: PmsTaskListMember[];
  onUpdate?: () => void | Promise<void>;
  task: PmsTask;
  taskListStatuses?: PmsTaskListStatus[];
  api: AppApi | null;
  t: TFunction;
}) {
  const [syncedTask, setSyncedTask] = useState(task);
  const [issueState, setIssueState] = useState(task);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [titleDraft, setTitleDraft] = useState(task.title);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // miy: a description change waiting for its delayed save or in flight,
  // reported as unsaved work (runtime.setNavigationState).
  const [descriptionSavePending, setDescriptionSavePending] = useState(false);
  const descriptionBlocksRef = useRef<BlockContent | null>(
    (task.description_blocks as BlockContent | null) ?? null,
  );
  const userRoleRequestSeqRef = useRef<
    Record<'assignees' | 'followers', number>
  >({
    assignees: 0,
    followers: 0,
  });

  const selectedLabelIds = useMemo(
    () => issueState.labels.map((label) => label.id),
    [issueState.labels],
  );
  const selectedAssigneeIds = useMemo(
    () =>
      resolveSelectedAssigneeIds({
        assignee_id: issueState.assignee_id,
        assignee_ids: issueState.assignee_ids,
      }),
    [issueState.assignee_id, issueState.assignee_ids],
  );
  const selectedFollowerIds = useMemo(
    () => issueState.follower_ids ?? [],
    [issueState.follower_ids],
  );
  const memberNamesForIds = useCallback(
    (userIds: string[], fallbackNames: string[] = []) =>
      resolveTaskDetailMemberNames(members, userIds, fallbackNames),
    [members],
  );
  const statusLabel = useCallback(
    (slug: string) => resolveTaskDetailStatusLabel(slug, taskListStatuses, t),
    [taskListStatuses, t],
  );

  if (task !== syncedTask) {
    setSyncedTask(task);
    setIssueState(task);
    setTitleDraft(task.title);
    descriptionBlocksRef.current =
      (task.description_blocks as BlockContent | null) ?? null;
    setSaveError(null);
  }

  useEffect(
    () => () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
    },
    [],
  );

  const persistIssueUpdate = useCallback(
    async (
      payload: Record<string, unknown>,
      applyOptimistic: (current: PmsTask) => PmsTask,
      fallbackMessage: string,
    ) => {
      if (!api || !canEdit) return null;

      const previousIssue = issueState;
      setSaveError(null);
      setIssueState((current) => applyOptimistic(current));

      try {
        const updatedIssue = await updateTask(
          api,
          issueState.id,
          payload,
          workspaceSlug,
        );
        setIssueState(updatedIssue);
        await Promise.resolve(onUpdate?.());
        return updatedIssue;
      } catch (error) {
        setIssueState(previousIssue);
        setSaveError(getErrorMessage(error, fallbackMessage));
        return null;
      }
    },
    [canEdit, issueState, onUpdate, api, workspaceSlug],
  );

  const patchField = useCallback(
    (field: TaskDetailField, value: unknown) => {
      void persistIssueUpdate(
        { [field]: value },
        (current) => ({ ...current, [field]: value }) as PmsTask,
        t('pms.taskDetail.errors.saveIssueFailed'),
      );
    },
    [persistIssueUpdate, t],
  );

  const commitTitleChange = useCallback(async () => {
    if (!canEdit) {
      setTitleDraft(issueState.title);
      return;
    }
    const nextTitle = titleDraft.trim();
    if (nextTitle.length < 2 || nextTitle === issueState.title) {
      setTitleDraft(issueState.title);
      return;
    }

    await persistIssueUpdate(
      { title: nextTitle },
      (current) => ({ ...current, title: nextTitle }),
      t('pms.taskDetail.errors.saveIssueFailed'),
    );
  }, [canEdit, issueState.title, persistIssueUpdate, t, titleDraft]);

  const setIssueUserRoleIds = useCallback(
    async (role: TaskUserRole, userIds: string[]) => {
      if (!api || !canEdit) return;
      const previousIssue = issueState;
      const nextNames = memberNamesForIds(userIds);
      const requestSeq = userRoleRequestSeqRef.current[role] + 1;
      userRoleRequestSeqRef.current[role] = requestSeq;
      setSaveError(null);
      setIssueState((current) =>
        applyTaskUserRoleState(current, role, userIds, nextNames),
      );

      try {
        const saved =
          role === 'assignees'
            ? await setTaskAssignees(
                api,
                issueState.id,
                userIds,
                workspaceSlug,
              )
            : await setTaskFollowers(
                api,
                issueState.id,
                userIds,
                workspaceSlug,
              );
        if (userRoleRequestSeqRef.current[role] === requestSeq) {
          const savedIds = saved.map((item) => item.user_id);
          const savedNames = saved.map((item) => item.full_name);
          setIssueState((current) =>
            applyTaskUserRoleState(current, role, savedIds, savedNames),
          );
          await Promise.resolve(onUpdate?.());
        }
      } catch (error) {
        if (userRoleRequestSeqRef.current[role] !== requestSeq) {
          return;
        }
        setIssueState((current) =>
          restoreTaskUserRoleState(current, role, previousIssue),
        );
        setSaveError(
          getErrorMessage(
            error,
            t('pms.taskDetail.errors.saveIssueUsersFailed'),
          ),
        );
      }
    },
    [canEdit, issueState, memberNamesForIds, onUpdate, t, api, workspaceSlug],
  );

  const toggleIssueUserRole = useCallback(
    (role: TaskUserRole, userId: string) => {
      const currentIds =
        role === 'assignees' ? selectedAssigneeIds : selectedFollowerIds;
      const nextIds = toggleTaskUserRoleId(currentIds, userId);
      void setIssueUserRoleIds(role, nextIds);
    },
    [selectedAssigneeIds, selectedFollowerIds, setIssueUserRoleIds],
  );

  const handleDescriptionChange = useCallback(
    (content: BlockContent) => {
      descriptionBlocksRef.current = content;
      if (!api || !canEdit) return;
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      setSaveError(null);
      setDescriptionSavePending(true);
      saveTimerRef.current = setTimeout(() => {
        saveTimerRef.current = null;
        updateTask(
          api,
          issueState.id,
          { description_blocks: content },
          workspaceSlug,
        )
          .then(async (updatedIssue) => {
            setIssueState(updatedIssue);
            descriptionBlocksRef.current =
              (updatedIssue.description_blocks as BlockContent | null) ??
              content;
            await Promise.resolve(onUpdate?.());
          })
          .catch((error) => {
            setSaveError(
              getErrorMessage(
                error,
                t('pms.taskDetail.errors.saveDescriptionFailed'),
              ),
            );
          })
          .finally(() => {
            if (!saveTimerRef.current) setDescriptionSavePending(false);
          });
      }, 500);
    },
    [canEdit, issueState.id, onUpdate, api, t, workspaceSlug],
  );

  return {
    commitTitleChange,
    descriptionBlocksRef,
    descriptionSavePending,
    handleDescriptionChange,
    issueState,
    memberNamesForIds,
    patchField,
    persistIssueUpdate,
    saveError,
    selectedAssigneeIds,
    selectedFollowerIds,
    selectedLabelIds,
    setSaveError,
    setTitleDraft,
    statusLabel,
    titleDraft,
    toggleIssueUserRole,
  };
}
