import {
  useEffect,
  useMemo,
  useReducer,
  useState,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';

import { NoAccessNotice } from '../../../components/common/NoAccessNotice';
import { ResourcePickerDialog } from '../../../components/picker/ResourcePickerDialog';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import { useAppTargetSpaces } from '../../../platform/app-runtime/use-app-target-spaces';
import {
  listPmsTaskLists,
  listTaskListTasks,
  type PmsTask,
} from '../api/pms-api';
import {
  EMPTY_EXCLUDED_TASK_IDS,
  INITIAL_TASK_PICKER_MODAL_STATE,
  buildTaskPickerTaskParams,
  getVisibleTaskPickerTasks,
  taskPickerModalReducer,
} from './task-picker-model';

export interface TaskPickerModalCopy {
  titleKey: string;
  descriptionKey: string;
  workspaceLabelKey: string;
  noAccessActionKey: string;
  loadListsErrorKey: string;
  loadTasksErrorKey: string;
  attachErrorKey: string;
  taskListLabelKey: string;
  noTaskListsKey: string;
  searchPlaceholderKey: string;
  emptyKey: string;
}

export interface TaskPickerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onPick: (task: PmsTask) => Promise<void> | void;
  excludeTaskIds?: string[];
  workspaceSlug?: string | null;
  copy?: TaskPickerModalCopy;
  contentClassName?: string;
  fixedTaskListId?: string | null;
  layer?: 'default' | 'elevated';
  overlayClassName?: string;
}

const DEFAULT_TASK_PICKER_MODAL_COPY: TaskPickerModalCopy = {
  titleKey: 'pms.taskPicker.title',
  descriptionKey: 'pms.taskPicker.description',
  workspaceLabelKey: 'pms.taskPicker.pmsWorkspace',
  noAccessActionKey: 'pms.taskPicker.attachAction',
  loadListsErrorKey: 'pms.taskPicker.errors.loadListsFailed',
  loadTasksErrorKey: 'pms.taskPicker.errors.loadTasksFailed',
  attachErrorKey: 'pms.taskPicker.errors.attachFailed',
  taskListLabelKey: 'pms.taskPicker.taskListLabel',
  noTaskListsKey: 'pms.taskPicker.noTaskLists',
  searchPlaceholderKey: 'pms.taskPicker.searchPlaceholder',
  emptyKey: 'pms.taskPicker.empty',
};

export function TaskPickerModal(props: TaskPickerModalProps) {
  const user = usePmsCurrentUser();
  return (
    <TaskPickerSpaceSelection
      key={`${user?.id ?? ''}:${props.workspaceSlug ?? ''}:${props.fixedTaskListId ?? ''}`}
      {...props}
    />
  );
}

function TaskPickerSpaceSelection(props: TaskPickerModalProps) {
  const api = useAppApi();
  const { t } = useTranslation('shell');
  const targets = useAppTargetSpaces('pms', { includeViewers: true });
  // PMS global is an aggregate, not the owner of a task list. Attachment and
  // parent selection always read the selected list's explicit owner space.
  const choices = targets.choices.filter((space) => space.kind === 'app_space');
  const [selectedKey, setSelectedKey] = useState(props.workspaceSlug ?? '');
  // miy: a fixed list (parent picker) is read by its id and the server checks
  // access, so it needs no owner-space choice; the PMS workspace key the
  // screens carry (`PMS_WORKSPACE_KEY`) is not a space key.
  const allowed =
    Boolean(props.fixedTaskListId) ||
    choices.some((space) => space.key === selectedKey);
  const canAccess =
    Boolean(api) &&
    (props.fixedTaskListId
      ? allowed || targets.loading || targets.error
      : choices.length > 0 || targets.loading || targets.error);
  return (
    <TaskPickerContent
      key={`${selectedKey}:${allowed}`}
      {...props}
      workspaceSlug={selectedKey}
      canAccess={canAccess}
      canLoad={allowed}
      spacesLoading={targets.loading}
      spacesError={targets.error}
      spaceControl={
        props.fixedTaskListId ? null : (
          <label className="app-text-body-sm flex flex-col gap-2">
            {t('searchSpace.label')}
            <select
              className="app-field-input-sm"
              aria-label={t('searchSpace.label')}
              value={selectedKey}
              onChange={(event) => setSelectedKey(event.target.value)}
            >
              {!allowed ? (
                <option value={selectedKey} disabled>
                  {t('searchSpace.choose')}
                </option>
              ) : null}
              {choices.map((space) => (
                <option key={space.key} value={space.key}>
                  {space.name}
                </option>
              ))}
            </select>
          </label>
        )
      }
    />
  );
}

function TaskPickerContent({
  isOpen,
  onClose,
  onPick,
  excludeTaskIds = EMPTY_EXCLUDED_TASK_IDS,
  workspaceSlug,
  copy = DEFAULT_TASK_PICKER_MODAL_COPY,
  contentClassName,
  fixedTaskListId = null,
  layer,
  overlayClassName,
  canAccess,
  canLoad,
  spaceControl,
  spacesLoading,
  spacesError,
}: TaskPickerModalProps & {
  canAccess: boolean;
  canLoad: boolean;
  spaceControl: ReactNode;
  spacesLoading: boolean;
  spacesError: boolean;
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const {
    attachErrorKey,
    descriptionKey,
    emptyKey,
    loadListsErrorKey,
    loadTasksErrorKey,
    noAccessActionKey,
    noTaskListsKey,
    searchPlaceholderKey,
    taskListLabelKey,
    titleKey,
    workspaceLabelKey,
  } = copy;
  const [
    {
      taskLists,
      selectedTaskListId,
      tasks,
      query,
      loading,
      submittingId,
      error,
    },
    dispatch,
  ] = useReducer(taskPickerModalReducer, INITIAL_TASK_PICKER_MODAL_STATE);

  useEffect(() => {
    if (!isOpen || !api || !canLoad || !workspaceSlug) return;
    let cancelled = false;
    dispatch({ type: 'resetForOpen', selectedTaskListId: fixedTaskListId });
    if (fixedTaskListId) {
      return () => {
        cancelled = true;
      };
    }
    listPmsTaskLists(api, undefined, workspaceSlug)
      .then((response) => {
        if (cancelled) return;
        dispatch({
          type: 'taskListsLoaded',
          taskLists: response.items.filter((list) => !list.archived),
        });
      })
      .catch((err: Error) => {
        if (cancelled) return;
        dispatch({
          type: 'taskListsFailed',
          message: err.message || t(loadListsErrorKey),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [
    canLoad,
    fixedTaskListId,
    isOpen,
    loadListsErrorKey,
    t,
    api,
    workspaceSlug,
  ]);

  useEffect(() => {
    if (
      !isOpen ||
      !api ||
      !canLoad ||
      !workspaceSlug ||
      !selectedTaskListId
    ) {
      dispatch({ type: 'tasksIdle' });
      return;
    }
    let cancelled = false;
    dispatch({ type: 'tasksLoading' });
    listTaskListTasks(
      api,
      selectedTaskListId,
      buildTaskPickerTaskParams(query),
      workspaceSlug,
    )
      .then((response) => {
        if (cancelled) return;
        dispatch({ type: 'tasksLoaded', tasks: response.items });
      })
      .catch((err: Error) => {
        if (cancelled) return;
        dispatch({
          type: 'tasksFailed',
          message: err.message || t(loadTasksErrorKey),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [
    canLoad,
    isOpen,
    loadTasksErrorKey,
    query,
    selectedTaskListId,
    t,
    api,
    workspaceSlug,
  ]);

  const filteredTasks = useMemo(
    () => getVisibleTaskPickerTasks(tasks, excludeTaskIds),
    [excludeTaskIds, tasks],
  );

  async function handlePick(task: PmsTask) {
    dispatch({ type: 'pickStarted', taskId: task.id });
    try {
      await onPick(task);
      onClose();
    } catch (err) {
      dispatch({
        type: 'pickFailed',
        message: err instanceof Error ? err.message : t(attachErrorKey),
      });
    } finally {
      dispatch({ type: 'pickFinished' });
    }
  }

  return (
    <ResourcePickerDialog
      accessNotice={
        <NoAccessNotice
          workspaceLabel={t(workspaceLabelKey)}
          action={t(noAccessActionKey)}
        />
      }
      canAccess={canAccess}
      closeLabel={t('common:actions.close')}
      contentClassName={contentClassName}
      controlsSlot={
        <div className="space-y-3">
          {spaceControl}
          {fixedTaskListId || !canLoad ? null : (
            <label className="block space-y-1">
              <span className="app-text-control-sm text-app-ink/70">
                {t(taskListLabelKey)}
              </span>
              <select
                value={selectedTaskListId ?? ''}
                onChange={(event) =>
                  dispatch({
                    type: 'setSelectedTaskListId',
                    taskListId: event.target.value || null,
                  })
                }
                className="app-field-input"
              >
                {taskLists.length === 0 ? (
                  <option value="">{t(noTaskListsKey)}</option>
                ) : null}
                {taskLists.map((taskList) => (
                  <option key={taskList.id} value={taskList.id}>
                    {taskList.key} · {taskList.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
      }
      description={t(descriptionKey)}
      emptyLabel={t(emptyKey)}
      error={error || (spacesError ? t(loadListsErrorKey) : null)}
      getItemId={(task) => task.id}
      isOpen={isOpen}
      items={filteredTasks}
      layer={layer}
      loading={loading || spacesLoading}
      overlayClassName={overlayClassName}
      onClose={onClose}
      onPick={(task) => void handlePick(task)}
      renderItem={(task) => (
        <div className="min-w-0">
          <p className="app-text-body line-clamp-1 text-app-ink">
            {task.title}
          </p>
          <p className="app-text-caption text-app-ink/40">
            {task.reference} · {task.status_label}
          </p>
        </div>
      )}
      search={{
        label: t('common:actions.search'),
        onChange: (value) => dispatch({ type: 'setQuery', query: value }),
        placeholder: t(searchPlaceholderKey),
        value: query,
      }}
      submittingId={submittingId}
      title={t(titleKey)}
    />
  );
}
