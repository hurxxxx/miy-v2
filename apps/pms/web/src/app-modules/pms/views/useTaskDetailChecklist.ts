import { useCallback, useState, type SetStateAction } from 'react';
import type { TFunction } from 'i18next';

import {
  createChecklistItem,
  deleteChecklistItem,
  updateChecklistItem,
  type PmsChecklistItem,
} from '../api/pms-api';
import {
  getTaskDetailMutationErrorMessage,
  notifyTaskDetailUpdated,
} from './task-detail-mutation';
import { type AppApi } from '../../../platform/runtime/runtime-context';

type CreateChecklistItemPayload = Parameters<typeof createChecklistItem>[2];

export function buildTaskDetailChecklistItemPayload({
  sortOrder,
  text,
}: {
  sortOrder: number;
  text: string;
}): CreateChecklistItemPayload {
  return {
    sort_order: sortOrder,
    text: text.trim(),
  };
}

export function getTaskDetailChecklistProgress(
  checklistItems: Pick<PmsChecklistItem, 'completed'>[],
): { done: number; total: number } {
  return {
    done: checklistItems.filter((item) => item.completed).length,
    total: checklistItems.length,
  };
}

export function useTaskDetailChecklist({
  workspaceSlug,
  canEdit,
  checklistItems,
  onUpdate,
  setChecklistItems,
  setSaveError,
  taskId,
  api,
  t,
}: {
  workspaceSlug?: string | null;
  canEdit: boolean;
  checklistItems: PmsChecklistItem[];
  onUpdate?: () => void | Promise<void>;
  setChecklistItems: (value: SetStateAction<PmsChecklistItem[]>) => void;
  setSaveError: (message: string | null) => void;
  taskId: string;
  api: AppApi | null;
  t: TFunction;
}) {
  const [newChecklistText, setNewChecklistText] = useState('');
  const [addingChecklist, setAddingChecklist] = useState(false);
  const [editingChecklistId, setEditingChecklistId] = useState<string | null>(
    null,
  );
  const [editingChecklistText, setEditingChecklistText] = useState('');
  const checklistProgress = getTaskDetailChecklistProgress(checklistItems);

  const handleAddChecklistItem = useCallback(async () => {
    if (!api || !canEdit || !newChecklistText.trim()) return;
    setAddingChecklist(true);
    setSaveError(null);
    try {
      const item = await createChecklistItem(
        api,
        taskId,
        buildTaskDetailChecklistItemPayload({
          sortOrder: checklistItems.length,
          text: newChecklistText,
        }),
        workspaceSlug,
      );
      setChecklistItems((prev) => [...prev, item]);
      setNewChecklistText('');
      await notifyTaskDetailUpdated(onUpdate);
    } catch (error) {
      setSaveError(
        getTaskDetailMutationErrorMessage(
          error,
          t('pms.taskDetail.errors.addChecklistFailed'),
        ),
      );
    } finally {
      setAddingChecklist(false);
    }
  }, [
    canEdit,
    checklistItems.length,
    newChecklistText,
    onUpdate,
    setChecklistItems,
    setSaveError,
    t,
    taskId,
    api,
    workspaceSlug,
  ]);

  const handleToggleChecklistItem = useCallback(
    async (item: PmsChecklistItem) => {
      if (!api || !canEdit) return;
      const newCompleted = !item.completed;
      setChecklistItems((prev) =>
        prev.map((checklistItem) =>
          checklistItem.id === item.id
            ? { ...checklistItem, completed: newCompleted }
            : checklistItem,
        ),
      );
      try {
        await updateChecklistItem(
          api,
          item.id,
          { completed: newCompleted },
          workspaceSlug,
        );
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setChecklistItems((prev) =>
          prev.map((checklistItem) =>
            checklistItem.id === item.id
              ? { ...checklistItem, completed: !newCompleted }
              : checklistItem,
          ),
        );
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.updateChecklistFailed'),
          ),
        );
      }
    },
    [
      canEdit,
      onUpdate,
      setChecklistItems,
      setSaveError,
      t,
      api,
      workspaceSlug,
    ],
  );

  const handleSaveChecklistEdit = useCallback(
    async (itemId: string) => {
      if (!api || !canEdit || !editingChecklistText.trim()) return;
      try {
        await updateChecklistItem(
          api,
          itemId,
          {
            text: editingChecklistText.trim(),
          },
          workspaceSlug,
        );
        setChecklistItems((prev) =>
          prev.map((checklistItem) =>
            checklistItem.id === itemId
              ? { ...checklistItem, text: editingChecklistText.trim() }
              : checklistItem,
          ),
        );
        setEditingChecklistId(null);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.editChecklistFailed'),
          ),
        );
      }
    },
    [
      canEdit,
      editingChecklistText,
      setChecklistItems,
      setSaveError,
      t,
      api,
      workspaceSlug,
    ],
  );

  const handleDeleteChecklistItem = useCallback(
    async (itemId: string) => {
      if (!api || !canEdit) return;
      try {
        await deleteChecklistItem(api, itemId, workspaceSlug);
        setChecklistItems((prev) =>
          prev.filter((checklistItem) => checklistItem.id !== itemId),
        );
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.deleteChecklistFailed'),
          ),
        );
      }
    },
    [
      canEdit,
      onUpdate,
      setChecklistItems,
      setSaveError,
      t,
      api,
      workspaceSlug,
    ],
  );

  return {
    addingChecklist,
    checklistDone: checklistProgress.done,
    checklistTotal: checklistProgress.total,
    editingChecklistId,
    editingChecklistText,
    handleAddChecklistItem,
    handleDeleteChecklistItem,
    handleSaveChecklistEdit,
    handleToggleChecklistItem,
    newChecklistText,
    setEditingChecklistId,
    setEditingChecklistText,
    setNewChecklistText,
  };
}
