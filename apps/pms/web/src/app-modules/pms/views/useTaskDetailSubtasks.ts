import { useCallback, useState, type SetStateAction } from 'react';
import type { TFunction } from 'i18next';

import {
  createTaskListTask,
  deleteTask,
  updateTask,
  type PmsTask,
  type PmsTaskListStatus,
} from '../api/pms-api';
import { getDefaultTaskStatus } from './pms-constants';
import {
  getTaskDetailMutationErrorMessage,
  notifyTaskDetailUpdated,
} from './task-detail-mutation';
import { type AppApi } from '../../../platform/runtime/runtime-context';

type CreateSubtaskPayload = Parameters<typeof createTaskListTask>[2];

export function buildTaskDetailSubtaskPayload({
  parentTask,
  taskListStatuses,
  title,
}: {
  parentTask: Pick<PmsTask, 'id'>;
  taskListStatuses?: PmsTaskListStatus[];
  title: string;
}): CreateSubtaskPayload {
  return {
    assignee_id: null,
    description: '',
    due_date: null,
    milestone_id: null,
    parent_id: parentTask.id,
    priority: 'medium',
    start_date: null,
    status: getDefaultTaskStatus(taskListStatuses),
    title: title.trim(),
  };
}

export function useTaskDetailSubtasks({
  workspaceSlug,
  canEdit,
  onUpdate,
  parentTask,
  setSaveError,
  setSubtasks,
  taskListStatuses,
  api,
  t,
}: {
  workspaceSlug?: string | null;
  canEdit: boolean;
  onUpdate?: () => void | Promise<void>;
  parentTask: Pick<PmsTask, 'id' | 'list_id'>;
  setSaveError: (message: string | null) => void;
  setSubtasks: (value: SetStateAction<PmsTask[]>) => void;
  taskListStatuses?: PmsTaskListStatus[];
  api: AppApi | null;
  t: TFunction;
}) {
  const [newSubtaskTitle, setNewSubtaskTitle] = useState('');
  const [addingSubtask, setAddingSubtask] = useState(false);
  const [subtaskMenuOpen, setSubtaskMenuOpen] = useState<string | null>(null);

  const handleUnlinkSubtask = useCallback(
    async (subtaskId: string) => {
      if (!api || !canEdit) return;
      setSaveError(null);
      try {
        await updateTask(api, subtaskId, { parent_id: null }, workspaceSlug);
        setSubtasks((prev) =>
          prev.filter((subtask) => subtask.id !== subtaskId),
        );
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.unlinkSubtaskFailed'),
          ),
        );
      }
    },
    [canEdit, onUpdate, setSaveError, setSubtasks, t, api, workspaceSlug],
  );

  const handleArchiveSubtask = useCallback(
    async (subtaskId: string) => {
      if (!api || !canEdit) return;
      setSaveError(null);
      try {
        await updateTask(api, subtaskId, { archived: true }, workspaceSlug);
        setSubtasks((prev) =>
          prev.filter((subtask) => subtask.id !== subtaskId),
        );
        setSubtaskMenuOpen(null);
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.archiveSubtaskFailed'),
          ),
        );
      }
    },
    [canEdit, onUpdate, setSaveError, setSubtasks, t, api, workspaceSlug],
  );

  const handleDeleteSubtask = useCallback(
    async (subtaskId: string) => {
      if (!api || !canEdit) return;
      setSaveError(null);
      try {
        await deleteTask(api, subtaskId, workspaceSlug);
        setSubtasks((prev) =>
          prev.filter((subtask) => subtask.id !== subtaskId),
        );
        setSubtaskMenuOpen(null);
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.deleteSubtaskFailed'),
          ),
        );
      }
    },
    [canEdit, onUpdate, setSaveError, setSubtasks, t, api, workspaceSlug],
  );

  const handleAddSubtask = useCallback(async () => {
    if (!api || !canEdit || !newSubtaskTitle.trim()) return;
    setAddingSubtask(true);
    setSaveError(null);
    try {
      const subtask = await createTaskListTask(
        api,
        parentTask.list_id,
        buildTaskDetailSubtaskPayload({
          parentTask,
          taskListStatuses,
          title: newSubtaskTitle,
        }),
        workspaceSlug,
      );
      setSubtasks((prev) => [...prev, subtask]);
      setNewSubtaskTitle('');
      await notifyTaskDetailUpdated(onUpdate);
    } catch (error) {
      setSaveError(
        getTaskDetailMutationErrorMessage(
          error,
          t('pms.taskDetail.errors.createSubtaskFailed'),
        ),
      );
    } finally {
      setAddingSubtask(false);
    }
  }, [
    canEdit,
    newSubtaskTitle,
    onUpdate,
    parentTask,
    setSaveError,
    setSubtasks,
    t,
    taskListStatuses,
    api,
    workspaceSlug,
  ]);

  return {
    addingSubtask,
    handleAddSubtask,
    handleArchiveSubtask,
    handleDeleteSubtask,
    handleUnlinkSubtask,
    newSubtaskTitle,
    setNewSubtaskTitle,
    setSubtaskMenuOpen,
    subtaskMenuOpen,
  };
}
