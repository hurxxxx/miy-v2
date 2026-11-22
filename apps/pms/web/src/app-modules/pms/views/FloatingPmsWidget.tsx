import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  CalendarDays,
  CheckCircle2,
  ClipboardList,
  Plus,
} from 'lucide-react';
import { Button, ContentState, StatusSlot, useConfirm } from '@miy/ui';

import { cn } from '../../../lib/utils';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import { normalizeTimeZone, zonedDateKey } from '../../../platform/time/time-utils';
import { runRequestsWithConcurrency } from '../../../platform/network/request-concurrency';

import {
  listAllPmsTaskLists,
  listPersonalPmsAssignedTasks,
  listTaskListStatuses,
  type PersonalPmsTask,
  type PmsTaskList,
  type PmsTaskListStatus,
  type PmsWorkspaceRef,
} from '../api/pms-api';
import { taskListRoleAllows } from '../api/pms-permissions';
import { formatDate, getStatusLabel, getStatusTone } from './pms-constants';
import { NewTaskModal } from './NewTaskModal';
import { TaskDetail } from './TaskDetail';
import { TaskDetailModal } from './TaskDetailModal';
import { reconcilePmsTaskListCatalog } from './pms-task-list-catalog-model';
import { usePmsTaskListChangeSubscription } from './usePmsTaskListChangeSubscription';

const FLOATING_PMS_STATUS_LOAD_CONCURRENCY = 4;

function taskListLabel(taskList: PmsTaskList): string {
  return [taskList.team_name, taskList.folder_name, taskList.name]
    .filter(Boolean)
    .join(' / ');
}

function statusToneClass(
  status: string,
  statuses?: PmsTaskListStatus[],
): string {
  switch (getStatusTone(status, statuses)) {
    case 'accent':
      return 'bg-app-accent/10 text-app-accent';
    case 'danger':
      return 'bg-[var(--ui-color-danger)]/10 text-[var(--ui-color-danger)]';
    case 'success':
      return 'bg-[var(--ui-color-success)]/10 text-[var(--ui-color-success)]';
    case 'warning':
      return 'bg-[var(--ui-color-warning)]/15 text-[var(--ui-color-warning)]';
    default:
      return 'bg-app-surface text-app-ink/55';
  }
}

function dueDateToneClass(dueDate: string | null | undefined, today: string): string {
  if (!dueDate) {
    return 'text-app-ink/45';
  }
  if (dueDate < today) {
    return 'text-[var(--ui-color-danger)]';
  }
  if (dueDate === today) {
    return 'text-[var(--ui-color-warning)]';
  }
  return 'text-app-ink/45';
}

function getErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

/**
 * AI-DO's floating PMS widget (portal dock), shown in miy as the app's side
 * panel `/widgets/my-tasks`. The portal-global open requests
 * (`dispatchFloatingPmsOpen`: open a task, create from a personal to-do) and
 * the to-do conversion event are not ported: the platform relays nothing
 * between frames. A task opens in the panel the way the widget opened it.
 */
export function FloatingPmsWidget({
  onChanged,
  reloadSeq = 0,
  workspaceSlug,
}: {
  onChanged?: () => void;
  reloadSeq?: number;
  workspaceSlug?: string | null;
}) {
  const { t } = useTranslation('apps');
  const { api, settings } = useRuntime();
  const today = zonedDateKey(new Date(), normalizeTimeZone(settings.timeZone));
  const { confirm, confirmDialog } = useConfirm();
  const [detailDirty, setDetailDirty] = useState(false);
  const [tasks, setTasks] = useState<PersonalPmsTask[]>([]);
  const [workspaces, setWorkspaces] = useState<PmsWorkspaceRef[]>([]);
  const [taskLists, setTaskLists] = useState<PmsTaskList[]>([]);
  const [workspaceSlugByTaskListId, setWorkspaceSlugByTaskListId] = useState<
    Record<string, string>
  >({});
  const [statusesByListId, setStatusesByListId] = useState<
    Record<string, PmsTaskListStatus[]>
  >({});
  const [newTaskListId, setNewTaskListId] = useState('');
  const [newTaskWorkspaceSlug, setNewTaskWorkspaceSlug] = useState('');
  const [newTaskListsLoading, setNewTaskListsLoading] = useState(false);
  const [newTaskInitialTitle, setNewTaskInitialTitle] = useState('');
  const [newTaskModalKey, setNewTaskModalKey] = useState(0);
  const [newTaskOpen, setNewTaskOpen] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedExternalTask, setSelectedExternalTask] =
    useState<PersonalPmsTask | null>(null);
  const [loadingTaskDetailId, setLoadingTaskDetailId] = useState<string | null>(
    null,
  );
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const reloadGenerationRef = useRef(0);

  const taskListById = useMemo(
    () => new Map(taskLists.map((taskList) => [taskList.id, taskList])),
    [taskLists],
  );
  const editableTaskLists = useMemo(
    () =>
      taskLists.filter(
        (taskList) =>
          taskListRoleAllows(taskList.role, 'member') &&
          workspaceSlugByTaskListId[taskList.id] === newTaskWorkspaceSlug,
      ),
    [newTaskWorkspaceSlug, taskLists, workspaceSlugByTaskListId],
  );
  const selectedNewTaskList =
    editableTaskLists.find((taskList) => taskList.id === newTaskListId) ??
    editableTaskLists[0] ??
    null;
  const newTaskListOptions = useMemo(
    () =>
      editableTaskLists.map((taskList) => ({
        id: taskList.id,
        label: taskListLabel(taskList),
      })),
    [editableTaskLists],
  );
  const openTasks = tasks;
  const selectedTask =
    selectedExternalTask?.id === selectedTaskId
      ? selectedExternalTask
      : (openTasks.find((task) => task.id === selectedTaskId) ?? null);
  const selectedTaskWorkspaceSlug = selectedTask?.workspace.slug ?? null;
  const selectedTaskList = selectedTask
    ? (taskListById.get(selectedTask.list_id) ?? null)
    : null;
  const selectedTaskStatuses = selectedTask
    ? (statusesByListId[selectedTask.list_id] ?? [])
    : [];

  const loadWorkspaceContext = useCallback(
    async (targetWorkspaceSlug: string): Promise<PmsTaskList[]> => {
      if (!api || !targetWorkspaceSlug) return [];
      // miy: the widget's space `slug` is the space id
      // (listPersonalPmsAssignedTasks), so the lists of that space are read
      // with `team_id`; AI-DO read the space's own workspace-scoped list.
      const response = await listAllPmsTaskLists(
        api,
        targetWorkspaceSlug,
        targetWorkspaceSlug,
      );
      const lists = response.items;
      const statusEntries = await runRequestsWithConcurrency(
        lists.map((taskList) => taskList.id),
        FLOATING_PMS_STATUS_LOAD_CONCURRENCY,
        async (taskListId) => {
          try {
            const statusResponse = await listTaskListStatuses(
              api,
              taskListId,
              targetWorkspaceSlug,
            );
            return [taskListId, statusResponse.items] as const;
          } catch {
            return [taskListId, [] as PmsTaskListStatus[]] as const;
          }
        },
      );
      setTaskLists((current) => {
        const byId = new Map(
          current.map((taskList) => [taskList.id, taskList]),
        );
        lists.forEach((taskList) => byId.set(taskList.id, taskList));
        return Array.from(byId.values());
      });
      setWorkspaceSlugByTaskListId((current) => ({
        ...current,
        ...Object.fromEntries(
          lists.map((taskList) => [taskList.id, targetWorkspaceSlug]),
        ),
      }));
      setStatusesByListId((current) => ({
        ...current,
        ...Object.fromEntries(statusEntries),
      }));
      return lists;
    },
    [api],
  );

  const reload = useCallback(async () => {
    const reloadGeneration = reloadGenerationRef.current + 1;
    reloadGenerationRef.current = reloadGeneration;
    if (!api) {
      setTasks([]);
      setWorkspaces([]);
      setTaskLists([]);
      setWorkspaceSlugByTaskListId({});
      setStatusesByListId({});
      setLoading(false);
      return;
    }

    setLoading(true);
    setLoadError(null);
    try {
      const assignedResponse = await listPersonalPmsAssignedTasks(api);
      if (reloadGeneration !== reloadGenerationRef.current) return;
      setTasks(assignedResponse.items);
      setWorkspaces(assignedResponse.workspaces);
      setNewTaskWorkspaceSlug((current) =>
        current &&
        assignedResponse.workspaces.some((item) => item.slug === current)
          ? current
          : (assignedResponse.workspaces.find(
              (item) => item.slug === workspaceSlug,
            )?.slug ??
            (assignedResponse.workspaces.length === 1
              ? assignedResponse.workspaces[0]?.slug
              : null) ??
            ''),
      );
    } catch (caughtError) {
      if (reloadGeneration !== reloadGenerationRef.current) return;
      setLoadError(
        getErrorMessage(caughtError, t('pms.errors.assignedIssuesLoadFailed')),
      );
    } finally {
      if (reloadGeneration === reloadGenerationRef.current) setLoading(false);
    }
  }, [t, api, workspaceSlug]);

  useEffect(() => {
    let cancelled = false;
    if (!newTaskWorkspaceSlug) {
      setNewTaskListId('');
      setNewTaskListsLoading(false);
      return undefined;
    }
    setNewTaskListsLoading(true);
    loadWorkspaceContext(newTaskWorkspaceSlug)
      .then((lists) => {
        if (cancelled) return;
        setNewTaskListId((current) =>
          current && lists.some((taskList) => taskList.id === current)
            ? current
            : (lists.find((taskList) =>
                taskListRoleAllows(taskList.role, 'member'),
              )?.id ?? ''),
        );
      })
      .catch(() => {
        if (!cancelled) setNewTaskListId('');
      })
      .finally(() => {
        if (!cancelled) setNewTaskListsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [loadWorkspaceContext, newTaskWorkspaceSlug]);

  useEffect(() => {
    void reload();
  }, [reload, reloadSeq]);

  usePmsTaskListChangeSubscription(
    useCallback(
      (detail) => {
        const changedListId =
          detail.type === 'updated' ? detail.taskList.id : detail.taskListId;
        const leftActiveCatalog =
          detail.type === 'deleted' || detail.taskList.archived;

        setTaskLists((current) =>
          detail.type === 'updated'
            ? reconcilePmsTaskListCatalog(current, detail.taskList, 'active')
            : current.filter((taskList) => taskList.id !== detail.taskListId),
        );
        if (leftActiveCatalog) {
          setTasks((current) =>
            current.filter((task) => task.list_id !== changedListId),
          );
          setNewTaskListId((current) =>
            current === changedListId ? '' : current,
          );
          setStatusesByListId((current) => {
            const next = { ...current };
            delete next[changedListId];
            return next;
          });
          setWorkspaceSlugByTaskListId((current) => {
            const next = { ...current };
            delete next[changedListId];
            return next;
          });
          if (selectedTask?.list_id === changedListId) {
            setSelectedTaskId(null);
            setSelectedExternalTask(null);
            setLoadingTaskDetailId(null);
          }
        }

        void reload();
        if (newTaskWorkspaceSlug) {
          void loadWorkspaceContext(newTaskWorkspaceSlug);
        }
      },
      [
        loadWorkspaceContext,
        newTaskWorkspaceSlug,
        reload,
        selectedTask?.list_id,
      ],
    ),
  );

  useEffect(() => {
    if (
      selectedTaskId &&
      loadingTaskDetailId !== selectedTaskId &&
      selectedExternalTask?.id !== selectedTaskId &&
      !openTasks.some((task) => task.id === selectedTaskId)
    ) {
      setSelectedTaskId(null);
    }
  }, [loadingTaskDetailId, openTasks, selectedExternalTask, selectedTaskId]);

  const handleTaskCreated = useCallback(() => {
    void reload();
    onChanged?.();
  }, [onChanged, reload]);

  const closeSelectedTask = useCallback(async () => {
    if (detailDirty) {
      const confirmed = await confirm({
        title: t('common:actions.close'),
        description: t('pms.unsavedChanges'),
        confirmLabel: t('common:actions.close'),
        cancelLabel: t('common:actions.cancel'),
      });
      if (!confirmed) return;
    }
    setSelectedTaskId(null);
    setSelectedExternalTask(null);
    setLoadingTaskDetailId(null);
  }, [confirm, detailDirty, t]);

  const closeNewTaskModal = useCallback(() => {
    setNewTaskOpen(false);
    setNewTaskInitialTitle('');
  }, []);

  const handleTaskUpdated = useCallback(async () => {
    await reload();
    onChanged?.();
  }, [onChanged, reload]);

  const detailModal = selectedTask ? (
    <TaskDetailModal title={selectedTask.title} onClose={closeSelectedTask}>
      <TaskDetail
        canEdit={taskListRoleAllows(selectedTaskList?.role, 'member')}
        onClose={closeSelectedTask}
        onDirtyChange={setDetailDirty}
        onUpdate={handleTaskUpdated}
        spaceId={selectedTaskList?.team_id ?? null}
        spaceName={selectedTaskList?.team_name}
        task={selectedTask}
        taskListStatuses={selectedTaskStatuses}
        workspaceSlug={selectedTaskWorkspaceSlug}
      />
    </TaskDetailModal>
  ) : null;

  return (
    <div className="flex h-full min-w-0 flex-col bg-app-bg text-app-ink">
      <section className="flex shrink-0 items-center justify-between gap-3 border-b border-app-border px-4 py-3">
        <div className="min-w-0">
          <h4 className="app-text-title-sm text-app-ink">
            {t('pms.floating.assignedTitle')}
          </h4>
          <p className="app-text-caption text-app-ink/45">
            {t('pms.floating.assignedCount', { count: openTasks.length })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            className="shrink-0 gap-1.5"
            disabled={workspaces.length === 0}
            onClick={() => {
              setNewTaskInitialTitle('');
              setNewTaskModalKey((current) => current + 1);
              setNewTaskOpen(true);
            }}
            variant="primary"
          >
            <Plus aria-hidden="true" size={15} />
            <span>{t('pms.newTask')}</span>
          </Button>
        </div>
      </section>

      <StatusSlot
        announce="assertive"
        className="shrink-0 px-4"
        message={openTasks.length > 0 ? loadError : null}
        size="compact"
        tone="error"
      />

      <div className="custom-scrollbar min-h-0 flex-1 overflow-y-auto">
        {loadError && openTasks.length === 0 ? (
          <ContentState kind="error" size="fill" title={loadError} />
        ) : loading && openTasks.length === 0 ? (
          <ContentState
            kind="loading"
            size="fill"
            title={t('common:feedback.loading')}
          />
        ) : openTasks.length === 0 ? (
          <div className="flex h-full items-center justify-center px-6 text-center">
            <div className="grid justify-items-center gap-2">
              <ClipboardList
                aria-hidden="true"
                className="text-app-ink/25"
                size={24}
              />
              <p className="app-text-body-sm text-app-ink/45">
                {t('pms.floating.emptyAssigned')}
              </p>
            </div>
          </div>
        ) : (
          <ul className="divide-y divide-app-border">
            {openTasks.map((task) => {
              const taskStatuses = statusesByListId[task.list_id] ?? [];
              const taskList = taskListById.get(task.list_id) ?? null;
              return (
                <li key={task.id}>
                  <button
                    type="button"
                    aria-label={t('pms.floating.openTask', {
                      title: task.title,
                    })}
                    className="w-full px-4 py-3 text-left transition-colors hover:bg-app-surface-hover focus:outline-none focus:ring-2 focus:ring-inset focus:ring-app-accent/30"
                    onClick={() => {
                      setSelectedExternalTask(task);
                      setSelectedTaskId(task.id);
                      void loadWorkspaceContext(task.workspace.slug);
                    }}
                  >
                    <div className="flex min-w-0 items-start gap-2.5">
                      <CheckCircle2
                        aria-hidden="true"
                        className="mt-0.5 shrink-0 text-app-ink/30"
                        size={16}
                      />
                      <div className="min-w-0 flex-1">
                        <p className="app-text-body-sm line-clamp-2 text-app-ink">
                          {task.title}
                        </p>
                        <p className="app-text-caption mt-1 truncate text-app-ink/45">
                          {task.workspace.name}
                          {taskList ? ` · ${taskListLabel(taskList)}` : null}
                        </p>
                        <div className="mt-2 flex flex-wrap items-center gap-1.5">
                          <span
                            className={cn(
                              'app-text-micro rounded-full px-2 py-0.5',
                              statusToneClass(task.status, taskStatuses),
                            )}
                          >
                            {getStatusLabel(task.status, taskStatuses)}
                          </span>
                          {task.due_date ? (
                            <span
                              className={cn(
                                'app-text-micro inline-flex items-center gap-1',
                                dueDateToneClass(task.due_date, today),
                              )}
                            >
                              <CalendarDays aria-hidden="true" size={12} />
                              {formatDate(task.due_date)}
                            </span>
                          ) : null}
                        </div>
                      </div>
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      {newTaskOpen ? (
        <NewTaskModal
          key={newTaskModalKey}
          canCreate={
            selectedNewTaskList
              ? !newTaskListsLoading &&
                taskListRoleAllows(selectedNewTaskList.role, 'member')
              : false
          }
          contentClassName="z-[120]"
          initialTitle={newTaskInitialTitle}
          isOpen={newTaskOpen}
          onClose={closeNewTaskModal}
          onCreated={handleTaskCreated}
          onTaskListIdChange={setNewTaskListId}
          onWorkspaceSlugChange={setNewTaskWorkspaceSlug}
          overlayClassName="z-[119]"
          parentPickerContentClassName="z-[122]"
          parentPickerOverlayClassName="z-[121]"
          taskListId={selectedNewTaskList?.id ?? ''}
          taskListLoading={newTaskListsLoading}
          taskListSpaceId={selectedNewTaskList?.team_id ?? null}
          taskListOptions={newTaskListOptions}
          taskListStatuses={
            selectedNewTaskList
              ? (statusesByListId[selectedNewTaskList.id] ?? [])
              : []
          }
          workspaceOptions={workspaces.map((workspace) => ({
            label: workspace.name,
            slug: workspace.slug,
          }))}
          workspaceSlug={newTaskWorkspaceSlug}
        />
      ) : null}

      {detailModal}
      {confirmDialog}
    </div>
  );
}
