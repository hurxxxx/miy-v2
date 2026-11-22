import { useCallback, useEffect, useReducer, useState, type SetStateAction } from 'react';

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
  const [loadFailed, setLoadFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!api) return;
    let cancelled = false;
    dispatch({ type: 'loading', value: true });
    setLoadFailed(false);
    void Promise.allSettled([
      getTaskDetail(api, taskId, workspaceSlug),
      listTaskActivityLogs(api, taskId, workspaceSlug),
    ]).then(([detail, logs]) => {
      if (cancelled) return;
      if (detail.status === 'fulfilled') {
        dispatch({
          type: 'loaded',
          detail: detail.value,
          logs: logs.status === 'fulfilled' ? logs.value : undefined,
        });
      } else if (logs.status === 'fulfilled') {
        dispatch({ type: 'activityLogs', value: logs.value.items });
      }
      setLoadFailed(detail.status === 'rejected' || logs.status === 'rejected');
      dispatch({ type: 'loading', value: false });
    });
    return () => {
      cancelled = true;
    };
  }, [api, attempt, taskId, workspaceSlug]);

  return {
    state,
    loadFailed,
    retryLoad: () => setAttempt((current) => current + 1),
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
