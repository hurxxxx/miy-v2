import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from 'react';
import { AnimatePresence, LazyMotion, domAnimation } from 'motion/react';
import { Button } from '@miy/ui';
import { ChevronDown, FolderKanban, Layers, Layout } from 'lucide-react';
import { useSearchParams } from '../../../platform/router';
import { useTranslation } from 'react-i18next';

import { useAppApi } from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import { runRequestsWithConcurrency } from '../../../platform/network/request-concurrency';
import {
  DEFAULT_PMS_TASK_SORT,
  createTaskListTask,
  deleteTask,
  getTaskDetail,
  listAllPmsTaskLists,
  listAllTaskListTasks,
  listSpaceMembers,
  listTaskListLabels,
  listTaskListMilestones,
  listTaskListStatuses,
  reorderTaskListTasks,
  updateTask,
  type TaskFilterParams,
  type PmsLabel,
  type PmsMilestone,
  type PmsTask,
  type PmsTaskList,
  type PmsTaskListMember,
  type PmsTaskListStatus,
} from '../api/pms-api';
import {
  filterDefaultVisibleTasks,
  hasSelectedCompletionStatus,
  mergeTaskListStatusesForFilter,
  setCompletionStatusesVisible,
  withEffectiveTaskStatusFilter,
} from '../api/pms-filters';
import { taskListRoleAllows } from '../api/pms-permissions';
import { FilterBar } from './FilterBar';
import {
  createOpeningTaskFilterParams,
  spaceTasksFilterScopeKey,
} from './filter-bar-model';
import { getDefaultTaskStatus, getStatusSlugs } from './pms-constants';
import { BoardView } from './BoardView';
import { CalendarView } from './CalendarView';
import { GanttView } from './GanttView';
import { ListView } from './ListView';
import { TableView } from './TableView';
import {
  PmsCenteredLoadingState,
  PmsCenteredStateBlock,
} from './PmsCenteredStateBlock';
import { TaskDetail } from './TaskDetail';
import { TaskDetailModal } from './TaskDetailModal';
import {
  findPmsTaskInBundles,
  getRequestedPmsTaskId,
  resolvePmsTaskClosedTransition,
  resolvePmsTaskSelectedTransition,
  resolveRequestedPmsTaskTransition,
} from './pms-task-selection-workflow';
import { usePmsTaskListChangeSubscription } from './usePmsTaskListChangeSubscription';
import { PmsSpaceToolTabs } from './PmsSpaceToolTabs';
import {
  combineSpaceTasks,
  loadSpaceTasksLayout,
  saveSpaceTasksLayout,
  supportsCombinedSpaceLayout,
  type SpaceTasksLayout,
} from './space-tasks-layout-model';
import type { PmsTaskListToolTab } from './pms-view-route';
import type { TaskBoardPositionUpdate } from './pms-task-hierarchy';

type ListBundle = {
  labels: PmsLabel[];
  milestones: PmsMilestone[];
  tasks: PmsTask[];
  members: PmsTaskListMember[];
  statuses: PmsTaskListStatus[];
};

type SpaceTasksViewProps = {
  activeTab: PmsTaskListToolTab;
  spaceId: string;
  spaceName?: string | null;
  workspaceSlug?: string | null;
};

const PMS_SPACE_TASKS_SPACE_MEMBER_CONCURRENCY = 4;
const PMS_SPACE_TASKS_LIST_BUNDLE_CONCURRENCY = 2;

function getErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

function sortTaskLists(lists: PmsTaskList[], locale: string): PmsTaskList[] {
  const sorted = Array.from(lists);
  sorted.sort(
    (left, right) =>
      (left.team_name ?? '').localeCompare(right.team_name ?? '', locale) ||
      (left.folder_name ?? '').localeCompare(right.folder_name ?? '', locale) ||
      left.sort_order - right.sort_order ||
      left.name.localeCompare(right.name, locale),
  );
  return sorted;
}

function uniqueBy<T>(items: T[], keyOf: (item: T) => string): T[] {
  const seen = new Set<string>();
  const out: T[] = [];
  for (const item of items) {
    const key = keyOf(item);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  return out;
}

interface SpaceTasksState {
  taskLists: PmsTaskList[];
  bundles: Map<string, ListBundle>;
  selectedIssue: PmsTask | null;
  loading: boolean;
  error: string | null;
}

type SpaceTasksAction =
  | { type: 'load:start' }
  | {
      type: 'load:success';
      taskLists: PmsTaskList[];
      bundles: Map<string, ListBundle>;
    }
  | { type: 'load:fail'; message: string }
  | { type: 'selected:set'; task: PmsTask | null }
  | { type: 'selected:requested-loaded'; task: PmsTask | null }
  | {
      type: 'bundle:update-tasks';
      listId: string;
      updater: (tasks: PmsTask[]) => PmsTask[];
    };

const INITIAL_SPACE_TASKS_STATE: SpaceTasksState = {
  taskLists: [],
  bundles: new Map(),
  selectedIssue: null,
  loading: true,
  error: null,
};

function spaceTasksReducer(
  state: SpaceTasksState,
  action: SpaceTasksAction,
): SpaceTasksState {
  switch (action.type) {
    case 'load:start':
      return { ...state, loading: true, error: null };
    case 'load:success':
      return {
        ...state,
        taskLists: action.taskLists,
        bundles: action.bundles,
        selectedIssue: state.selectedIssue
          ? (findPmsTaskInBundles(action.bundles, state.selectedIssue.id) ??
            state.selectedIssue)
          : null,
        loading: false,
      };
    case 'load:fail':
      return {
        ...state,
        taskLists: [],
        bundles: new Map(),
        loading: false,
        error: action.message,
      };
    case 'selected:set':
      return { ...state, selectedIssue: action.task };
    case 'selected:requested-loaded':
      return { ...state, selectedIssue: action.task };
    case 'bundle:update-tasks': {
      const bundle = state.bundles.get(action.listId);
      if (!bundle) return state;
      const bundles = new Map(state.bundles);
      bundles.set(action.listId, {
        ...bundle,
        tasks: action.updater(bundle.tasks),
      });
      return { ...state, bundles };
    }
    default:
      return state;
  }
}

function useSpaceTasksViewElement({
  activeTab,
  spaceId,
  spaceName,
  workspaceSlug,
}: {
  activeTab: PmsTaskListToolTab;
  spaceId: string;
  spaceName?: string | null;
  workspaceSlug?: string | null;
}) {
  const { t, i18n } = useTranslation('apps');
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const api = useAppApi();
  const user = usePmsCurrentUser();
  const [searchParams, setSearchParams] = useSearchParams();
  const [filterParams, setFilterParams] = useState<TaskFilterParams>(() =>
    createOpeningTaskFilterParams(spaceTasksFilterScopeKey(spaceId)),
  );
  const [layout, setLayout] = useState<SpaceTasksLayout>(() =>
    loadSpaceTasksLayout(spaceId),
  );
  const changeLayout = useCallback(
    (next: SpaceTasksLayout) => {
      setLayout(next);
      saveSpaceTasksLayout(spaceId, next);
    },
    [spaceId],
  );
  const [state, dispatch] = useReducer(
    spaceTasksReducer,
    INITIAL_SPACE_TASKS_STATE,
  );
  const loadGenerationRef = useRef(0);
  const { taskLists, bundles, selectedIssue, loading, error } = state;
  const requestedTaskId = getRequestedPmsTaskId(searchParams);

  useEffect(() => {
    if (!requestedTaskId) {
      dispatch({ type: 'selected:set', task: null });
    }
  }, [requestedTaskId]);

  const selectedTaskList = useMemo(
    () =>
      selectedIssue
        ? (taskLists.find(
            (taskList) => taskList.id === selectedIssue.list_id,
          ) ?? null)
        : null,
    [selectedIssue, taskLists],
  );
  const selectedBundle = selectedIssue
    ? (bundles.get(selectedIssue.list_id) ?? null)
    : null;

  const reloadSpaceTasks = useCallback(async () => {
    if (!api) return;
    const loadGeneration = loadGenerationRef.current + 1;
    loadGenerationRef.current = loadGeneration;
    dispatch({ type: 'load:start' });
    try {
      const listResponse = await listAllPmsTaskLists(
        api,
        spaceId,
        workspaceSlug,
      );
      const sortedLists = sortTaskLists(
        listResponse.items.filter(
          (taskList) => !taskList.archived && taskList.team_id === spaceId,
        ),
        locale,
      );
      const uniqueSpaceIds = Array.from(
        new Set(
          sortedLists
            .map((taskList) => taskList.team_id)
            .filter((spaceId): spaceId is string => Boolean(spaceId)),
        ),
      );
      const memberEntries = await runRequestsWithConcurrency(
        uniqueSpaceIds,
        PMS_SPACE_TASKS_SPACE_MEMBER_CONCURRENCY,
        async (spaceId) => {
          try {
            const response = await listSpaceMembers(
              api,
              spaceId,
              workspaceSlug,
            );
            return [spaceId, response.items] as const;
          } catch {
            return [spaceId, [] as PmsTaskListMember[]] as const;
          }
        },
      );
      const membersBySpace = new Map(memberEntries);
      const bundleEntries = await runRequestsWithConcurrency(
        sortedLists,
        PMS_SPACE_TASKS_LIST_BUNDLE_CONCURRENCY,
        async (taskList) => {
          const statusRequest = listTaskListStatuses(
            api,
            taskList.id,
            workspaceSlug,
          ).catch(() => ({
            items: [] as PmsTaskListStatus[],
          }));
          const [
            issueResponse,
            statusResponse,
            labelResponse,
            milestoneResponse,
          ] = await Promise.all([
            statusRequest.then((response) =>
              listAllTaskListTasks(
                api,
                taskList.id,
                withEffectiveTaskStatusFilter(filterParams, response.items),
                workspaceSlug,
              ),
            ),
            statusRequest,
            listTaskListLabels(api, taskList.id, workspaceSlug).catch(() => ({
              items: [] as PmsLabel[],
            })),
            listTaskListMilestones(api, taskList.id, workspaceSlug).catch(
              () => ({
                items: [] as PmsMilestone[],
              }),
            ),
          ]);
          return [
            taskList.id,
            {
              labels: labelResponse.items,
              milestones: milestoneResponse.items,
              tasks: issueResponse.items,
              members: taskList.team_id
                ? (membersBySpace.get(taskList.team_id) ?? [])
                : [],
              statuses: statusResponse.items,
            },
          ] as const;
        },
      );
      if (loadGeneration !== loadGenerationRef.current) return;
      dispatch({
        type: 'load:success',
        taskLists: sortedLists,
        bundles: new Map(bundleEntries),
      });
    } catch (caughtError) {
      if (loadGeneration !== loadGenerationRef.current) return;
      dispatch({
        type: 'load:fail',
        message: getErrorMessage(caughtError, t('pms.errors.listDataFailed')),
      });
    }
  }, [filterParams, locale, spaceId, t, api, workspaceSlug]);

  useEffect(() => {
    void reloadSpaceTasks();
  }, [reloadSpaceTasks]);

  useEffect(() => {
    if (!api || !requestedTaskId || loading) {
      return;
    }
    const transition = resolveRequestedPmsTaskTransition({
      requestedTaskId,
      visibleTask: findPmsTaskInBundles(bundles, requestedTaskId),
    });
    if (transition.selectedTask) {
      dispatch({
        type: 'selected:requested-loaded',
        task: transition.selectedTask,
      });
      return;
    }
    if (!transition.detailRequest) {
      return;
    }

    let cancelled = false;
    getTaskDetail(api, transition.detailRequest.taskId, workspaceSlug)
      .then((detail) => {
        if (!cancelled) {
          dispatch({
            type: 'selected:requested-loaded',
            task: taskLists.some(
              (taskList) => taskList.id === detail.task.list_id,
            )
              ? detail.task
              : null,
          });
        }
      })
      .catch(() => {
        if (!cancelled)
          dispatch({ type: 'selected:requested-loaded', task: null });
      });

    return () => {
      cancelled = true;
    };
  }, [bundles, loading, requestedTaskId, taskLists, api, workspaceSlug]);

  const handleSelectIssue = useCallback(
    (task: PmsTask) => {
      const transition = resolvePmsTaskSelectedTransition({
        searchParams,
        task,
      });
      dispatch({ type: 'selected:set', task: transition.selectedTask });
      if (transition.searchParams) {
        setSearchParams(transition.searchParams, { replace: true });
      }
    },
    [searchParams, setSearchParams],
  );

  const handleClose = useCallback(() => {
    const transition = resolvePmsTaskClosedTransition<PmsTask>({
      searchParams,
    });
    dispatch({ type: 'selected:set', task: transition.selectedTask });
    if (transition.searchParams) {
      setSearchParams(transition.searchParams, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  usePmsTaskListChangeSubscription(
    useCallback(
      (detail) => {
        const changedListId =
          detail.type === 'updated' ? detail.taskList.id : detail.taskListId;
        const leftActiveCatalog =
          detail.type === 'deleted' || detail.taskList.archived;
        if (leftActiveCatalog && selectedIssue?.list_id === changedListId) {
          handleClose();
        }
        void reloadSpaceTasks();
      },
      [handleClose, reloadSpaceTasks, selectedIssue?.list_id],
    ),
  );

  const updateBundleIssues = useCallback(
    (listId: string, updater: (tasks: PmsTask[]) => PmsTask[]) => {
      dispatch({ type: 'bundle:update-tasks', listId, updater });
    },
    [],
  );

  const handleCreateIssue = useCallback(
    async (taskList: PmsTaskList, title: string, parentId: string | null) => {
      if (!api || !taskListRoleAllows(taskList.role, 'member')) return;
      const bundle = bundles.get(taskList.id);
      const created = await createTaskListTask(
        api,
        taskList.id,
        {
          title,
          description: '',
          status: getDefaultTaskStatus(bundle?.statuses),
          priority: 'medium',
          assignee_id: null,
          milestone_id: null,
          start_date: null,
          due_date: null,
          parent_id: parentId,
        },
        workspaceSlug,
      );
      updateBundleIssues(taskList.id, (tasks) => [...tasks, created]);
    },
    [bundles, api, updateBundleIssues, workspaceSlug],
  );

  const handleDeleteIssue = useCallback(
    async (taskList: PmsTaskList, taskId: string) => {
      if (!api || !taskListRoleAllows(taskList.role, 'member')) return;
      await deleteTask(api, taskId, workspaceSlug);
      updateBundleIssues(taskList.id, (tasks) =>
        tasks.filter((task) => task.id !== taskId),
      );
      if (selectedIssue?.id === taskId)
        dispatch({ type: 'selected:set', task: null });
    },
    [selectedIssue?.id, api, updateBundleIssues, workspaceSlug],
  );

  const handleUpdateIssue = useCallback(
    async (
      taskList: PmsTaskList,
      taskId: string,
      payload: Record<string, unknown>,
    ) => {
      if (!api || !taskListRoleAllows(taskList.role, 'member')) return;
      const bundle = bundles.get(taskList.id);
      if (!bundle?.tasks.some((task) => task.id === taskId)) return;
      if (
        typeof payload.status === 'string' &&
        !getStatusSlugs(bundle.statuses).includes(payload.status)
      ) {
        return;
      }
      const updated = await updateTask(api, taskId, payload, workspaceSlug);
      updateBundleIssues(taskList.id, (tasks) =>
        tasks.map((task) => (task.id === updated.id ? updated : task)),
      );
      if (selectedIssue?.id === updated.id)
        dispatch({ type: 'selected:set', task: updated });
    },
    [bundles, selectedIssue?.id, api, updateBundleIssues, workspaceSlug],
  );

  const handleReorderIssues = useCallback(
    async (taskList: PmsTaskList, updates: TaskBoardPositionUpdate[]) => {
      if (
        !api ||
        !taskListRoleAllows(taskList.role, 'member') ||
        updates.length === 0
      ) {
        return;
      }
      const bundle = bundles.get(taskList.id);
      if (
        !bundle ||
        updates.some(
          (update) => !bundle.tasks.some((task) => task.id === update.taskId),
        )
      ) {
        return;
      }
      const result = await reorderTaskListTasks(
        api,
        taskList.id,
        {
          items: updates.map((update) => {
            const item: {
              board_position: number;
              parent_id?: string | null;
              task_id: string;
            } = {
              board_position: update.boardPosition,
              task_id: update.taskId,
            };
            if (update.parentId !== undefined) item.parent_id = update.parentId;
            return item;
          }),
        },
        workspaceSlug,
      );
      if (result.items.length === 0) return;
      const updatedById = new Map(
        result.items.map((item) => [item.id, item] as const),
      );
      updateBundleIssues(taskList.id, (tasks) =>
        tasks.map((task) => updatedById.get(task.id) ?? task),
      );
      const updatedSelectedIssue = selectedIssue
        ? updatedById.get(selectedIssue.id)
        : undefined;
      if (updatedSelectedIssue) {
        dispatch({ type: 'selected:set', task: updatedSelectedIssue });
      }
    },
    [bundles, selectedIssue, api, updateBundleIssues, workspaceSlug],
  );

  const visibleTasksByListId = useMemo(
    () =>
      new Map(
        Array.from(bundles.entries()).map(([listId, bundle]) => [
          listId,
          filterDefaultVisibleTasks(bundle.tasks, bundle.statuses, {
            status: filterParams.status,
          }),
        ]),
      ),
    [bundles, filterParams.status],
  );
  const taskCount = useMemo(
    () =>
      Array.from(visibleTasksByListId.values()).reduce(
        (total, tasks) => total + tasks.length,
        0,
      ),
    [visibleTasksByListId],
  );
  const filterOptions = useMemo(() => {
    const allBundles = Array.from(bundles.values());
    return {
      labels: uniqueBy(
        allBundles.flatMap((bundle) => bundle.labels),
        (label) => label.id,
      ),
      members: uniqueBy(
        allBundles.flatMap((bundle) => bundle.members),
        (member) => member.user_id,
      ),
      milestones: uniqueBy(
        allBundles.flatMap((bundle) => bundle.milestones),
        (milestone) => milestone.id,
      ),
      statuses: mergeTaskListStatusesForFilter(
        allBundles.flatMap((bundle) => bundle.statuses),
      ),
    };
  }, [bundles]);
  // One space-wide filter, so every list's Gantt shows the same choice.
  const showCompletedItems = hasSelectedCompletionStatus(
    filterParams.status,
    filterOptions.statuses,
  );
  const handleShowCompletedItemsChange = useCallback(
    (showCompleted: boolean) => {
      setFilterParams((current) => ({
        ...current,
        status: setCompletionStatusesVisible(
          current.status,
          filterOptions.statuses,
          showCompleted,
        ),
      }));
    },
    [filterOptions.statuses],
  );

  const layoutChoiceShown =
    supportsCombinedSpaceLayout(activeTab) && taskLists.length > 1;
  const showCombined = layoutChoiceShown && layout === 'combined';
  // Per-list sections are drawn only when the lists are not merged into one chart.
  const sectionLists = showCombined ? [] : taskLists;
  const combinedTasks = useMemo(
    () => combineSpaceTasks(taskLists, visibleTasksByListId),
    [taskLists, visibleTasksByListId],
  );

  // Every list's tasks in one chart. A change still saves to the task's own list, and only
  // tasks of lists the viewer may edit can be dragged.
  const renderCombinedView = () => {
    const editableListIds = new Set(
      taskLists
        .filter((taskList) => taskListRoleAllows(taskList.role, 'member'))
        .map((taskList) => taskList.id),
    );
    const canEditTask = (task: PmsTask) =>
      editableListIds.has(combinedTasks.listByTaskId.get(task.id)?.id ?? '');
    const updateIssue = (taskId: string, payload: Record<string, unknown>) => {
      const taskList = combinedTasks.listByTaskId.get(taskId);
      if (!taskList || !editableListIds.has(taskList.id)) return;
      return handleUpdateIssue(taskList, taskId, payload);
    };
    const anyListHasTasks = taskLists.some(
      (taskList) => taskList.task_count > 0,
    );
    if (
      combinedTasks.tasks.length === 0 &&
      !(activeTab === 'gantt' && anyListHasTasks)
    ) {
      return (
        <p className="app-text-body-sm text-app-ink/40">
          {t('pms.allTasks.emptyList')}
        </p>
      );
    }
    if (activeTab === 'calendar') {
      return (
        <CalendarView
          tasks={combinedTasks.tasks}
          taskListStatuses={filterOptions.statuses}
          taskSourceLabels={combinedTasks.listNameByTaskId}
        />
      );
    }
    return (
      <GanttView
        canEdit={editableListIds.size > 0}
        canEditTask={canEditTask}
        completedItemsControlDisabled={filterOptions.statuses.length === 0}
        members={filterOptions.members}
        tasks={combinedTasks.tasks}
        onSelectIssue={handleSelectIssue}
        onShowCompletedItemsChange={handleShowCompletedItemsChange}
        onUpdateIssue={editableListIds.size > 0 ? updateIssue : undefined}
        showCompletedItems={showCompletedItems}
        taskListStatuses={filterOptions.statuses}
        taskSourceLabels={combinedTasks.listNameByTaskId}
      />
    );
  };

  const renderTaskListView = (
    taskList: PmsTaskList,
    bundle: ListBundle,
    visibleTasks: PmsTask[],
  ) => {
    const canEdit = taskListRoleAllows(taskList.role, 'member');
    const updateIssue = (taskId: string, payload: Record<string, unknown>) =>
      handleUpdateIssue(taskList, taskId, payload);

    // A Gantt whose tasks are all filtered out (say, all completed) still
    // renders: its toolbar holds the completed-items toggle that brings them
    // back. Only a list with no tasks at all gets the empty message.
    const keepFilteredGantt = activeTab === 'gantt' && taskList.task_count > 0;
    if (
      visibleTasks.length === 0 &&
      !keepFilteredGantt &&
      (activeTab !== 'list' || !canEdit)
    ) {
      return (
        <p className="app-text-body-sm text-app-ink/40">
          {t('pms.allTasks.emptyList')}
        </p>
      );
    }

    if (activeTab === 'board') {
      return (
        <BoardView
          tasks={visibleTasks}
          members={bundle.members}
          onSelectIssue={handleSelectIssue}
          onUpdateIssue={canEdit ? updateIssue : undefined}
          taskListStatuses={bundle.statuses}
        />
      );
    }
    if (activeTab === 'calendar') {
      return (
        <CalendarView tasks={visibleTasks} taskListStatuses={bundle.statuses} />
      );
    }
    if (activeTab === 'gantt') {
      return (
        <GanttView
          canEdit={canEdit}
          completedItemsControlDisabled={filterOptions.statuses.length === 0}
          members={bundle.members}
          tasks={visibleTasks}
          onSelectIssue={handleSelectIssue}
          onShowCompletedItemsChange={handleShowCompletedItemsChange}
          onUpdateIssue={canEdit ? updateIssue : undefined}
          showCompletedItems={showCompletedItems}
          taskListStatuses={bundle.statuses}
        />
      );
    }
    if (activeTab === 'table') {
      return (
        <TableView
          canEdit={canEdit}
          tasks={visibleTasks}
          members={bundle.members}
          onSelectIssue={handleSelectIssue}
          onUpdateIssue={canEdit ? updateIssue : undefined}
          taskListStatuses={bundle.statuses}
        />
      );
    }
    return (
      <ListView
        canEdit={canEdit}
        groupBy="none"
        tasks={visibleTasks}
        members={bundle.members}
        currentUserId={user?.id}
        onCreateIssue={(title, parentId) =>
          handleCreateIssue(taskList, title, parentId)
        }
        onDeleteIssue={(taskId) => handleDeleteIssue(taskList, taskId)}
        onReorderIssues={(updates) => handleReorderIssues(taskList, updates)}
        onSelectIssue={handleSelectIssue}
        onUpdateIssue={updateIssue}
        showToolbar={false}
        sort={DEFAULT_PMS_TASK_SORT}
        taskListStatuses={bundle.statuses}
      />
    );
  };

  return (
    <div className="relative flex h-full min-w-0 flex-col">
      <header className="border-b border-app-border bg-app-bg px-4 pt-3 lg:px-5">
        <div className="flex min-w-0 items-center gap-3">
          <div className="flex size-7 shrink-0 items-center justify-center rounded bg-app-accent text-app-accent-fg">
            <Layout size={16} />
          </div>
          <div className="min-w-0">
            <h1 className="app-text-title-md truncate text-app-ink">
              {spaceName ?? t('pms.spaceOverview.fallbackSpaceName')}
            </h1>
            <p className="app-text-caption truncate text-app-ink/45">
              {t('pms.allTasks.summary', {
                lists: taskLists.length,
                tasks: taskCount,
              })}
            </p>
          </div>
        </div>
        <PmsSpaceToolTabs
          activeTab={activeTab}
          className="mt-3"
          spaceId={spaceId}
          workspaceSlug={workspaceSlug}
        />
      </header>

      {taskLists.length > 0 ? (
        <FilterBar
          taskListId={spaceTasksFilterScopeKey(spaceId)}
          filterParams={filterParams}
          setFilterParams={setFilterParams}
          members={filterOptions.members}
          currentUserId={user?.id}
          milestones={filterOptions.milestones}
          labels={filterOptions.labels}
          taskListStatuses={filterOptions.statuses}
        />
      ) : null}

      <main className="flex-1 overflow-y-auto bg-app-bg p-4 custom-scrollbar lg:p-6">
        {loading ? (
          <PmsCenteredLoadingState minHeightClassName="py-16" />
        ) : error ? (
          <PmsCenteredStateBlock minHeightClassName="py-16" tone="danger">
            {error}
          </PmsCenteredStateBlock>
        ) : taskLists.length === 0 ? (
          <PmsCenteredStateBlock minHeightClassName="py-16">
            {t('pms.overviewPage.noLists')}
          </PmsCenteredStateBlock>
        ) : (
          <div className="space-y-4">
            {layoutChoiceShown ? (
              <div
                aria-label={t('pms.allTasks.layoutLabel')}
                className="flex items-center justify-end gap-1"
                role="group"
              >
                <span className="app-text-caption mr-1 text-app-ink/45">
                  {t('pms.allTasks.layoutLabel')}
                </span>
                {(['by-list', 'combined'] as const).map((option) => (
                  <Button
                    key={option}
                    aria-pressed={layout === option}
                    onClick={() => changeLayout(option)}
                    size="dense"
                    variant={layout === option ? 'subtle' : 'ghost'}
                  >
                    {t(
                      option === 'combined'
                        ? 'pms.allTasks.layoutCombined'
                        : 'pms.allTasks.layoutByList',
                    )}
                  </Button>
                ))}
              </div>
            ) : null}
            {showCombined ? (
              <section className="overflow-hidden rounded-lg border border-app-border bg-app-surface">
                <div className="flex min-w-0 items-center gap-3 border-b border-app-border px-4 py-3">
                  <Layers size={16} className="shrink-0 text-app-accent" />
                  <h2 className="app-text-title-sm min-w-0 flex-1 truncate text-app-ink">
                    {t('pms.allTasks.combinedTitle', {
                      count: taskLists.length,
                    })}
                  </h2>
                  <span className="app-text-caption shrink-0 text-app-ink/45">
                    {t('pms.spaceOverview.taskCount', {
                      count: combinedTasks.tasks.length,
                    })}
                  </span>
                </div>
                <div className="p-4">{renderCombinedView()}</div>
              </section>
            ) : null}
            {sectionLists.map((taskList) => {
              const bundle = bundles.get(taskList.id) ?? {
                labels: [],
                milestones: [],
                tasks: [],
                members: [],
                statuses: [],
              };
              const visibleTasks = visibleTasksByListId.get(taskList.id) ?? [];
              return (
                <section
                  key={taskList.id}
                  className="overflow-hidden rounded-lg border border-app-border bg-app-surface"
                >
                  <div className="flex min-w-0 items-center gap-3 border-b border-app-border px-4 py-3">
                    <ChevronDown
                      size={14}
                      className="shrink-0 text-app-ink/45"
                    />
                    <FolderKanban
                      size={16}
                      className="shrink-0 text-app-accent"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="app-text-caption truncate text-app-ink/45">
                        {taskList.folder_name
                          ? t('pms.allTasks.listLocationWithFolder', {
                              folder: taskList.folder_name,
                              space:
                                taskList.team_name ??
                                t('pms.spaceOverview.fallbackSpaceName'),
                            })
                          : t('pms.allTasks.listLocation', {
                              space:
                                taskList.team_name ??
                                t('pms.spaceOverview.fallbackSpaceName'),
                            })}
                      </p>
                      <h2 className="app-text-title-sm truncate text-app-ink">
                        {taskList.name}
                      </h2>
                    </div>
                    <span className="app-text-caption shrink-0 text-app-ink/45">
                      {t('pms.spaceOverview.taskCount', {
                        count: visibleTasks.length,
                      })}
                    </span>
                  </div>
                  <div className="p-4">
                    {renderTaskListView(taskList, bundle, visibleTasks)}
                  </div>
                </section>
              );
            })}
          </div>
        )}
      </main>

      <LazyMotion features={domAnimation}>
        <AnimatePresence>
          {selectedIssue ? (
            <TaskDetailModal
              title={selectedIssue.title}
              onClose={handleClose}
            >
              <TaskDetail
                canEdit={taskListRoleAllows(selectedTaskList?.role, 'member')}
                members={selectedBundle?.members ?? []}
                task={selectedIssue}
                taskListStatuses={selectedBundle?.statuses ?? []}
                onClose={handleClose}
                onUpdate={reloadSpaceTasks}
                spaceName={selectedTaskList?.team_name}
                workspaceSlug={workspaceSlug}
              />
            </TaskDetailModal>
          ) : null}
        </AnimatePresence>
      </LazyMotion>
    </div>
  );
}

function SpaceTasksViewSession(props: SpaceTasksViewProps) {
  return useSpaceTasksViewElement(props);
}

export function SpaceTasksView(props: SpaceTasksViewProps) {
  const sessionKey = `${props.workspaceSlug ?? ''}:${props.spaceId}`;
  return <SpaceTasksViewSession key={sessionKey} {...props} />;
}
