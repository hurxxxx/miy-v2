import { useCallback, useEffect, useReducer, type SetStateAction } from 'react';

import {
  getTaskDetail,
  listTaskActivityLogs,
  type PmsAttachment,
  type PmsChecklistItem,
  type PmsComment,
  type PmsTask,
} from '../api/pms-api';
import {
  INITIAL_TASK_DETAIL_RESOURCES_STATE,
  taskDetailResourcesReducer,
} from './task-detail-resources-model';
import { type AppApi } from '../../../platform/runtime/runtime-context';
export {
  INITIAL_TASK_DETAIL_RESOURCES_STATE,
  taskDetailResourcesReducer,
  taskDetailResourcesFromLoaded,
} from './task-detail-resources-model';
export type {
  TaskDetailResourcesAction,
  TaskDetailResourcesState,
} from './task-detail-resources-model';

export function useTaskDetailResources({
  taskId,
  api,
  workspaceSlug,
}: {
  taskId: string;
  api: AppApi | null;
  workspaceSlug: string | null;
}) {
  const [state, dispatch] = useReducer(
    taskDetailResourcesReducer,
    INITIAL_TASK_DETAIL_RESOURCES_STATE,
  );

  useEffect(() => {
    if (!api) return;
    dispatch({ type: 'loading', value: true });
    Promise.all([
      getTaskDetail(api, taskId, workspaceSlug),
      listTaskActivityLogs(api, taskId, workspaceSlug),
    ])
      .then(([detail, logs]) => dispatch({ type: 'loaded', detail, logs }))
      .finally(() => dispatch({ type: 'loading', value: false }));
  }, [api, taskId, workspaceSlug]);

  return {
    state,
    setAttachments: useCallback(
      (value: SetStateAction<PmsAttachment[]>) =>
        dispatch({ type: 'attachments', value }),
      [],
    ),
    setChecklistItems: useCallback(
      (value: SetStateAction<PmsChecklistItem[]>) =>
        dispatch({ type: 'checklistItems', value }),
      [],
    ),
    setComments: useCallback(
      (value: SetStateAction<PmsComment[]>) =>
        dispatch({ type: 'comments', value }),
      [],
    ),
    setSubtasks: useCallback(
      (value: SetStateAction<PmsTask[]>) =>
        dispatch({ type: 'subtasks', value }),
      [],
    ),
  };
}
