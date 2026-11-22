import { useEffect, useId, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ContextNote, useFeedback } from '@miy/ui';

import { ResourcePickerDialog } from '../../../components/picker/ResourcePickerDialog';
import { NoAccessNotice } from '../../../components/common/NoAccessNotice';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import { useAppTargetSpaces } from '../../../platform/app-runtime/use-app-target-spaces';
import {
  copyTasksToList,
  listAllPmsTaskLists,
  type PmsTaskList,
  type TaskCopyResult,
} from '../api/pms-api';
import {
  buildTaskCopyTargets,
  filterTaskCopyTargets,
  type TaskCopyTarget,
} from './task-copy-model';

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

interface TaskCopyDialogProps {
  workspaceSlug?: string | null;
  isOpen: boolean;
  onClose: () => void;
  onCopied: (result: TaskCopyResult, target: TaskCopyTarget) => void;
  sourceTaskListId: string;
  taskIds: readonly string[];
}

export function TaskCopyDialog(props: TaskCopyDialogProps) {
  const user = usePmsCurrentUser();
  return (
    <TaskCopyContent
      key={`${user?.id ?? ''}:${props.workspaceSlug ?? ''}:${props.sourceTaskListId}`}
      {...props}
    />
  );
}

function TaskCopyContent({
  workspaceSlug,
  isOpen,
  onClose,
  onCopied,
  sourceTaskListId,
  taskIds,
}: TaskCopyDialogProps) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const targetSpaces = useAppTargetSpaces('pms');
  const catalogKey = targetSpaces.choices.find(
    (space) => space.kind === 'app_global',
  )?.key;
  const feedback = useFeedback();
  const resetProgressId = useId();
  const [taskLists, setTaskLists] = useState<PmsTaskList[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [resetProgress, setResetProgress] = useState(false);
  const [submittingId, setSubmittingId] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen || !api || !catalogKey) return undefined;
    let cancelled = false;
    setLoading(true);
    setLoadError(null);
    setQuery('');
    setResetProgress(false);
    listAllPmsTaskLists(api, undefined, catalogKey)
      .then((response) => {
        if (!cancelled) setTaskLists(response.items);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setLoadError(errorMessage(error, t('pms.bulk.copy.loadFailed')));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [catalogKey, isOpen, t, api]);

  const targets = useMemo(
    () =>
      filterTaskCopyTargets(
        buildTaskCopyTargets(taskLists, sourceTaskListId),
        query,
      ),
    [query, sourceTaskListId, taskLists],
  );

  async function handlePick(target: TaskCopyTarget) {
    if (
      !api ||
      !workspaceSlug ||
      !catalogKey ||
      taskIds.length === 0 ||
      submittingId !== null
    )
      return;
    setSubmittingId(target.id);
    try {
      const result = await copyTasksToList(
        api,
        sourceTaskListId,
        {
          task_ids: [...taskIds],
          target_list_id: target.id,
          reset_progress: resetProgress,
        },
        workspaceSlug,
      );
      onCopied(result, target);
      onClose();
    } catch (error) {
      feedback.error(errorMessage(error, t('pms.bulk.copy.failed')));
    } finally {
      setSubmittingId(null);
    }
  }

  return (
    <ResourcePickerDialog
      canAccess={Boolean(catalogKey)}
      accessNotice={
        <NoAccessNotice
          workspaceLabel={t('pms.taskPicker.pmsWorkspace')}
          action={t('pms.bulk.copy.title')}
        />
      }
      closeLabel={t('common:actions.close')}
      controlsSlot={
        <div className="space-y-3">
          <ContextNote>{t('pms.bulk.copy.note')}</ContextNote>
          <div className="flex items-start gap-2">
            <input
              id={resetProgressId}
              type="checkbox"
              checked={resetProgress}
              disabled={submittingId !== null}
              onChange={(event) => setResetProgress(event.target.checked)}
              className="mt-1 size-3.5 shrink-0 accent-app-accent"
            />
            <label htmlFor={resetProgressId} className="min-w-0">
              <span className="app-text-control-sm block text-app-ink">
                {t('pms.bulk.copy.resetProgress')}
              </span>
              <span className="app-text-caption block text-app-ink-muted">
                {t('pms.bulk.copy.resetProgressHint')}
              </span>
            </label>
          </div>
        </div>
      }
      description={t('pms.bulk.copy.description', { count: taskIds.length })}
      emptyLabel={t('pms.bulk.copy.empty')}
      error={loadError}
      getItemId={(target) => target.id}
      isOpen={isOpen}
      items={targets}
      layer="elevated"
      loading={loading}
      onClose={onClose}
      onPick={(target) => void handlePick(target)}
      renderItem={(target) => (
        <div className="min-w-0">
          <p className="app-text-body line-clamp-1 text-app-ink">
            {target.name}
            {target.isCurrent ? (
              <span className="app-text-caption ml-2 text-app-ink-muted">
                {t('pms.bulk.copy.currentList')}
              </span>
            ) : null}
          </p>
          <p className="app-text-caption line-clamp-1 text-app-ink/40">
            {target.spaceName ? `${target.spaceName} · ` : ''}
            {target.key}
          </p>
        </div>
      )}
      search={{
        label: t('common:actions.search'),
        onChange: setQuery,
        placeholder: t('pms.bulk.copy.searchPlaceholder'),
        value: query,
      }}
      submittingId={submittingId}
      title={t('pms.bulk.copy.title')}
    />
  );
}
