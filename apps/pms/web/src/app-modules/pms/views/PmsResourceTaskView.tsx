import { useEffect, useState } from 'react';
import { useParams } from '../../../platform/router';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import { AppRuntimeNotice } from '../../../platform/app-runtime/AppRuntimeNotice';
import { getPmsTaskList, getTaskDetail, type PmsTask } from '../api/pms-api';
import { TaskDetail } from './TaskDetail';
import { buildPmsTaskListToolPath } from './pms-view-route';

/**
 * `/tasks/:taskId`: where notifications and the home card point (miy).
 *
 * AI-DO had two locators: the list route with `?task=` and the single-task
 * share route `/resources/pms/:spaceKey/tasks/:taskId` (this view). Here the
 * task is read first; a person who can open its list goes on to the list with
 * the task open (replacing this address), and a person who can read only this
 * task (a specific share) sees AI-DO's read-only single-task view.
 *
 * Specific task shares are read-only and never load the list or space directory.
 */
export function PmsResourceTaskView() {
  const { api, navigate } = useRuntime();
  const { taskId } = useParams();
  const [result, setResult] = useState<{
    taskId: string;
    task: PmsTask | null;
  } | null>(null);
  useEffect(() => {
    if (!taskId) return;
    let cancelled = false;
    getTaskDetail(api, taskId)
      .then(async (detail) => {
        if (cancelled) return;
        const listId = detail.task.list_id;
        const canOpenList = await getPmsTaskList(api, listId).then(
          () => true,
          () => false,
        );
        if (cancelled) return;
        if (canOpenList) {
          navigate(
            buildPmsTaskListToolPath({ taskId: detail.task.id, taskListId: listId }),
            { replace: true },
          );
          return;
        }
        setResult({ taskId, task: detail.task });
      })
      .catch(() => {
        if (!cancelled) setResult({ taskId, task: null });
      });
    return () => {
      cancelled = true;
    };
  }, [api, navigate, taskId]);
  const current = result?.taskId === taskId ? result : null;
  if (!taskId) return <AppRuntimeNotice status="space_denied" />;
  if (!current) return <AppRuntimeNotice status="loading" />;
  if (!current.task) return <AppRuntimeNotice status="space_denied" />;
  return (
    <TaskDetail task={current.task} canEdit={false} onClose={() => navigate('/')} />
  );
}
