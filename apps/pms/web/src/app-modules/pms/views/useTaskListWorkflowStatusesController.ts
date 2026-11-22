import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  createSpaceStatus,
  createTaskListStatus,
  listTaskListStatuses,
  updateSpaceStatus,
  updateTaskListStatus,
  updateTaskListStatusMode,
  type PmsStatusCategory,
  type PmsTaskListStatus,
  type PmsTaskListStatusesResponse,
} from '../api/pms-api';
import {
  buildCreateWorkflowStatusCommand,
  buildUpdateWorkflowStatusCommand,
  createTaskListWorkflowSettingsModel,
  normalizeWorkflowStatusMode,
  normalizeWorkflowStatusSource,
  type TaskListWorkflowStatusMode,
  type TaskListWorkflowStatusSource,
} from './task-list-workflow-settings-model';
import { PMS_WORKFLOW_DEFAULT_STATUS_COLOR } from './pms-color-palettes';
import { type AppApi } from '../../../platform/runtime/runtime-context';

export interface TaskListWorkflowStatusesClient {
  createSpaceStatus: typeof createSpaceStatus;
  createTaskListStatus: typeof createTaskListStatus;
  listTaskListStatuses: typeof listTaskListStatuses;
  updateSpaceStatus: typeof updateSpaceStatus;
  updateTaskListStatus: typeof updateTaskListStatus;
  updateTaskListStatusMode: typeof updateTaskListStatusMode;
}

export interface TaskListWorkflowStatusesMessages {
  createFailed: string;
  loadFailed: string;
  updateFailed: string;
  updateModeFailed: string;
}

export interface UseTaskListWorkflowStatusesControllerParams {
  workspaceSlug?: string | null;
  client?: TaskListWorkflowStatusesClient;
  currentUserRole: string | null;
  messages: TaskListWorkflowStatusesMessages;
  onError: (message: string | null) => void;
  onLoadError: (message: string | null) => void;
  onStatusesChanged?: (statuses: PmsTaskListStatus[]) => void;
  taskListId: string;
  teamId: string | null;
  api: AppApi | null | undefined;
}

export const taskListWorkflowStatusesClient: TaskListWorkflowStatusesClient = {
  createSpaceStatus,
  createTaskListStatus,
  listTaskListStatuses,
  updateSpaceStatus,
  updateTaskListStatus,
  updateTaskListStatusMode,
};

export function useTaskListWorkflowStatusesController({
  workspaceSlug,
  client = taskListWorkflowStatusesClient,
  currentUserRole,
  messages,
  onError,
  onLoadError,
  onStatusesChanged,
  taskListId,
  teamId,
  api,
}: UseTaskListWorkflowStatusesControllerParams) {
  const [statuses, setStatuses] = useState<PmsTaskListStatus[]>([]);
  const [statusMode, setStatusMode] =
    useState<TaskListWorkflowStatusMode>('custom');
  const [statusSource, setStatusSource] =
    useState<TaskListWorkflowStatusSource>('list');
  const [loadingStatuses, setLoadingStatuses] = useState(true);
  const [newStatusName, setNewStatusName] = useState('');
  const [newStatusColor, setNewStatusColor] = useState(
    PMS_WORKFLOW_DEFAULT_STATUS_COLOR,
  );
  const [newStatusCategory, setNewStatusCategory] =
    useState<PmsStatusCategory>('active');
  const [creatingStatus, setCreatingStatus] = useState(false);
  const [editingStatusId, setEditingStatusId] = useState<string | null>(null);
  const [editStatusName, setEditStatusName] = useState('');
  const [editStatusColor, setEditStatusColor] = useState('');
  const [editStatusCategory, setEditStatusCategory] =
    useState<PmsStatusCategory>('active');

  // The PMS screen stores the reported statuses and rebuilds this callback from them.
  // Reading it through a ref keeps that new identity from re-running the load effect,
  // which otherwise reloaded statuses and every task in a loop while settings stayed open.
  const onStatusesChangedRef = useRef(onStatusesChanged);
  onStatusesChangedRef.current = onStatusesChanged;

  const replaceStatuses = useCallback(
    (response: PmsTaskListStatusesResponse) => {
      setStatuses(response.items);
      setStatusMode(normalizeWorkflowStatusMode(response.mode));
      setStatusSource(normalizeWorkflowStatusSource(response.source));
      onStatusesChangedRef.current?.(response.items);
    },
    [],
  );

  const reloadStatuses = useCallback(async () => {
    if (!api) {
      setLoadingStatuses(false);
      return;
    }
    setLoadingStatuses(true);
    onLoadError(null);
    try {
      replaceStatuses(
        await client.listTaskListStatuses(api, taskListId, workspaceSlug),
      );
    } catch (caughtError) {
      onLoadError(errorMessage(caughtError, messages.loadFailed));
    } finally {
      setLoadingStatuses(false);
    }
  }, [
    client,
    messages.loadFailed,
    onLoadError,
    replaceStatuses,
    taskListId,
    api,
    workspaceSlug,
  ]);

  useEffect(() => {
    void reloadStatuses();
  }, [reloadStatuses]);

  const handleStatusModeChange = useCallback(
    async (mode: TaskListWorkflowStatusMode) => {
      if (!api || mode === statusMode) return;
      onError(null);
      try {
        const result = await client.updateTaskListStatusMode(
          api,
          taskListId,
          mode,
          workspaceSlug,
        );
        replaceStatuses(result);
      } catch (caughtError) {
        onError(errorMessage(caughtError, messages.updateModeFailed));
      }
    },
    [
      client,
      messages.updateModeFailed,
      onError,
      replaceStatuses,
      statusMode,
      taskListId,
      api,
      workspaceSlug,
    ],
  );

  const handleCreateStatus = useCallback(async () => {
    if (!api) return;
    const command = buildCreateWorkflowStatusCommand({
      category: newStatusCategory,
      color: newStatusColor,
      name: newStatusName,
      sortOrder: statuses.length,
      statusSource,
      taskListId,
      teamId,
    });
    if (command.kind === 'noop') return;
    setCreatingStatus(true);
    onError(null);
    try {
      const created =
        command.kind === 'space'
          ? await client.createSpaceStatus(
              api,
              command.spaceId,
              command.payload,
              workspaceSlug,
            )
          : await client.createTaskListStatus(
              api,
              command.taskListId,
              command.payload,
              workspaceSlug,
            );
      const updated = [...statuses, created];
      setStatuses(updated);
      onStatusesChanged?.(updated);
      setNewStatusName('');
      setNewStatusColor(PMS_WORKFLOW_DEFAULT_STATUS_COLOR);
      setNewStatusCategory('active');
    } catch (caughtError) {
      onError(errorMessage(caughtError, messages.createFailed));
    } finally {
      setCreatingStatus(false);
    }
  }, [
    client,
    messages.createFailed,
    newStatusCategory,
    newStatusColor,
    newStatusName,
    onError,
    onStatusesChanged,
    statusSource,
    statuses,
    taskListId,
    teamId,
    api,
    workspaceSlug,
  ]);

  const startEditStatus = useCallback((status: PmsTaskListStatus) => {
    setEditingStatusId(status.id);
    setEditStatusName(status.name);
    setEditStatusColor(status.color);
    setEditStatusCategory(status.category);
  }, []);

  const handleSaveEditStatus = useCallback(async () => {
    if (!api) return;
    const command = buildUpdateWorkflowStatusCommand({
      category: editStatusCategory,
      color: editStatusColor,
      name: editStatusName,
      statusId: editingStatusId,
      statusSource,
    });
    if (command.kind === 'noop') return;
    onError(null);
    try {
      const updatedStatus =
        command.kind === 'space'
          ? await client.updateSpaceStatus(
              api,
              command.statusId,
              command.payload,
              workspaceSlug,
            )
          : await client.updateTaskListStatus(
              api,
              command.statusId,
              command.payload,
              workspaceSlug,
            );
      const updated = statuses.map((status) =>
        status.id === command.statusId ? updatedStatus : status,
      );
      setStatuses(updated);
      onStatusesChanged?.(updated);
      setEditingStatusId(null);
    } catch (caughtError) {
      onError(errorMessage(caughtError, messages.updateFailed));
    }
  }, [
    client,
    editStatusCategory,
    editStatusColor,
    editStatusName,
    editingStatusId,
    messages.updateFailed,
    onError,
    onStatusesChanged,
    statusSource,
    statuses,
    api,
    workspaceSlug,
  ]);

  const workflowSettings = useMemo(
    () =>
      createTaskListWorkflowSettingsModel({
        currentUserRole,
        statusMode,
        statusSource,
        statuses,
      }),
    [currentUserRole, statusMode, statusSource, statuses],
  );

  return {
    canEditWorkflow: workflowSettings.canEditWorkflow,
    creatingStatus,
    editStatusCategory,
    editStatusColor,
    editStatusName,
    editingStatusId,
    handleCreateStatus,
    handleSaveEditStatus,
    handleStatusModeChange,
    loadingStatuses,
    newStatusCategory,
    newStatusColor,
    newStatusName,
    reloadStatuses,
    setEditStatusCategory,
    setEditStatusColor,
    setEditStatusName,
    setEditingStatusId,
    setNewStatusCategory,
    setNewStatusColor,
    setNewStatusName,
    startEditStatus,
    statuses,
    statusesByCategory: workflowSettings.statusesByCategory,
    statusMode,
    statusSource,
    workflowReadOnly: workflowSettings.workflowReadOnly,
  };
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}
