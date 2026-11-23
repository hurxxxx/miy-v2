import { LinkedResources } from './LinkedResources';
import {
  type ReactNode,
  type SetStateAction,
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from 'react';
import {
  Link,
  useSearchParams,
  useNavigate,
  useLocation,
} from '../../../platform/router';
import { i18n } from '../../../platform/i18n';
import {
  useAppRouteParams as useParams,
  useOptionalAppRuntime,
} from '../../../platform/app-runtime';
import { useTranslation } from 'react-i18next';
import { LazyMotion, domAnimation, m, AnimatePresence } from 'motion/react';
import { useConfirm } from '@miy/ui';
import { usePrompt } from '@miy/ui';
import { useFeedback } from '@miy/ui';
import { buildAppRoutePath } from '../../../platform/core-web';
import {
  Archive,
  ArchiveRestore,
  Layout,
  Star,
  Plus,
  List as ListIcon,
  Grid,
  Calendar,
  Activity,
  Table,
  Download,
  MoreHorizontal,
} from 'lucide-react';
import { cn } from '../../../lib/utils';
import {
  useAppApi,
  useAppApiBlob,
} from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import {
  buildWorkspaceAppPath,
  getWorkspaceBySlug,
  getCurrentWorkspaceSlug,
  resolveDefaultWorkspaceAppPath,
} from '../../../platform/workspaces/workspace-utils';
import {
  getPmsTaskList,
  listAllPmsTaskLists,
  listSpaces,
  listSpaceMembers,
  listAllTaskListTasks,
  listTaskListMilestones,
  listTaskListLabels,
  listTaskListStatuses,
  listCustomFields,
  setTaskCustomFieldValue,
  getTaskDetail,
  createTaskListTask,
  deletePmsTaskList,
  deleteTask,
  reorderTaskListTasks,
  reorderTaskListTasksByTitle,
  updateTask,
  updatePmsTaskList,
  exportTaskListXlsx,
  DEFAULT_PMS_TASK_SORT,
  type TaskFilterParams,
  type PmsTaskList,
  type PmsTask,
  type PmsTaskListMember,
  type PmsSpace,
  type PmsMilestone,
  type PmsLabel,
  type PmsTaskListStatus,
  type PmsTaskSort,
  type PmsTaskSortDirection,
  type PmsCustomField,
  type PmsSavedFilter,
} from '../api/pms-api';
import {
  createDefaultTaskFilterParams,
  filterDefaultVisibleTasks,
  hasSelectedCompletionStatus,
  reconcileSelectedTaskIds,
  setCompletionStatusesVisible,
  withEffectiveTaskStatusFilter,
} from '../api/pms-filters';

import { SpaceTasksView } from './SpaceTasksView';
import { ListView } from './ListView';
import { BoardView } from './BoardView';
import { CalendarView } from './CalendarView';
import { GanttView } from './GanttView';
import { TableView } from './TableView';
import { TaskDetail } from './TaskDetail';
import { TaskDetailModal } from './TaskDetailModal';
import { NewTaskModal } from './NewTaskModal';
import { AssignedToMeView } from './AssignedToMeView';
import { TodayOverdueView } from './TodayOverdueView';
import { TaskListSettingsPanel } from './TaskListSettingsPanel';
import { CreateSpaceModal } from './CreateSpaceModal';
import { FilterBar } from './FilterBar';
import { BulkActionBar } from './BulkActionBar';
import { setTasksSelected } from './bulk-action-bar-model';
import {
  createLoadedSavedTaskFilterParams,
  taskExportFilterParams,
} from './filter-bar-model';
import { usePmsSavedFilters } from './usePmsSavedFilters';
import { apiErrorMessage } from '../../../platform/api/api-error';
import { SpaceOverviewView } from './SpaceOverviewView';
import { PmsGlobalOverview } from './PmsGlobalOverview';
import { ListContextMenu } from '../sidebar/ListContextMenu';
import {
  PmsCenteredLoadingState,
  PmsCenteredStateBlock,
} from './PmsCenteredStateBlock';
import {
  EMPTY_SELECTED_TASK_IDS,
  buildInlineTaskCreatePayload,
  createDefaultTaskFilterParamsForList,
  resolveScopedTaskFilterState,
  resolveScopedTaskSelectionState,
  selectScopedTaskFilterParams,
  selectScopedTaskSelectionIds,
  type ScopedTaskFilterState,
  type ScopedTaskSelectionState,
} from './pms-view-scoped-state-model';
import {
  buildPmsSpaceToolPath,
  clearPmsCreateTaskSearchParams,
  readPmsCreateTaskRequest,
  resolvePmsViewRoute,
} from './pms-view-route';
import {
  findPmsTaskById,
  getRequestedPmsTaskId,
  resolvePmsTaskClosedTransition,
  resolvePmsTaskSelectedTransition,
  resolveReloadedPmsTaskSelection,
  resolveRequestedPmsTaskTransition,
  resolveSingleListPmsTaskSelection,
  resolveSingleListPmsTaskDetailTransition,
} from './pms-task-selection-workflow';
import type { TaskBoardPositionUpdate } from './pms-task-hierarchy';
import { taskListRoleAllows } from '../api/pms-permissions';
import {
  PMS_SPACE_CHANGED_EVENT,
  PMS_SPACE_MEMBERS_CHANGED_EVENT,
  PMS_TASK_LIST_CHANGED_EVENT,
  dispatchPmsTaskListChanged,
  type PmsSpaceChangedDetail,
  type PmsSpaceMembersChangedDetail,
  type PmsTaskListChangedDetail,
} from './pms-events';
import { usePmsTaskListGroupPreference } from './usePmsTaskListGroupPreference';
import { reconcilePmsTaskListCatalog } from './pms-task-list-catalog-model';

type PmsViewTab = 'List' | 'Board' | 'Calendar' | 'Gantt' | 'Table';

const PMS_VIEW_TAB_LABEL_KEYS: Record<PmsViewTab, string> = {
  List: 'pms.viewTabs.list',
  Board: 'pms.viewTabs.board',
  Calendar: 'pms.viewTabs.calendar',
  Gantt: 'pms.viewTabs.gantt',
  Table: 'pms.viewTabs.table',
};

const PMS_VIEW_TABS = [
  'List',
  'Board',
  'Calendar',
  'Gantt',
  'Table',
] as const satisfies readonly PmsViewTab[];

const PMS_MOBILE_PRIMARY_TABS = [
  'List',
  'Board',
  'Calendar',
] as const satisfies readonly PmsViewTab[];

const PMS_MOBILE_MORE_TABS = [
  'Gantt',
  'Table',
] as const satisfies readonly PmsViewTab[];

const PMS_VIEW_TAB_QUERY: Record<string, PmsViewTab> = {
  board: 'Board',
  calendar: 'Calendar',
  gantt: 'Gantt',
  list: 'List',
  table: 'Table',
};

function isSameListCollection(
  left: PmsTaskList[],
  right: PmsTaskList[],
): boolean {
  return (
    left.length === right.length &&
    left.every((item, index) => {
      const other = right[index];
      return (
        item?.id === other?.id &&
        item?.updated_at === other?.updated_at &&
        item?.archived === other?.archived &&
        item?.name === other?.name &&
        item?.team_id === other?.team_id &&
        item?.folder_id === other?.folder_id
      );
    })
  );
}

function resolveLoadedSpaceName(
  spaces: PmsSpace[],
  taskLists: PmsTaskList[],
  spaceId: string,
): string | null {
  return (
    spaces.find((space) => space.id === spaceId)?.name ??
    taskLists.find((taskList) => taskList.team_id === spaceId)?.team_name ??
    null
  );
}

export const PMSView = () => <>{usePMSViewElement()}</>;

function usePMSViewElement(): ReactNode {
  const { t } = useTranslation('apps');
  const { toolId } = useParams();
  const runtime = useOptionalAppRuntime();
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const api = useAppApi();
  const apiBlob = useAppApiBlob();
  const user = usePmsCurrentUser();
  const { confirm, confirmDialog } = useConfirm();
  const { prompt, promptDialog } = usePrompt();
  const feedback = useFeedback();
  const pmsRoot =
    runtime?.href() ?? resolveDefaultWorkspaceAppPath(user, 'pms');
  const currentWorkspaceSlug =
    runtime?.space.key ??
    getWorkspaceBySlug(user, searchParams.get('workspace'))?.slug ??
    getCurrentWorkspaceSlug();
  const pmsListRootPath =
    !runtime && currentWorkspaceSlug
      ? buildWorkspaceAppPath(currentWorkspaceSlug, 'pms')
      : pmsRoot;
  const [selectedIssueDraft, setSelectedIssueDraft] = useState<PmsTask | null>(
    null,
  );
  const [isNewTaskModalOpen, setIsNewTaskModalOpen] = useState(false);
  const [newTaskModalKey, setNewTaskModalKey] = useState(0);
  const [newTaskContext, setNewTaskContext] = useState<{
    initialTitle: string;
    sourceTodoId: string | null;
  }>({ initialTitle: '', sourceTodoId: null });

  const [taskLists, setTaskLists] = useState<PmsTaskList[]>([]);
  const [spaces, setSpaces] = useState<PmsSpace[]>([]);
  const [selectedTaskListId, setSelectedTaskListId] = useState<string>('');
  const [loadedRouteListId, setLoadedRouteListId] = useState<string | null>(null);
  const [tasks, setIssues] = useState<PmsTask[]>([]);
  const [members, setMembers] = useState<PmsTaskListMember[]>([]);
  const [milestones, setMilestones] = useState<PmsMilestone[]>([]);
  const [labels, setLabels] = useState<PmsLabel[]>([]);
  // miy: the list's custom fields (table columns, task detail, filters).
  const [customFields, setCustomFields] = useState<PmsCustomField[]>([]);
  const [taskListStatusState, setTaskListStatusState] = useState<{
    taskListId: string;
    items: PmsTaskListStatus[];
  }>({ taskListId: '', items: [] });
  const [taskSort, setTaskSort] = useState<PmsTaskSort>(DEFAULT_PMS_TASK_SORT);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filterParamsState, setFilterParamsState] =
    useState<ScopedTaskFilterState>(() => ({
      taskListId: '',
      params: createDefaultTaskFilterParams(),
    }));
  const [selectedTaskIdsState, setSelectedTaskIdsState] =
    useState<ScopedTaskSelectionState>({
      taskListId: '',
      ids: EMPTY_SELECTED_TASK_IDS,
    });
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [createSpaceOpen, setCreateSpaceOpen] = useState(false);
  const [taskListMenuOpen, setTaskListMenuOpen] = useState(false);
  const [mobileMoreOpen, setMobileMoreOpen] = useState(false);

  const taskListMenuButtonRef = useRef<HTMLButtonElement>(null);
  const mobileMoreRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!mobileMoreOpen) return;
    function onClick(e: MouseEvent) {
      if (
        mobileMoreRef.current &&
        !mobileMoreRef.current.contains(e.target as Node)
      ) {
        setMobileMoreOpen(false);
      }
    }
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, [mobileMoreOpen]);

  const createTaskRequested = searchParams.get('create') === '1';
  const createTaskRequest = useMemo(
    () => readPmsCreateTaskRequest(searchParams),
    [searchParams],
  );
  const pmsRoute = resolvePmsViewRoute({
    runtimeSpaceId:
      runtime?.space.kind === 'app_space' ? runtime.space.id : null,
    createTaskRequested,
    isNewTaskModalOpen,
    requestedTab: searchParams.get('tab'),
    routePathname: location.pathname,
    toolId,
  });
  const isAssignedTasksView = pmsRoute.kind === 'assigned';
  const isTodayView = pmsRoute.kind === 'today';
  const routeTaskListId = pmsRoute.kind === 'list' ? pmsRoute.taskListId : null;
  const routeListPending =
    routeTaskListId !== null && loadedRouteListId !== routeTaskListId;
  const spaceOverviewId =
    pmsRoute.kind === 'spaceOverview' ? pmsRoute.spaceId : null;
  const spaceTasksSpaceId =
    pmsRoute.kind === 'spaceTasks' ? pmsRoute.spaceId : null;
  const spaceTasksTab = pmsRoute.kind === 'spaceTasks' ? pmsRoute.tab : null;
  const isOverviewRoute = pmsRoute.kind === 'overview';
  // miy: `/` is always the global overview (see the overview branch below).
  const isGlobalOverview = isOverviewRoute;
  const userId = user?.id;
  useEffect(() => {
    if (!isGlobalOverview || !userId) return;
    const handleSpaceCreated = (event: Event) => {
      const detail = (event as CustomEvent<PmsSpaceChangedDetail>).detail;
      if (detail?.type !== 'created' || detail.creatorId !== userId) return;
      setSpaces((current) => [
        detail.space,
        ...current.filter((space) => space.id !== detail.space.id),
      ]);
    };
    window.addEventListener(PMS_SPACE_CHANGED_EVENT, handleSpaceCreated);
    return () => {
      window.removeEventListener(PMS_SPACE_CHANGED_EVENT, handleSpaceCreated);
    };
  }, [isGlobalOverview, userId]);
  const shouldLoadSingleTaskList =
    pmsRoute.kind === 'list' || pmsRoute.kind === 'taskCreateFallback';
  const selectedTaskList = taskLists.find(
    (taskList) => taskList.id === selectedTaskListId,
  );
  const selectedIssue = resolveSingleListPmsTaskSelection({
    selectedTaskListId,
    task: selectedIssueDraft,
  });
  const taskListName = selectedTaskList?.name || 'List';
  const isArchivedTaskList = selectedTaskList?.archived === true;
  const canEditTaskList =
    !isArchivedTaskList && taskListRoleAllows(selectedTaskList?.role, 'member');
  const canManageTaskList = taskListRoleAllows(selectedTaskList?.role, 'admin');
  const createTaskContextRef = useRef({
    canEditTaskList: false,
    selectedTaskListId: '',
  });
  createTaskContextRef.current = { canEditTaskList, selectedTaskListId };
  const requestedTaskId = getRequestedPmsTaskId(searchParams);
  const requestedTab = searchParams.get('tab');
  const taskListSettingsRequested = searchParams.get('settings') === '1';
  const activeTab = PMS_VIEW_TAB_QUERY[requestedTab ?? ''] ?? 'List';
  const { groupBy: taskListGroupBy, setGroupBy: setTaskListGroupBy } =
    usePmsTaskListGroupPreference({
      enabled: pmsRoute.kind === 'list' && activeTab === 'List',
      workspaceSlug: currentWorkspaceSlug,
    });
  const defaultFilterParams = useMemo(
    () => createDefaultTaskFilterParamsForList(selectedTaskListId),
    [selectedTaskListId],
  );
  const filterParams = selectScopedTaskFilterParams(
    filterParamsState,
    selectedTaskListId,
    defaultFilterParams,
  );
  const taskListStatuses = useMemo(
    () =>
      taskListStatusState.taskListId === selectedTaskListId
        ? taskListStatusState.items
        : [],
    [selectedTaskListId, taskListStatusState],
  );
  const taskListStatusesReady =
    taskListStatusState.taskListId === selectedTaskListId;
  const exportFilter = taskExportFilterParams(filterParams, taskListStatuses);
  const setFilterParams = useCallback(
    (next: SetStateAction<TaskFilterParams>) => {
      setFilterParamsState((current) =>
        resolveScopedTaskFilterState(
          current,
          selectedTaskListId,
          next,
          defaultFilterParams,
        ),
      );
    },
    [defaultFilterParams, selectedTaskListId],
  );
  // miy: saved filters on the server. The "open with" filter arrives after
  // the list opens and applies only while the person has not changed this
  // list's filters yet (a filter state scoped to another list means untouched).
  const filterTouchedRef = useRef({ filterListId: '', selectedTaskListId: '' });
  filterTouchedRef.current = {
    filterListId: filterParamsState.taskListId,
    selectedTaskListId,
  };
  const applySavedFilter = useCallback(
    (filter: PmsSavedFilter) => {
      setFilterParams((current) =>
        createLoadedSavedTaskFilterParams(filter.params, current),
      );
      if (filter.sort) setTaskSort(filter.sort);
      if (filter.group_by && filter.group_by !== taskListGroupBy) {
        setTaskListGroupBy(filter.group_by);
      }
    },
    [setFilterParams, setTaskListGroupBy, taskListGroupBy],
  );
  const applySavedFilterRef = useRef(applySavedFilter);
  applySavedFilterRef.current = applySavedFilter;
  const savedFilters = usePmsSavedFilters({
    scope:
      pmsRoute.kind === 'list' && selectedTaskListId
        ? { listId: selectedTaskListId }
        : null,
    onOpeningDefault: (filter, scopeKey) => {
      const current = filterTouchedRef.current;
      if (scopeKey !== `list:${current.selectedTaskListId}`) return;
      if (current.filterListId === current.selectedTaskListId) return;
      applySavedFilterRef.current(filter);
    },
  });
  const captureSavedFilter = useCallback(
    () => ({
      sort: taskSort,
      group_by: activeTab === 'List' ? taskListGroupBy : null,
    }),
    [activeTab, taskListGroupBy, taskSort],
  );

  const selectedTaskIds = selectScopedTaskSelectionIds(
    selectedTaskIdsState,
    selectedTaskListId,
  );
  const setSelectedTaskIds = useCallback(
    (next: SetStateAction<Set<string>>) => {
      setSelectedTaskIdsState((current) =>
        resolveScopedTaskSelectionState(current, selectedTaskListId, next),
      );
    },
    [selectedTaskListId],
  );
  const visibleTasks = useMemo(
    () =>
      filterDefaultVisibleTasks(tasks, taskListStatuses, {
        status: filterParams.status,
      }),
    [filterParams.status, taskListStatuses, tasks],
  );
  const showCompletedItems = hasSelectedCompletionStatus(
    filterParams.status,
    taskListStatuses,
  );
  const handleShowCompletedItemsChange = useCallback(
    (showCompleted: boolean) => {
      setFilterParams((current) => ({
        ...current,
        status: setCompletionStatusesVisible(
          current.status,
          taskListStatuses,
          showCompleted,
        ),
      }));
    },
    [setFilterParams, taskListStatuses],
  );
  useEffect(() => {
    setSelectedTaskIds((current) =>
      reconcileSelectedTaskIds(current, visibleTasks),
    );
  }, [setSelectedTaskIds, visibleTasks]);
  const mobileMoreActive = PMS_MOBILE_MORE_TABS.includes(
    activeTab as (typeof PMS_MOBILE_MORE_TABS)[number],
  );

  const openNewTaskModal = useCallback(
    ({
      initialTitle = '',
      sourceTodoId = null,
    }: {
      initialTitle?: string | null;
      sourceTodoId?: string | null;
    } = {}) => {
      setNewTaskContext({
        initialTitle: initialTitle?.trim() ?? '',
        sourceTodoId,
      });
      setNewTaskModalKey((current) => current + 1);
      setIsNewTaskModalOpen(true);
    },
    [],
  );

  const closeNewTaskModal = useCallback(() => {
    setIsNewTaskModalOpen(false);
    setNewTaskContext({ initialTitle: '', sourceTodoId: null });
  }, []);

  const selectViewTab = (tab: typeof activeTab) => {
    setMobileMoreOpen(false);
    const nextParams = new URLSearchParams(searchParams);
    if (tab === 'List') {
      nextParams.delete('tab');
    } else {
      nextParams.set('tab', tab.toLowerCase());
    }
    setSearchParams(nextParams, { replace: true });
  };

  useEffect(() => {
    if (!api || !selectedTaskList?.team_id) return;
    let cancelled = false;
    const selectedSpaceId = selectedTaskList.team_id;

    const reloadSpaceMembers = async () => {
      try {
        const response = await listSpaceMembers(
          api,
          selectedSpaceId,
          currentWorkspaceSlug,
        );
        if (!cancelled) {
          setMembers(response.items);
        }
      } catch {
        // Keep the current assignee candidates if a cross-panel refresh fails.
      }
    };

    const handleSpaceMembersChanged = (event: Event) => {
      const detail = (event as CustomEvent<PmsSpaceMembersChangedDetail>)
        .detail;
      if (detail?.spaceId !== selectedSpaceId) return;
      void reloadSpaceMembers();
    };

    window.addEventListener(
      PMS_SPACE_MEMBERS_CHANGED_EVENT,
      handleSpaceMembersChanged,
    );
    return () => {
      cancelled = true;
      window.removeEventListener(
        PMS_SPACE_MEMBERS_CHANGED_EVENT,
        handleSpaceMembersChanged,
      );
    };
  }, [currentWorkspaceSlug, selectedTaskList?.team_id, api]);

  useEffect(() => {
    if (
      !taskListSettingsRequested ||
      !selectedTaskListId ||
      loading ||
      isArchivedTaskList
    )
      return;
    setSettingsOpen(true);
    const nextParams = new URLSearchParams(searchParams);
    nextParams.delete('settings');
    setSearchParams(nextParams, { replace: true });
  }, [
    loading,
    searchParams,
    isArchivedTaskList,
    selectedTaskListId,
    setSearchParams,
    taskListSettingsRequested,
  ]);

  const getErrorMessage = useCallback(
    (error: unknown, fallback: string) =>
      error instanceof Error ? error.message : fallback,
    [],
  );

  const applyIssueCollection = useCallback(
    (nextIssues: PmsTask[]) => {
      setIssues(nextIssues);
      setSelectedTaskIds((current) =>
        reconcileSelectedTaskIds(current, nextIssues),
      );
      setSelectedIssueDraft((current) => {
        return resolveReloadedPmsTaskSelection({
          currentTask: current,
          missingPolicy: 'preserve',
          tasks: nextIssues,
        });
      });
    },
    [setSelectedTaskIds],
  );

  const handleSelectIssue = useCallback(
    (task: PmsTask) => {
      const transition = resolvePmsTaskSelectedTransition({
        searchParams,
        task,
      });
      setSelectedIssueDraft(transition.selectedTask);
      if (transition.searchParams) {
        setSearchParams(transition.searchParams, { replace: true });
      }
    },
    [searchParams, setSearchParams],
  );

  const clearSelectedIssue = useCallback(() => {
    setSelectedIssueDraft(null);
    if (!requestedTaskId) {
      return;
    }

    const transition = resolvePmsTaskClosedTransition<PmsTask>({
      searchParams,
    });
    setSelectedIssueDraft(transition.selectedTask);
    if (transition.searchParams) {
      setSearchParams(transition.searchParams, { replace: true });
    }
  }, [requestedTaskId, searchParams, setSearchParams]);

  useEffect(() => {
    const handleTaskListChanged = (event: Event) => {
      const detail = (event as CustomEvent<PmsTaskListChangedDetail>).detail;
      if (!detail) return;
      if (detail.type === 'updated') {
        setTaskLists((current) =>
          reconcilePmsTaskListCatalog(current, detail.taskList, 'active'),
        );
        return;
      }

      setTaskLists((current) =>
        current.filter((taskList) => taskList.id !== detail.taskListId),
      );
      if (selectedTaskListId === detail.taskListId) {
        clearSelectedIssue();
        setSelectedTaskListId('');
        navigate(pmsListRootPath);
      }
    };

    window.addEventListener(PMS_TASK_LIST_CHANGED_EVENT, handleTaskListChanged);
    return () => {
      window.removeEventListener(
        PMS_TASK_LIST_CHANGED_EVENT,
        handleTaskListChanged,
      );
    };
  }, [clearSelectedIssue, navigate, pmsListRootPath, selectedTaskListId]);

  const handleRenameSelectedTaskList = useCallback(async () => {
    if (
      !api ||
      !selectedTaskList ||
      !canManageTaskList ||
      selectedTaskList.archived
    )
      return;
    const newName = await prompt({
      title: t('pms.sidebar.renameList'),
      defaultValue: selectedTaskList.name,
      placeholder: t('pms.listName'),
      submitLabel: t('common:actions.save'),
      cancelLabel: t('common:actions.cancel'),
    });
    const trimmedName = newName?.trim() ?? '';
    if (!trimmedName || trimmedName === selectedTaskList.name) return;
    setError(null);
    try {
      const updated = await updatePmsTaskList(
        api,
        selectedTaskList.id,
        {
          name: trimmedName,
        },
        currentWorkspaceSlug,
      );
      setTaskLists((current) =>
        reconcilePmsTaskListCatalog(current, updated, 'active'),
      );
      dispatchPmsTaskListChanged({ type: 'updated', taskList: updated });
    } catch (error) {
      setError(getErrorMessage(error, t('pms.sidebar.renameListFailed')));
    }
  }, [
    canManageTaskList,
    getErrorMessage,
    prompt,
    selectedTaskList,
    t,
    api,
    currentWorkspaceSlug,
  ]);

  const handleArchiveSelectedTaskList = useCallback(async () => {
    if (
      !api ||
      !selectedTaskList ||
      !canManageTaskList ||
      selectedTaskList.archived
    )
      return;
    const confirmed = await confirm({
      title: t('pms.archive.confirmTitle'),
      description: t('pms.archive.confirmDescription', {
        name: selectedTaskList.name,
      }),
      confirmLabel: t('pms.archive.action'),
      cancelLabel: t('common:actions.cancel'),
    });
    if (!confirmed) return;

    setError(null);
    try {
      const updated = await updatePmsTaskList(
        api,
        selectedTaskList.id,
        {
          archived: true,
        },
        currentWorkspaceSlug,
      );
      setTaskLists((current) =>
        reconcilePmsTaskListCatalog(current, updated, 'active'),
      );
      dispatchPmsTaskListChanged({ type: 'updated', taskList: updated });
      feedback.success(
        t('pms.archive.archivedFeedback', { name: selectedTaskList.name }),
      );
      clearSelectedIssue();
      setSelectedTaskListId('');
      navigate(
        updated.team_id
          ? buildPmsSpaceToolPath(updated.team_id, {
              runtime,
              workspaceSlug: currentWorkspaceSlug,
            })
          : pmsListRootPath,
      );
    } catch (caughtError) {
      feedback.error(
        getErrorMessage(caughtError, t('pms.archive.archiveFailed')),
      );
    }
  }, [
    canManageTaskList,
    clearSelectedIssue,
    confirm,
    currentWorkspaceSlug,
    getErrorMessage,
    navigate,
    pmsListRootPath,
    runtime,
    selectedTaskList,
    t,
    feedback,
    api,
  ]);

  const handleRestoreSelectedTaskList = useCallback(async () => {
    if (
      !api ||
      !selectedTaskList ||
      !canManageTaskList ||
      !selectedTaskList.archived
    )
      return;

    setError(null);
    try {
      const updated = await updatePmsTaskList(
        api,
        selectedTaskList.id,
        {
          archived: false,
        },
        currentWorkspaceSlug,
      );
      setTaskLists((current) =>
        reconcilePmsTaskListCatalog(current, updated, 'active'),
      );
      dispatchPmsTaskListChanged({ type: 'updated', taskList: updated });
      feedback.success(
        t('pms.archive.restoredFeedback', { name: selectedTaskList.name }),
      );
    } catch (caughtError) {
      feedback.error(
        getErrorMessage(caughtError, t('pms.archive.restoreFailed')),
      );
    }
  }, [
    canManageTaskList,
    feedback,
    getErrorMessage,
    selectedTaskList,
    t,
    api,
    currentWorkspaceSlug,
  ]);

  const handleDeleteSelectedTaskList = useCallback(async () => {
    if (
      !api ||
      !selectedTaskList ||
      !canManageTaskList ||
      !selectedTaskList.archived
    )
      return;
    const confirmed = await confirm({
      title: t('pms.sidebar.deleteList'),
      description: t('pms.sidebar.deleteListDescription', {
        name: selectedTaskList.name,
      }),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!confirmed) return;
    setError(null);
    try {
      await deletePmsTaskList(api, selectedTaskList.id, currentWorkspaceSlug);
      setTaskLists((current) =>
        current.filter((taskList) => taskList.id !== selectedTaskList.id),
      );
      dispatchPmsTaskListChanged({
        type: 'deleted',
        taskListId: selectedTaskList.id,
      });
      clearSelectedIssue();
      setSelectedTaskListId('');
      navigate(
        selectedTaskList.team_id
          ? buildPmsSpaceToolPath(selectedTaskList.team_id, {
              runtime,
              workspaceSlug: currentWorkspaceSlug,
            })
          : pmsListRootPath,
      );
    } catch (error) {
      setError(getErrorMessage(error, t('pms.sidebar.deleteListFailed')));
    }
  }, [
    canManageTaskList,
    clearSelectedIssue,
    confirm,
    currentWorkspaceSlug,
    getErrorMessage,
    navigate,
    pmsListRootPath,
    runtime,
    selectedTaskList,
    t,
    api,
  ]);

  const usesSpaceOverview = spaceOverviewId !== null;
  const selectDefaultList = routeTaskListId === null && createTaskRequested;
  // The space overview loads the selected space in its own view.
  useEffect(() => {
    if (!api || usesSpaceOverview) return;
    const activeApi = api;
    let cancelled = false;

    async function loadTaskLists() {
      setLoading(true);
      setError(null);

      try {
        if (routeTaskListId !== null) {
          const taskList = await getPmsTaskList(
            activeApi,
            routeTaskListId,
            currentWorkspaceSlug,
          );
          if (cancelled) return;
          setTaskLists((current) =>
            isSameListCollection(current, [taskList]) ? current : [taskList],
          );
          setSelectedTaskListId(routeTaskListId);
          return;
        }

        const [response, spaceItems] = await Promise.all([
          listAllPmsTaskLists(activeApi, undefined, currentWorkspaceSlug),
          listSpaces(activeApi, currentWorkspaceSlug),
        ]);
        if (cancelled) {
          return;
        }

        setSpaces(spaceItems);
        const nextTaskLists = response.items;
        setTaskLists((current) =>
          isSameListCollection(current, nextTaskLists)
            ? current
            : nextTaskLists,
        );
        setSelectedTaskListId((current) => {
          if (!shouldLoadSingleTaskList) {
            return '';
          }
          if (selectDefaultList) {
            if (
              current &&
              nextTaskLists.some(
                (taskList) =>
                  taskList.id === current &&
                  taskListRoleAllows(taskList.role, 'member'),
              )
            ) {
              return current;
            }
            return (
              nextTaskLists.find((taskList) =>
                taskListRoleAllows(taskList.role, 'member'),
              )?.id ?? ''
            );
          }
          if (
            current &&
            nextTaskLists.some((taskList) => taskList.id === current)
          ) {
            return current;
          }
          return nextTaskLists[0]?.id || '';
        });
      } catch (err) {
        if (cancelled) {
          return;
        }
        setSelectedTaskListId('');
        setError(
          getErrorMessage(err, i18n.t('pms.errors.listLoadFailed', { ns: 'apps' })),
        );
      } finally {
        if (!cancelled) {
          setLoadedRouteListId(routeTaskListId);
          setLoading(false);
        }
      }
    }

    void loadTaskLists();

    return () => {
      cancelled = true;
    };
  }, [
    currentWorkspaceSlug,
    getErrorMessage,
    routeTaskListId,
    selectDefaultList,
    shouldLoadSingleTaskList,
    usesSpaceOverview,
    api,
  ]);

  const reloadIssues = useCallback(
    async (statuses: PmsTaskListStatus[] = taskListStatuses) => {
      if (!api || !selectedTaskListId) return;

      try {
        setError(null);
        const res = await listAllTaskListTasks(
          api,
          selectedTaskListId,
          withEffectiveTaskStatusFilter(filterParams, statuses),
          currentWorkspaceSlug,
          { sort: taskSort },
        );
        applyIssueCollection(res.items);
      } catch (err) {
        setError(getErrorMessage(err, t('pms.errors.issueLoadFailed')));
      }
    },
    [
      applyIssueCollection,
      currentWorkspaceSlug,
      getErrorMessage,
      filterParams,
      selectedTaskListId,
      taskListStatuses,
      taskSort,
      api,
      t,
    ],
  );

  const startTaskListDataLoad = useCallback(() => {
    setLoading(true);
    setError(null);
  }, []);

  const applyTaskListDataLoad = useCallback(
    ({
      issues,
      nextMembers,
      nextMilestones,
      nextLabels,
      nextStatuses,
    }: {
      issues: PmsTask[];
      nextMembers: PmsTaskListMember[];
      nextMilestones: PmsMilestone[];
      nextLabels: PmsLabel[];
      nextStatuses: PmsTaskListStatus[];
    }) => {
      applyIssueCollection(issues);
      setMembers(nextMembers);
      setMilestones(nextMilestones);
      setLabels(nextLabels);
      setTaskListStatusState({
        taskListId: selectedTaskListId,
        items: nextStatuses,
      });
    },
    [applyIssueCollection, selectedTaskListId],
  );

  const failTaskListDataLoad = useCallback(
    (err: unknown) => {
      setError(
        getErrorMessage(err, i18n.t('pms.errors.listDataFailed', { ns: 'apps' })),
      );
    },
    [getErrorMessage],
  );

  // Load tasks, members, milestones, labels, and statuses when the selected list changes.
  useEffect(() => {
    if (!api || !selectedTaskListId || !shouldLoadSingleTaskList || routeListPending) {
      return;
    }
    let cancelled = false;
    const selectedList = taskLists.find(
      (taskList) => taskList.id === selectedTaskListId,
    );
    const selectedSpaceId = selectedList?.team_id;
    const statusRequest = listTaskListStatuses(
      api,
      selectedTaskListId,
      currentWorkspaceSlug,
    );
    startTaskListDataLoad();
    Promise.all([
      statusRequest.then((statusResponse) =>
        listAllTaskListTasks(
          api,
          selectedTaskListId,
          withEffectiveTaskStatusFilter(filterParams, statusResponse.items),
          currentWorkspaceSlug,
          { sort: taskSort },
        ),
      ),
      selectedSpaceId
        ? listSpaceMembers(api, selectedSpaceId, currentWorkspaceSlug)
        : Promise.resolve({ items: [], total: 0, page: 1, page_size: 20 }),
      listTaskListMilestones(api, selectedTaskListId, currentWorkspaceSlug),
      listTaskListLabels(api, selectedTaskListId, currentWorkspaceSlug),
      statusRequest,
      listCustomFields(api, selectedTaskListId, currentWorkspaceSlug),
    ])
      .then(
        ([issueRes, memberRes, milestoneRes, labelRes, statusRes, fieldRes]) => {
          if (cancelled) return;
          applyTaskListDataLoad({
            issues: issueRes.items,
            nextMembers: memberRes.items,
            nextMilestones: milestoneRes.items,
            nextLabels: labelRes.items,
            nextStatuses: statusRes.items,
          });
          setCustomFields(fieldRes.items);
        },
      )
      .catch((err) => {
        if (cancelled) return;
        failTaskListDataLoad(err);
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    applyTaskListDataLoad,
    currentWorkspaceSlug,
    failTaskListDataLoad,
    routeListPending,
    taskLists,
    api,
    selectedTaskListId,
    shouldLoadSingleTaskList,
    filterParams,
    startTaskListDataLoad,
    taskSort,
  ]);

  const toggleIssueSelection = useCallback(
    (taskId: string) => {
      setSelectedTaskIds((prev) => {
        const next = new Set(prev);
        if (next.has(taskId)) next.delete(taskId);
        else next.add(taskId);
        return next;
      });
    },
    [setSelectedTaskIds],
  );

  const selectTasks = useCallback(
    (taskIds: readonly string[], selected: boolean) => {
      setSelectedTaskIds((prev) => setTasksSelected(prev, taskIds, selected));
    },
    [setSelectedTaskIds],
  );

  const handleBulkDone = useCallback(async () => {
    setSelectedTaskIds(new Set());
    await reloadIssues();
  }, [reloadIssues, setSelectedTaskIds]);

  const handleLabelsChanged = useCallback((updated: PmsLabel[]) => {
    setLabels(updated);
  }, []);

  const handleMembersChanged = useCallback((updated: PmsTaskListMember[]) => {
    setMembers(updated);
  }, []);

  const handleMilestonesChanged = useCallback((updated: PmsMilestone[]) => {
    setMilestones(updated);
  }, []);

  const handleCustomFieldsChanged = useCallback(
    (updated: PmsCustomField[]) => {
      setCustomFields(updated);
      // Renamed or removed options change stored values.
      void reloadIssues();
    },
    [reloadIssues],
  );

  const handleStatusesChanged = useCallback(
    (updated: PmsTaskListStatus[]) => {
      setTaskListStatusState({
        taskListId: selectedTaskListId,
        items: updated,
      });
      void reloadIssues(updated);
    },
    [reloadIssues, selectedTaskListId],
  );

  const handleReorderIssues = useCallback(
    async (updates: TaskBoardPositionUpdate[]) => {
      if (
        !api ||
        !selectedTaskListId ||
        !canEditTaskList ||
        updates.length === 0
      ) {
        return;
      }
      const result = await reorderTaskListTasks(
        api,
        selectedTaskListId,
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
        currentWorkspaceSlug,
      );
      if (result.items.length === 0) return;
      const updatedById = new Map(
        result.items.map((item) => [item.id, item] as const),
      );
      applyIssueCollection(
        tasks.map((item) => updatedById.get(item.id) ?? item),
      );
    },
    [
      applyIssueCollection,
      canEditTaskList,
      currentWorkspaceSlug,
      selectedTaskListId,
      tasks,
      api,
    ],
  );

  const handleApplyTitleOrder = useCallback(
    async (direction: PmsTaskSortDirection) => {
      if (!api || !selectedTaskListId || !canEditTaskList) return;
      const confirmed = await confirm({
        title: t('pms.list.applyTitleOrderConfirmTitle'),
        description: t('pms.list.applyTitleOrderConfirmDescription', {
          direction:
            direction === 'asc'
              ? t('pms.list.sortAscending')
              : t('pms.list.sortDescending'),
        }),
        confirmLabel: t('pms.list.applyTitleOrder'),
        cancelLabel: t('common:actions.cancel'),
      });
      if (!confirmed) return;
      try {
        await reorderTaskListTasksByTitle(
          api,
          selectedTaskListId,
          direction,
          currentWorkspaceSlug,
        );
        feedback.success(t('pms.list.applyTitleOrderSaved'));
        // Show the saved order (the sort change reloads the tasks), unless the
        // user picked another sort while the save was running.
        setTaskSort((current) =>
          current.field === 'title' ? DEFAULT_PMS_TASK_SORT : current,
        );
      } catch (caughtError) {
        feedback.error(
          getErrorMessage(caughtError, t('pms.list.applyTitleOrderFailed')),
        );
      }
    },
    [
      canEditTaskList,
      confirm,
      currentWorkspaceSlug,
      feedback,
      getErrorMessage,
      selectedTaskListId,
      t,
      api,
    ],
  );

  const handleUpdateIssue = useCallback(
    async (taskId: string, payload: Record<string, unknown>) => {
      if (!api || !canEditTaskList) return;
      const updatedIssue = await updateTask(
        api,
        taskId,
        payload,
        currentWorkspaceSlug,
      );
      applyIssueCollection(
        tasks.map((item) =>
          item.id === updatedIssue.id ? updatedIssue : item,
        ),
      );
      await reloadIssues();
    },
    [
      applyIssueCollection,
      canEditTaskList,
      tasks,
      reloadIssues,
      api,
      currentWorkspaceSlug,
    ],
  );

  // miy: a custom field value edited in the table. The row shows the stored
  // form once the server answers; a failed save keeps the row as it was.
  const handleUpdateCustomField = useCallback(
    async (taskId: string, fieldId: string, value: string) => {
      if (!api || !canEditTaskList) return false;
      try {
        const saved = await setTaskCustomFieldValue(api, taskId, {
          field_id: fieldId,
          value,
        });
        setIssues((current) =>
          current.map((task) =>
            task.id === taskId
              ? {
                  ...task,
                  custom_fields: {
                    ...(task.custom_fields ?? {}),
                    [fieldId]: saved.value,
                  },
                }
              : task,
          ),
        );
        return true;
      } catch (caught) {
        feedback.error(
          apiErrorMessage(caught, t('pms.customFields.errors.valueFailed')),
        );
        return false;
      }
    },
    [api, canEditTaskList, feedback, t],
  );

  const handleCreateIssueInline = useCallback(
    async (title: string, parentId: string | null) => {
      if (!api || !selectedTaskListId || !canEditTaskList) return;
      const createdIssue = await createTaskListTask(
        api,
        selectedTaskListId,
        buildInlineTaskCreatePayload(title, taskListStatuses, parentId),
        currentWorkspaceSlug,
      );
      applyIssueCollection([...tasks, createdIssue]);
      await reloadIssues();
    },
    [
      applyIssueCollection,
      canEditTaskList,
      tasks,
      reloadIssues,
      selectedTaskListId,
      taskListStatuses,
      api,
      currentWorkspaceSlug,
    ],
  );

  const handleDeleteIssueInline = useCallback(
    async (taskId: string) => {
      if (!api || !canEditTaskList) return;
      await deleteTask(api, taskId, currentWorkspaceSlug);
      if (selectedIssueDraft?.id === taskId) {
        clearSelectedIssue();
      }
      applyIssueCollection(tasks.filter((task) => task.id !== taskId));
      await reloadIssues();
    },
    [
      applyIssueCollection,
      canEditTaskList,
      clearSelectedIssue,
      selectedIssueDraft,
      tasks,
      reloadIssues,
      api,
      currentWorkspaceSlug,
    ],
  );

  const prepareRequestedTaskLoad = useCallback(() => {
    setError(null);
    setSelectedTaskIds(new Set());
  }, [setSelectedTaskIds]);

  const applyRequestedTaskDetail = useCallback((task: PmsTask) => {
    const transition = resolveSingleListPmsTaskDetailTransition(task);
    const { listState } = transition;
    setFilterParamsState({
      taskListId: listState.taskListId,
      params: listState.filterParams,
    });
    setSelectedTaskIdsState({
      taskListId: listState.taskListId,
      ids: listState.selectedIds,
    });
    setSelectedTaskListId(listState.taskListId);
    setSelectedIssueDraft(transition.selectedTask);
  }, []);

  // Read the open task through a ref: closing the detail sets the draft to null
  // one render before `?task=` leaves the URL, and re-running this effect on
  // that change would re-select the still-visible task, so the panel needed a
  // second close. The ref keeps the completed-task guard without that re-run.
  const selectedIssueDraftRef = useRef(selectedIssueDraft);
  selectedIssueDraftRef.current = selectedIssueDraft;

  useEffect(() => {
    if (routeListPending) return;
    const transition = resolveRequestedPmsTaskTransition({
      requestedTaskId,
      selectedTask: selectedIssueDraftRef.current,
      visibleTask: findPmsTaskById(tasks, requestedTaskId),
    });
    if (transition.selectedTask) {
      setSelectedIssueDraft(transition.selectedTask);
      return;
    }
    if (!api || !transition.detailRequest) {
      return;
    }

    let cancelled = false;
    prepareRequestedTaskLoad();

    getTaskDetail(api, transition.detailRequest.taskId, currentWorkspaceSlug)
      .then((detail) => {
        if (cancelled) {
          return;
        }

        applyRequestedTaskDetail(detail.task);
      })
      .catch((caughtError) => {
        if (cancelled) {
          return;
        }

        setError(
          getErrorMessage(
            caughtError,
            i18n.t('pms.errors.requestedIssueFailed', { ns: 'apps' }),
          ),
        );
      });

    return () => {
      cancelled = true;
    };
  }, [
    applyRequestedTaskDetail,
    currentWorkspaceSlug,
    getErrorMessage,
    prepareRequestedTaskLoad,
    requestedTaskId,
    routeListPending,
    tasks,
    api,
  ]);

  const consumeCreateTaskRequest = useCallback(
    (
      nextParams: URLSearchParams,
      request: { sourceTodoId: string | null; title: string },
    ) => {
      const createTaskContext = createTaskContextRef.current;
      if (
        !createTaskContext.selectedTaskListId ||
        !createTaskContext.canEditTaskList
      ) {
        setSearchParams(nextParams, { replace: true });
        queueMicrotask(() => setError(t('pms.errors.createTaskNoList')));
        return;
      }

      setSearchParams(nextParams, { replace: true });
      queueMicrotask(() =>
        openNewTaskModal({
          initialTitle: request.title,
          sourceTodoId: request.sourceTodoId,
        }),
      );
    },
    [openNewTaskModal, setSearchParams, t],
  );

  useEffect(() => {
    if (!createTaskRequested || loading) {
      return;
    }

    const nextParams = clearPmsCreateTaskSearchParams(searchParams);

    queueMicrotask(() =>
      consumeCreateTaskRequest(nextParams, createTaskRequest),
    );
  }, [
    consumeCreateTaskRequest,
    createTaskRequest,
    createTaskRequested,
    loading,
    searchParams,
  ]);

  // AI-DO also told the portal's personal to-do widget which to-do became
  // this task (`PERSONAL_TODO_PMS_TASK_CREATED_EVENT`). miy relays nothing
  // between app frames, so only the list reloads.
  const handleNewTaskCreated = useCallback(() => {
    void reloadIssues();
  }, [reloadIssues]);

  if (isAssignedTasksView) {
    return <AssignedToMeView workspaceSlug={currentWorkspaceSlug} />;
  }
  if (isTodayView) {
    return <TodayOverdueView workspaceSlug={currentWorkspaceSlug} />;
  }
  if (spaceTasksSpaceId && spaceTasksTab) {
    const spaceName = resolveLoadedSpaceName(
      spaces,
      taskLists,
      spaceTasksSpaceId,
    );
    return (
      <SpaceTasksView
        activeTab={spaceTasksTab}
        spaceId={spaceTasksSpaceId}
        spaceName={spaceName}
        workspaceSlug={currentWorkspaceSlug}
      />
    );
  }
  if (spaceOverviewId) {
    const spaceName = resolveLoadedSpaceName(
      spaces,
      taskLists,
      spaceOverviewId,
    );
    return (
      <SpaceOverviewView
        spaceId={spaceOverviewId}
        spaceName={spaceName}
        workspaceSlug={currentWorkspaceSlug}
      />
    );
  }
  if (routeListPending) {
    return <PmsCenteredLoadingState minHeightClassName="h-full" />;
  }
  if (isOverviewRoute) {
    if (loading) {
      return <PmsCenteredLoadingState minHeightClassName="h-full" />;
    }
    if (error) {
      return (
        <PmsCenteredStateBlock minHeightClassName="h-full" tone="danger">
          {error}
        </PmsCenteredStateBlock>
      );
    }
    // miy: one PMS per installation, so `/` is AI-DO's app-first global
    // overview (`/pms`, entryBehavior "overview"). AI-DO's workspace route
    // instead redirected to the first space.
    return (
      <>
        <PmsGlobalOverview
          spaces={spaces}
          taskLists={taskLists}
          onCreateSpace={() => setCreateSpaceOpen(true)}
        />
        <CreateSpaceModal
          isOpen={createSpaceOpen}
          onClose={() => setCreateSpaceOpen(false)}
          workspaceSlug={currentWorkspaceSlug}
          onCreated={(space) =>
            navigate(
              buildPmsSpaceToolPath(space.id, {
                workspaceSlug: currentWorkspaceSlug,
              }),
            )
          }
        />
      </>
    );
  }
  if (error && !loading && !selectedTaskListId) {
    return (
      <div className="app-text-body flex h-full items-center justify-center text-app-danger-text">
        {error}
      </div>
    );
  }
  if (!selectedTaskListId && !loading) {
    return (
      <div className="app-text-body flex h-full items-center justify-center text-app-ink/55">
        {t('pms.overviewPage.noLists')}
      </div>
    );
  }

  return (
    <LazyMotion features={domAnimation}>
      <div className="relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden">
        <header className="border-b border-app-border bg-app-bg px-4 pt-4 transition-colors lg:px-6 lg:pt-3">
          {/* Row 1: breadcrumb */}
          <nav className="app-text-caption mb-1.5 hidden min-w-0 items-center gap-1.5 text-app-ink/55 lg:flex">
            <Link
              to={pmsListRootPath}
              className="hover:text-app-ink transition-colors shrink-0"
            >
              {t('pms.title')}
            </Link>
            {selectedTaskList?.team_name ? (
              <>
                <span className="text-app-ink/70 shrink-0">/</span>
                <span className="truncate max-w-[160px]">
                  {selectedTaskList.team_name}
                </span>
              </>
            ) : null}
            {selectedTaskList?.folder_name ? (
              <>
                <span className="text-app-ink/70 shrink-0">/</span>
                <span className="truncate max-w-[160px]">
                  {selectedTaskList.folder_name}
                </span>
              </>
            ) : null}
          </nav>

          {/* Row 2: title + actions */}
          <div className="mb-3 flex min-w-0 items-start justify-between gap-3 lg:items-center lg:gap-4">
            <div className="flex min-w-0 flex-1 items-center gap-2.5">
              <div className="flex size-8 shrink-0 items-center justify-center rounded bg-app-accent text-app-accent-fg lg:size-7">
                <Layout size={16} />
              </div>
              <div className="min-w-0">
                <h1 className="app-text-title-md min-w-0 truncate text-app-ink">
                  {selectedTaskList?.name || taskListName}
                </h1>
                {selectedTaskList?.team_name ? (
                  <p className="app-text-caption mt-0.5 truncate text-app-ink/45 lg:hidden">
                    {selectedTaskList.team_name}
                  </p>
                ) : null}
              </div>
              {isArchivedTaskList ? (
                <span className="app-text-caption inline-flex shrink-0 items-center gap-1 rounded-full border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink/60">
                  <Archive size={12} />
                  {t('pms.archive.archivedBadge')}
                </span>
              ) : null}
              {canManageTaskList ? (
                <div className="relative shrink-0">
                  <button
                    ref={taskListMenuButtonRef}
                    type="button"
                    onClick={() => setTaskListMenuOpen((open) => !open)}
                    className="flex size-7 items-center justify-center rounded text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
                    title={t('pms.spaceTree.listManagement')}
                    aria-label={t('pms.spaceTree.listManagement')}
                    aria-expanded={taskListMenuOpen}
                    aria-haspopup="menu"
                  >
                    <MoreHorizontal size={14} />
                  </button>
                  <ListContextMenu
                    open={taskListMenuOpen}
                    anchorRef={taskListMenuButtonRef}
                    onClose={() => setTaskListMenuOpen(false)}
                    onSettings={
                      isArchivedTaskList
                        ? undefined
                        : () => setSettingsOpen(true)
                    }
                    onRename={
                      isArchivedTaskList
                        ? undefined
                        : handleRenameSelectedTaskList
                    }
                    onArchive={
                      isArchivedTaskList
                        ? undefined
                        : handleArchiveSelectedTaskList
                    }
                    onRestore={
                      isArchivedTaskList
                        ? handleRestoreSelectedTaskList
                        : undefined
                    }
                    onDelete={
                      isArchivedTaskList
                        ? handleDeleteSelectedTaskList
                        : undefined
                    }
                  />
                </div>
              ) : null}
              <button
                type="button"
                className="shrink-0 text-app-ink/70 hover:text-yellow-500 transition-colors"
                title={t('pms.actions.favorite')}
              >
                <Star size={14} />
              </button>
            </div>
            <div className="flex shrink-0 items-center gap-1">
              {canManageTaskList && !isArchivedTaskList ? (
                <button
                  type="button"
                  onClick={() => void handleArchiveSelectedTaskList()}
                  className="app-text-control-sm hidden h-8 items-center gap-1.5 rounded px-2.5 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink lg:inline-flex"
                >
                  <Archive size={14} />
                  {t('pms.archive.action')}
                </button>
              ) : null}
              {canManageTaskList && isArchivedTaskList ? (
                <button
                  type="button"
                  onClick={() => void handleRestoreSelectedTaskList()}
                  className="app-text-control-sm hidden h-8 items-center gap-1.5 rounded border border-app-border px-2.5 text-app-ink/70 transition-colors hover:bg-app-surface-hover hover:text-app-ink lg:inline-flex"
                >
                  <ArchiveRestore size={14} />
                  {t('pms.archive.restore')}
                </button>
              ) : null}
              {selectedTaskListId && (
                <button
                  type="button"
                  // Until the statuses load, a filtered export cannot leave out the
                  // completed tasks the list hides.
                  disabled={!taskListStatusesReady}
                  onClick={() => {
                    if (api)
                      void exportTaskListXlsx(
                        api,
                        apiBlob,
                        selectedTaskListId,
                        exportFilter,
                        currentWorkspaceSlug,
                      );
                  }}
                  className="flex size-8 items-center justify-center rounded text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink disabled:cursor-not-allowed disabled:opacity-40"
                  title={t(
                    exportFilter
                      ? 'pms.actions.exportXlsxFiltered'
                      : 'pms.actions.exportXlsx',
                  )}
                >
                  <Download size={15} />
                </button>
              )}
              {canEditTaskList ? (
                <>
                  <div className="w-px h-5 bg-app-border mx-1" />
                  <button
                    type="button"
                    onClick={() => openNewTaskModal()}
                    className="app-control-primary h-9 w-9 shadow-sm sm:w-auto sm:px-3 lg:h-8"
                    aria-label={t('pms.newTask')}
                  >
                    <Plus size={14} />
                    <span className="hidden sm:inline">{t('pms.newTask')}</span>
                  </button>
                </>
              ) : null}
            </div>
          </div>

          {selectedTaskList && <LinkedResources scope="list" ownerId={selectedTaskList.id} canEdit={canEditTaskList} />}

          <div className="hidden items-center gap-6 lg:flex">
            {PMS_VIEW_TABS.map((tab) => (
              <button
                type="button"
                key={tab}
                onClick={() => selectViewTab(tab)}
                className={cn(
                  'app-text-control-sm relative pb-3 transition-all',
                  activeTab === tab
                    ? 'text-app-ink'
                    : 'text-app-ink/55 hover:text-app-ink',
                )}
              >
                <div className="flex items-center gap-2">
                  {tab === 'List' && <ListIcon size={14} />}
                  {tab === 'Board' && <Grid size={14} />}
                  {tab === 'Calendar' && <Calendar size={14} />}
                  {tab === 'Gantt' && <Activity size={14} />}
                  {tab === 'Table' && <Table size={14} />}
                  {t(PMS_VIEW_TAB_LABEL_KEYS[tab])}
                </div>
                {activeTab === tab && (
                  <m.div
                    layoutId="activeTab"
                    className="absolute bottom-0 left-0 right-0 h-0.5 bg-app-accent"
                  />
                )}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-1 pb-3 lg:hidden">
            <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
              {PMS_MOBILE_PRIMARY_TABS.map((tab) => (
                <button
                  type="button"
                  key={tab}
                  onClick={() => selectViewTab(tab)}
                  className={cn(
                    'app-control shrink-0 px-3',
                    activeTab === tab
                      ? 'app-control-active'
                      : 'border-app-border text-app-ink/65 hover:bg-app-surface-hover hover:text-app-ink',
                  )}
                >
                  {tab === 'List' && <ListIcon size={14} />}
                  {tab === 'Board' && <Grid size={14} />}
                  {tab === 'Calendar' && <Calendar size={14} />}
                  {t(PMS_VIEW_TAB_LABEL_KEYS[tab])}
                </button>
              ))}
            </div>

            <div ref={mobileMoreRef} className="relative shrink-0">
              <button
                type="button"
                onClick={() => setMobileMoreOpen((open) => !open)}
                className={cn(
                  'app-control px-3',
                  mobileMoreActive
                    ? 'app-control-active'
                    : 'border-app-border text-app-ink/65 hover:bg-app-surface-hover hover:text-app-ink',
                )}
              >
                <MoreHorizontal size={14} />
                {t('pms.actions.more')}
              </button>
              {mobileMoreOpen ? (
                <div className="absolute right-0 top-full z-30 mt-1 w-44 rounded-lg border border-app-border bg-app-bg py-1 shadow-xl">
                  {PMS_MOBILE_MORE_TABS.map((tab) => (
                    <button
                      key={tab}
                      type="button"
                      onClick={() => selectViewTab(tab)}
                      className={cn(
                        'app-text-control-sm flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-app-surface-hover',
                        activeTab === tab ? 'text-app-accent' : 'text-app-ink',
                      )}
                    >
                      {tab === 'Gantt' && <Activity size={14} />}
                      {tab === 'Table' && <Table size={14} />}
                      {t(PMS_VIEW_TAB_LABEL_KEYS[tab])}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          </div>
        </header>

        {isArchivedTaskList ? (
          <div className="app-text-caption flex items-center gap-2 border-b border-app-border bg-app-surface-sidebar px-4 py-2 text-app-ink/60 lg:px-6">
            <Archive size={13} className="shrink-0" />
            <span>{t('pms.archive.readOnlyNotice')}</span>
          </div>
        ) : null}

        {selectedTaskListId && (
          <FilterBar
            taskListId={selectedTaskListId}
            filterParams={filterParams}
            setFilterParams={setFilterParams}
            members={members}
            currentUserId={user?.id}
            milestones={milestones}
            labels={labels}
            taskListStatuses={taskListStatuses}
            customFields={customFields}
            savedFilters={savedFilters}
            captureSavedFilter={captureSavedFilter}
            onApplySavedFilter={applySavedFilter}
            canShareSavedFilters={canManageTaskList}
          />
        )}

        <main
          className={cn(
            'min-h-0 min-w-0 flex-1 custom-scrollbar',
            activeTab === 'List'
              ? 'overflow-hidden p-4 lg:p-8'
              : 'overflow-y-auto p-4 lg:p-8',
          )}
        >
          {loading && tasks.length === 0 ? (
            <PmsCenteredLoadingState />
          ) : error ? (
            <PmsCenteredStateBlock tone="danger">{error}</PmsCenteredStateBlock>
          ) : (
            <AnimatePresence mode="wait">
              {activeTab === 'List' && (
                <m.div
                  key="list"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  className="h-full min-h-0 min-w-0"
                >
                  <ListView
                    completedItemsControlDisabled={!taskListStatusesReady}
                    groupBy={taskListGroupBy}
                    tasks={visibleTasks}
                    onGroupByChange={setTaskListGroupBy}
                    onSortChange={setTaskSort}
                    onApplyTitleOrder={handleApplyTitleOrder}
                    onShowCompletedItemsChange={handleShowCompletedItemsChange}
                    showCompletedItems={showCompletedItems}
                    sort={taskSort}
                    onSelectIssue={handleSelectIssue}
                    selectedIds={selectedTaskIds}
                    onToggleSelect={toggleIssueSelection}
                    onSelectTasks={selectTasks}
                    taskListStatuses={taskListStatuses}
                    members={members}
                    currentUserId={user?.id}
                    canEdit={canEditTaskList}
                    onUpdateIssue={handleUpdateIssue}
                    onReorderIssues={handleReorderIssues}
                    onCreateIssue={handleCreateIssueInline}
                    onDeleteIssue={handleDeleteIssueInline}
                  />
                </m.div>
              )}
              {activeTab === 'Board' && (
                <m.div
                  key="board"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  className="h-full"
                >
                  <BoardView
                    tasks={visibleTasks}
                    onSelectIssue={handleSelectIssue}
                    onUpdateIssue={
                      canEditTaskList ? handleUpdateIssue : undefined
                    }
                    selectedIds={selectedTaskIds}
                    onToggleSelect={
                      canEditTaskList ? toggleIssueSelection : undefined
                    }
                    members={members}
                    taskListStatuses={taskListStatuses}
                  />
                </m.div>
              )}
              {activeTab === 'Calendar' && (
                <m.div
                  key="calendar"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  className="h-full"
                >
                  <CalendarView
                    milestones={milestones}
                    tasks={visibleTasks}
                    taskListStatuses={taskListStatuses}
                  />
                </m.div>
              )}
              {activeTab === 'Gantt' && (
                <m.div
                  key="gantt"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  className="h-full"
                >
                  <GanttView
                    milestones={milestones}
                    tasks={visibleTasks}
                    taskListStatuses={taskListStatuses}
                    members={members}
                    onSelectIssue={handleSelectIssue}
                    onUpdateIssue={handleUpdateIssue}
                    canEdit={canEditTaskList}
                    completedItemsControlDisabled={!taskListStatusesReady}
                    onShowCompletedItemsChange={handleShowCompletedItemsChange}
                    showCompletedItems={showCompletedItems}
                    onOpenStatusSettings={
                      canManageTaskList && !isArchivedTaskList
                        ? () => setSettingsOpen(true)
                        : undefined
                    }
                  />
                </m.div>
              )}
              {activeTab === 'Table' && (
                <m.div
                  key="table"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -10 }}
                  className="h-full"
                >
                  <TableView
                    canEdit={canEditTaskList}
                    customFields={customFields}
                    onSortChange={setTaskSort}
                    onUpdateCustomField={handleUpdateCustomField}
                    sort={taskSort}
                    tasks={visibleTasks}
                    onSelectIssue={handleSelectIssue}
                    onUpdateIssue={handleUpdateIssue}
                    selectedIds={selectedTaskIds}
                    onToggleSelect={toggleIssueSelection}
                    onSelectTasks={selectTasks}
                    members={members}
                    taskListStatuses={taskListStatuses}
                  />
                </m.div>
              )}
            </AnimatePresence>
          )}
        </main>

        {/* Task detail — full-size modal overlay (ClickUp style) */}
        <AnimatePresence>
          {selectedIssue && (
            <TaskDetailModal
              title={selectedIssue.title}
              onClose={clearSelectedIssue}
            >
              <TaskDetail
                task={selectedIssue}
                members={members}
                milestones={milestones}
                taskListLabels={labels}
                taskListStatuses={taskListStatuses}
                customFields={customFields}
                spaceName={selectedTaskList?.team_name}
                spaceId={selectedTaskList?.team_id ?? null}
                workspaceSlug={currentWorkspaceSlug}
                canEdit={canEditTaskList}
                onClose={clearSelectedIssue}
                onUpdate={reloadIssues}
              />
            </TaskDetailModal>
          )}
        </AnimatePresence>

        <AnimatePresence>
          {isNewTaskModalOpen && (
            <NewTaskModal
              key={newTaskModalKey}
              isOpen={isNewTaskModalOpen}
              initialTitle={newTaskContext.initialTitle}
              onClose={closeNewTaskModal}
              taskListId={selectedTaskListId}
              taskListSpaceId={selectedTaskList?.team_id ?? null}
              onCreated={handleNewTaskCreated}
              taskListStatuses={taskListStatuses}
              canCreate={canEditTaskList}
              workspaceSlug={currentWorkspaceSlug}
            />
          )}
        </AnimatePresence>

        <AnimatePresence>
          {settingsOpen && selectedTaskListId && !isArchivedTaskList && (
            <TaskListSettingsPanel
              taskListId={selectedTaskListId}
              taskListName={selectedTaskList?.name ?? null}
              teamId={selectedTaskList?.team_id ?? null}
              workspaceSlug={currentWorkspaceSlug}
              currentUserRole={selectedTaskList?.role ?? null}
              onClose={() => setSettingsOpen(false)}
              onLabelsChanged={handleLabelsChanged}
              onMembersChanged={handleMembersChanged}
              onStatusesChanged={handleStatusesChanged}
              onMilestonesChanged={handleMilestonesChanged}
              onCustomFieldsChanged={handleCustomFieldsChanged}
            />
          )}
        </AnimatePresence>

        {canEditTaskList && selectedTaskIds.size > 0 && selectedTaskListId && (
          <BulkActionBar
            workspaceSlug={currentWorkspaceSlug}
            taskListId={selectedTaskListId}
            selectedIds={selectedTaskIds}
            totalCount={visibleTasks.length}
            onSelectAll={() =>
              setSelectedTaskIds(new Set(visibleTasks.map((task) => task.id)))
            }
            onDeselectAll={() => setSelectedTaskIds(new Set())}
            onDone={handleBulkDone}
            members={members}
            labels={labels}
            taskListStatuses={taskListStatuses}
          />
        )}

        <CreateSpaceModal
          isOpen={createSpaceOpen}
          onClose={() => setCreateSpaceOpen(false)}
          workspaceSlug={currentWorkspaceSlug}
          onCreated={(space) => {
            setSpaces((current) => [
              space,
              ...current.filter((item) => item.id !== space.id),
            ]);
            navigate(
              runtime
                ? buildAppRoutePath({
                    routeBase: runtime.app.route_base,
                    spaceKey: space.workspace_key,
                  })
                : buildPmsSpaceToolPath(space.id, {
                    workspaceSlug: currentWorkspaceSlug,
                  }),
            );
          }}
        />
        {promptDialog}
        {confirmDialog}
      </div>
    </LazyMotion>
  );
}
