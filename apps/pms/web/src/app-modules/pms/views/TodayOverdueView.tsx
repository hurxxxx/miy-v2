import { useCallback, useEffect, useRef, useState } from 'react';
import { buildAppRoutePath } from '../../../platform/core-web';
import { Link } from '../../../platform/router';
import {
  useAppRouteParams as useParams,
  useOptionalAppRuntime,
} from '../../../platform/app-runtime';
import { Calendar, AlertCircle, CheckCircle2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../../lib/utils';
import {
  useAppApi,
  useRuntime,
  type AppApi,
} from '../../../platform/runtime/runtime-context';
import {
  normalizeTimeZone,
  zonedDateKey,
} from '../../../platform/time/time-utils';
import {
  listAllPmsTaskLists,
  listAllTodayOverdueTasks,
  type PmsTask,
  type PmsTaskList,
} from '../api/pms-api';
import { formatDate } from './pms-constants';
import { PmsCenteredLoadingState } from './PmsCenteredStateBlock';
import { TaskAssigneeStack } from './TaskAssigneeStack';
import { buildPmsTaskContextLabel } from './pms-task-context';
import { buildPmsTaskListToolPath } from './pms-view-route';
import { buildTodayOverdueTaskGroups } from './today-overdue-model';
import { usePmsTaskListChangeSubscription } from './usePmsTaskListChangeSubscription';

type TodayOverdueTaskLoadResult = {
  taskLists: PmsTaskList[];
  tasks: PmsTask[];
};

export async function loadTodayOverdueTasks(
  api: AppApi,
  today: string,
  workspaceSlug?: string | null,
): Promise<TodayOverdueTaskLoadResult> {
  const [taskListResponse, taskResponse] = await Promise.all([
    listAllPmsTaskLists(api, undefined, workspaceSlug),
    listAllTodayOverdueTasks(api, today, workspaceSlug),
  ]);
  return {
    taskLists: taskListResponse.items,
    tasks: taskResponse.items,
  };
}

export const TodayOverdueView = ({
  workspaceSlug: workspaceSlugProp = null,
}: {
  workspaceSlug?: string | null;
}) => {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  // AI-DO read the signed-in user's profile time zone; miy uses the portal
  // display setting.
  const timeZoneSetting = useRuntime().settings.timeZone;
  const runtime = useOptionalAppRuntime();
  const { workspaceSlug: routeWorkspaceSlug } = useParams();
  const workspaceSlug = workspaceSlugProp ?? routeWorkspaceSlug ?? null;
  const [data, setData] = useState<TodayOverdueTaskLoadResult>({
    taskLists: [],
    tasks: [],
  });
  const [loading, setLoading] = useState(true);
  const loadGenerationRef = useRef(0);
  const today = zonedDateKey(new Date(), normalizeTimeZone(timeZoneSetting));

  const reloadTodayOverdueTasks = useCallback(async () => {
    if (!api) return;
    const loadGeneration = loadGenerationRef.current + 1;
    loadGenerationRef.current = loadGeneration;
    setLoading(true);
    try {
      const nextData = await loadTodayOverdueTasks(api, today, workspaceSlug);
      if (loadGeneration === loadGenerationRef.current) setData(nextData);
    } finally {
      if (loadGeneration === loadGenerationRef.current) setLoading(false);
    }
  }, [today, api, workspaceSlug]);

  useEffect(() => {
    void reloadTodayOverdueTasks();
  }, [reloadTodayOverdueTasks]);

  usePmsTaskListChangeSubscription(
    useCallback(() => {
      void reloadTodayOverdueTasks();
    }, [reloadTodayOverdueTasks]),
  );

  const { overdue, today: todayIssues } = buildTodayOverdueTaskGroups(
    data.tasks,
    today,
  );
  const fallbackSpaceName = t('pms.spaceOverview.fallbackSpaceName');
  const taskContextLabel = (task: PmsTask) =>
    buildPmsTaskContextLabel({
      fallbackSpaceName,
      task,
      taskLists: data.taskLists,
    });
  const taskPath = (task: PmsTask): string | null => {
    if (runtime) {
      const ownerSpaceKey = data.taskLists.find(
        (taskList) => taskList.id === task.list_id,
      )?.workspace_key;
      if (!ownerSpaceKey) return null;
      return buildAppRoutePath(
        { routeBase: runtime.app.route_base, spaceKey: ownerSpaceKey },
        `/lists/${encodeURIComponent(task.list_id)}?${new URLSearchParams({ task: task.id })}`,
      );
    }
    return buildPmsTaskListToolPath({
      taskId: task.id,
      taskListId: task.list_id,
      workspaceSlug,
    });
  };

  return (
    <div className="h-full flex flex-col relative">
      <header className="bg-app-bg border-b border-app-border px-8 pt-6 pb-4">
        <h1 className="app-text-title-lg text-app-ink">
          {t('pms.todayOverdue.title')}
        </h1>
        <p className="app-text-body mt-1 text-app-ink/55">
          {t('pms.todayOverdue.description')}
        </p>
      </header>

      <main className="flex-1 overflow-y-auto p-8 custom-scrollbar">
        {loading ? (
          <PmsCenteredLoadingState minHeightClassName="py-16" />
        ) : (
          <div className="max-w-4xl mx-auto space-y-8">
            {/* Overdue */}
            <section>
              <div className="flex items-center gap-2 mb-4 text-app-danger">
                <AlertCircle size={18} />
                <h2 className="app-text-title-md">
                  {t('pms.todayOverdue.overdue')}
                </h2>
                <span className="app-text-label rounded-full bg-app-danger/10 px-2 py-0.5 text-app-danger">
                  {overdue.length}
                </span>
              </div>
              <div className="space-y-2">
                {overdue.map((task) => (
                  <IssueAgendaItem
                    key={task.id}
                    contextLabel={taskContextLabel(task)}
                    isOverdue
                    task={task}
                    to={taskPath(task)}
                  />
                ))}
                {overdue.length === 0 && (
                  <p className="app-text-body text-app-ink/40">
                    {t('pms.todayOverdue.noOverdue')}
                  </p>
                )}
              </div>
            </section>

            {/* Today */}
            <section>
              <div className="flex items-center gap-2 mb-4 text-blue-400">
                <Calendar size={18} />
                <h2 className="app-text-title-md">
                  {t('pms.todayOverdue.today')}
                </h2>
                <span className="app-text-label rounded-full bg-blue-400/10 px-2 py-0.5 text-blue-400">
                  {todayIssues.length}
                </span>
              </div>
              <div className="space-y-2">
                {todayIssues.map((task) => (
                  <IssueAgendaItem
                    key={task.id}
                    contextLabel={taskContextLabel(task)}
                    task={task}
                    to={taskPath(task)}
                  />
                ))}
                {todayIssues.length === 0 && (
                  <p className="app-text-body text-app-ink/40">
                    {t('pms.todayOverdue.noToday')}
                  </p>
                )}
              </div>
            </section>
          </div>
        )}
      </main>
    </div>
  );
};

const IssueAgendaItem = ({
  contextLabel,
  task,
  to,
  isOverdue = false,
}: {
  contextLabel: string | null;
  task: PmsTask;
  to: string | null;
  isOverdue?: boolean;
}) => {
  const className =
    'group flex items-center justify-between rounded-lg border border-app-border bg-app-surface-sidebar p-4 transition-colors hover:border-app-border-strong';
  const content = (
    <>
      <div className="flex min-w-0 items-center gap-4">
        <span className="shrink-0 text-app-ink/55 transition-colors group-hover:text-app-success">
          <CheckCircle2 size={20} />
        </span>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="app-text-body truncate font-medium text-app-ink transition-colors group-hover:text-app-accent">
              {task.title}
            </h3>
          </div>
          <div className="app-text-caption mt-1 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-app-ink/55">
            <span
              className={cn(
                'flex shrink-0 items-center gap-1',
                isOverdue ? 'text-app-danger' : '',
              )}
            >
              <Calendar size={12} />
              {formatDate(task.due_date)}
            </span>
            {contextLabel ? (
              <span className="min-w-0 truncate">{contextLabel}</span>
            ) : null}
            {task.labels.length > 0 && (
              <span className="flex shrink-0 items-center gap-1">
                <span className="size-2 rounded-full bg-app-info" />
                {task.labels[0].name}
              </span>
            )}
          </div>
        </div>
      </div>
      <TaskAssigneeStack task={task} />
    </>
  );
  return to ? (
    <Link className={className} to={to}>
      {content}
    </Link>
  ) : (
    <div className={className}>{content}</div>
  );
};
