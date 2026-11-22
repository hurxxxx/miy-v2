import { useCallback, useState } from 'react';

import {
  createTaskListLabel,
  deleteLabel,
  updateLabel,
  type PmsLabel,
} from '../api/pms-api';
import { LABEL_PRESET_COLORS } from './pms-color-palettes';
import { type AppApi } from '../../../platform/runtime/runtime-context';

export { LABEL_PRESET_COLORS } from './pms-color-palettes';

type LabelErrorMessages = {
  createFailed: string;
  deleteFailed: string;
  updateFailed: string;
};

type UseTaskListLabelsControllerParams = {
  workspaceSlug?: string | null;
  errorMessages: LabelErrorMessages;
  onError: (message: string | null) => void;
  onLabelsChanged?: (labels: PmsLabel[]) => void;
  taskListId: string;
  api: AppApi | null | undefined;
};

export function useTaskListLabelsController({
  workspaceSlug,
  errorMessages,
  onError,
  onLabelsChanged,
  taskListId,
  api,
}: UseTaskListLabelsControllerParams) {
  const [labels, setLabels] = useState<PmsLabel[]>([]);
  const [newName, setNewName] = useState('');
  const [newColor, setNewColor] = useState(LABEL_PRESET_COLORS[0]);
  const [creating, setCreating] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editColor, setEditColor] = useState('');

  const replaceLabels = useCallback(
    (updated: PmsLabel[]) => {
      setLabels(updated);
      onLabelsChanged?.(updated);
    },
    [onLabelsChanged],
  );

  const handleCreate = useCallback(async () => {
    if (!api || !newName.trim()) return;
    setCreating(true);
    onError(null);
    try {
      const label = await createTaskListLabel(
        api,
        taskListId,
        {
          name: newName.trim(),
          color: newColor,
        },
        workspaceSlug,
      );
      replaceLabels([...labels, label]);
      setNewName('');
      setNewColor(LABEL_PRESET_COLORS[0]);
    } catch (caughtError) {
      onError(
        caughtError instanceof Error
          ? caughtError.message
          : errorMessages.createFailed,
      );
    } finally {
      setCreating(false);
    }
  }, [
    errorMessages.createFailed,
    labels,
    newColor,
    newName,
    onError,
    replaceLabels,
    taskListId,
    api,
    workspaceSlug,
  ]);

  const startEdit = useCallback((label: PmsLabel) => {
    setEditingId(label.id);
    setEditName(label.name);
    setEditColor(label.color);
  }, []);

  const handleSaveEdit = useCallback(async () => {
    if (!api || !editingId || !editName.trim()) return;
    onError(null);
    try {
      const updatedLabel = await updateLabel(
        api,
        editingId,
        {
          name: editName.trim(),
          color: editColor,
        },
        workspaceSlug,
      );
      replaceLabels(
        labels.map((label) => (label.id === editingId ? updatedLabel : label)),
      );
      setEditingId(null);
    } catch (caughtError) {
      onError(
        caughtError instanceof Error
          ? caughtError.message
          : errorMessages.updateFailed,
      );
    }
  }, [
    editColor,
    editName,
    editingId,
    errorMessages.updateFailed,
    labels,
    onError,
    replaceLabels,
    api,
    workspaceSlug,
  ]);

  const handleDelete = useCallback(
    async (labelId: string) => {
      if (!api) return;
      onError(null);
      try {
        await deleteLabel(api, labelId, workspaceSlug);
        replaceLabels(labels.filter((label) => label.id !== labelId));
      } catch (caughtError) {
        onError(
          caughtError instanceof Error
            ? caughtError.message
            : errorMessages.deleteFailed,
        );
      }
    },
    [
      errorMessages.deleteFailed,
      labels,
      onError,
      replaceLabels,
      api,
      workspaceSlug,
    ],
  );

  return {
    creating,
    editColor,
    editName,
    editingId,
    handleCreate,
    handleDelete,
    handleSaveEdit,
    labels,
    newColor,
    newName,
    replaceLabels,
    setEditColor,
    setEditName,
    setEditingId,
    setNewColor,
    setNewName,
    startEdit,
  };
}
