import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, Loader2, Pencil, Plus, Trash2, X } from 'lucide-react';
import { useConfirm, useFeedback } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { DateInput } from '../../../components/date/DateInput';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { apiErrorMessage } from '../../../platform/api/api-error';
import {
  createSpaceMilestone,
  createTaskListMilestone,
  deleteMilestone,
  listTaskListMilestones,
  updateMilestone,
  type PmsMilestone,
  type PmsMilestoneStatus,
} from '../api/pms-api';
import { formatDate } from './pms-constants';

const MILESTONE_STATUSES: readonly PmsMilestoneStatus[] = [
  'planned',
  'active',
  'complete',
];
const INPUT_CLASS =
  'app-text-body min-w-0 rounded border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink focus:border-app-accent focus:outline-none';
const DATE_CLASS =
  'app-text-caption min-w-[7.5rem] rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink';

type MilestoneDraft = {
  title: string;
  status: PmsMilestoneStatus;
  startDate: string;
  dueDate: string;
};

const EMPTY_DRAFT: MilestoneDraft = {
  title: '',
  status: 'planned',
  startDate: '',
  dueDate: '',
};

function asStatus(value: string): PmsMilestoneStatus {
  return (MILESTONE_STATUSES as readonly string[]).includes(value)
    ? (value as PmsMilestoneStatus)
    : 'planned';
}

/**
 * miy reinforcement: milestones of a list and of its whole space (AI-DO had
 * milestones per list but no screen to make them). Space owners and admins
 * create, edit and delete; drafts stay while lists reload.
 */
export function MilestoneSettingsSection({
  taskListId,
  spaceId,
  canManage,
  onMilestonesChanged,
}: {
  taskListId: string;
  spaceId: string | null;
  canManage: boolean;
  onMilestonesChanged?: (milestones: PmsMilestone[]) => void;
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const feedback = useFeedback();
  const { confirm, confirmDialog } = useConfirm();
  const [milestones, setMilestones] = useState<PmsMilestone[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [draft, setDraft] = useState<MilestoneDraft>(EMPTY_DRAFT);
  const [scope, setScope] = useState<'list' | 'space'>('list');
  const [creating, setCreating] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState<MilestoneDraft>(EMPTY_DRAFT);
  const [saving, setSaving] = useState(false);
  const loadRef = useRef(0);
  const onChangedRef = useRef(onMilestonesChanged);
  onChangedRef.current = onMilestonesChanged;

  useNavigationDirty(
    draft.title.trim() !== '' || editingId !== null || creating || saving,
  );

  const publish = useCallback((next: PmsMilestone[]) => {
    setMilestones(next);
    onChangedRef.current?.(next);
  }, []);

  const reload = useCallback(async () => {
    if (!api) return;
    const generation = loadRef.current + 1;
    loadRef.current = generation;
    setLoadError(null);
    try {
      const response = await listTaskListMilestones(api, taskListId);
      if (loadRef.current !== generation) return;
      publish(response.items);
    } catch (caught) {
      if (loadRef.current !== generation) return;
      setLoadError(
        apiErrorMessage(caught, t('pms.milestoneSettings.errors.loadFailed')),
      );
    } finally {
      if (loadRef.current === generation) setLoading(false);
    }
  }, [api, publish, t, taskListId]);

  useEffect(() => {
    setLoading(true);
    void reload();
  }, [reload]);

  const handleCreate = async () => {
    const title = draft.title.trim();
    if (!api || !title || creating) return;
    setCreating(true);
    try {
      const payload = {
        title,
        status: draft.status,
        start_date: draft.startDate || null,
        due_date: draft.dueDate || null,
      };
      if (scope === 'space' && spaceId) {
        await createSpaceMilestone(api, spaceId, payload);
      } else {
        await createTaskListMilestone(api, taskListId, payload);
      }
      setDraft(EMPTY_DRAFT);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.milestoneSettings.errors.createFailed')),
      );
    } finally {
      setCreating(false);
    }
  };

  const startEdit = (milestone: PmsMilestone) => {
    setEditingId(milestone.id);
    setEditDraft({
      title: milestone.title,
      status: asStatus(milestone.status),
      startDate: milestone.start_date ?? '',
      dueDate: milestone.due_date ?? '',
    });
  };

  const handleSaveEdit = async () => {
    if (!api || !editingId || saving) return;
    const title = editDraft.title.trim();
    if (title.length < 2) return;
    setSaving(true);
    try {
      await updateMilestone(api, editingId, {
        title,
        status: editDraft.status,
        start_date: editDraft.startDate || null,
        due_date: editDraft.dueDate || null,
      });
      setEditingId(null);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.milestoneSettings.errors.updateFailed')),
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (milestone: PmsMilestone) => {
    if (!api) return;
    const confirmed = await confirm({
      title: t('pms.milestoneSettings.deleteTitle'),
      description: t('pms.milestoneSettings.deleteConfirm', {
        name: milestone.title,
      }),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!confirmed) return;
    try {
      await deleteMilestone(api, milestone.id);
      if (editingId === milestone.id) setEditingId(null);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.milestoneSettings.errors.deleteFailed')),
      );
    }
  };

  const statusSelect = (
    value: PmsMilestoneStatus,
    onChange: (value: PmsMilestoneStatus) => void,
  ) => (
    <select
      aria-label={t('pms.milestoneSettings.statusLabel')}
      className="app-field-input-sm max-w-28 shrink-0"
      onChange={(event) => onChange(asStatus(event.target.value))}
      value={value}
    >
      {MILESTONE_STATUSES.map((status) => (
        <option key={status} value={status}>
          {t(`pms.milestoneSettings.status.${status}`)}
        </option>
      ))}
    </select>
  );

  return (
    <section className="flex h-full min-h-0 flex-col gap-3">
      <div>
        <h3 className="app-text-overline text-app-ink/50">
          {t('pms.milestoneSettings.heading')}
        </h3>
        <p className="app-text-caption mt-1 text-app-ink/50">
          {canManage
            ? t('pms.milestoneSettings.hint')
            : t('pms.milestoneSettings.readOnly')}
        </p>
      </div>
      {loadError ? (
        <p className="app-text-caption text-app-danger-text" role="alert">
          {loadError}
        </p>
      ) : null}
      {loading ? (
        <div className="flex justify-center py-6">
          <Loader2 size={18} className="animate-spin text-app-ink/40" />
        </div>
      ) : (
        <div className="min-h-0 flex-1 space-y-1.5 overflow-y-auto pr-1 custom-scrollbar">
          {milestones.length === 0 ? (
            <p className="app-text-caption py-4 text-center text-app-ink/45">
              {t('pms.milestoneSettings.empty')}
            </p>
          ) : null}
          {milestones.map((milestone) =>
            editingId === milestone.id ? (
              <div
                key={milestone.id}
                className="flex flex-wrap items-center gap-2 rounded-md border border-app-accent/40 bg-app-surface-sidebar px-2.5 py-2"
              >
                <input
                  aria-label={t('pms.milestoneSettings.edit', {
                    name: milestone.title,
                  })}
                  className={`${INPUT_CLASS} flex-1`}
                  maxLength={140}
                  onChange={(event) =>
                    setEditDraft({ ...editDraft, title: event.target.value })
                  }
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.nativeEvent.isComposing)
                      void handleSaveEdit();
                    if (event.key === 'Escape') setEditingId(null);
                  }}
                  value={editDraft.title}
                />
                {statusSelect(editDraft.status, (status) =>
                  setEditDraft({ ...editDraft, status }),
                )}
                <DateInput
                  aria-label={t('pms.milestoneSettings.startDate')}
                  className={DATE_CLASS}
                  onValueChange={(value) =>
                    setEditDraft({ ...editDraft, startDate: value })
                  }
                  value={editDraft.startDate}
                />
                <DateInput
                  aria-label={t('pms.milestoneSettings.dueDate')}
                  className={DATE_CLASS}
                  onValueChange={(value) =>
                    setEditDraft({ ...editDraft, dueDate: value })
                  }
                  value={editDraft.dueDate}
                />
                <button
                  type="button"
                  aria-label={t('common:actions.save')}
                  className="text-app-accent hover:opacity-80 disabled:opacity-40"
                  disabled={saving || editDraft.title.trim().length < 2}
                  onClick={() => void handleSaveEdit()}
                >
                  {saving ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : (
                    <Check size={14} />
                  )}
                </button>
                <button
                  type="button"
                  aria-label={t('common:actions.cancel')}
                  className="text-app-ink/40 hover:text-app-ink"
                  onClick={() => setEditingId(null)}
                >
                  <X size={14} />
                </button>
              </div>
            ) : (
              <MilestoneRow
                key={milestone.id}
                canManage={canManage}
                milestone={milestone}
                onDelete={() => void handleDelete(milestone)}
                onEdit={() => startEdit(milestone)}
              />
            ),
          )}
        </div>
      )}
      {canManage ? (
        <div className="grid gap-2 rounded-lg border border-app-border bg-app-surface-sidebar p-3">
          <div className="flex flex-wrap items-center gap-2">
            <input
              aria-label={t('pms.milestoneSettings.titlePlaceholder')}
              className={`${INPUT_CLASS} flex-1`}
              maxLength={140}
              onChange={(event) =>
                setDraft({ ...draft, title: event.target.value })
              }
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing)
                  void handleCreate();
              }}
              placeholder={t('pms.milestoneSettings.titlePlaceholder')}
              value={draft.title}
            />
            <select
              aria-label={t('pms.milestoneSettings.scopeLabel')}
              className="app-field-input-sm max-w-36 shrink-0"
              disabled={!spaceId}
              onChange={(event) =>
                setScope(event.target.value === 'space' ? 'space' : 'list')
              }
              value={scope}
            >
              <option value="list">{t('pms.milestoneSettings.scopeList')}</option>
              <option value="space">
                {t('pms.milestoneSettings.scopeSpace')}
              </option>
            </select>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {statusSelect(draft.status, (status) => setDraft({ ...draft, status }))}
            <DateInput
              aria-label={t('pms.milestoneSettings.startDate')}
              className={DATE_CLASS}
              onValueChange={(value) => setDraft({ ...draft, startDate: value })}
              value={draft.startDate}
            />
            <DateInput
              aria-label={t('pms.milestoneSettings.dueDate')}
              className={DATE_CLASS}
              onValueChange={(value) => setDraft({ ...draft, dueDate: value })}
              value={draft.dueDate}
            />
            <button
              type="button"
              className="app-control ml-auto"
              disabled={creating || draft.title.trim().length < 2}
              onClick={() => void handleCreate()}
            >
              {creating ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Plus size={14} />
              )}
              {t('pms.milestoneSettings.add')}
            </button>
          </div>
        </div>
      ) : null}
      {confirmDialog}
    </section>
  );
}

function MilestoneRow({
  canManage,
  milestone,
  onDelete,
  onEdit,
}: {
  canManage: boolean;
  milestone: PmsMilestone;
  onDelete: () => void;
  onEdit: () => void;
}) {
  const { t } = useTranslation('apps');
  const percent = Math.round((milestone.progress ?? 0) * 100);
  return (
    <div className="group flex min-h-11 items-center gap-3 rounded-md border border-app-border bg-app-surface-sidebar px-2.5 py-1.5">
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span className="app-text-body-sm min-w-0 truncate font-medium text-app-ink">
            {milestone.title}
          </span>
          <span className="app-text-micro shrink-0 rounded bg-app-surface-hover px-1.5 py-0.5 text-app-ink/60">
            {t(`pms.milestoneSettings.status.${asStatus(milestone.status)}`)}
          </span>
          {milestone.scope === 'space' ? (
            <span className="app-text-micro shrink-0 rounded bg-app-accent/10 px-1.5 py-0.5 text-app-accent">
              {t('pms.milestoneSettings.spaceBadge')}
            </span>
          ) : null}
        </div>
        <div className="app-text-micro mt-1 flex items-center gap-2 text-app-ink/45">
          <span>
            {milestone.due_date
              ? [milestone.start_date, milestone.due_date]
                  .filter(Boolean)
                  .map((date) => formatDate(date))
                  .join(' – ')
              : t('pms.milestoneSettings.noDueDate')}
          </span>
          <span
            aria-hidden
            className="h-1 w-16 overflow-hidden rounded-full bg-app-border"
          >
            <span
              className="block h-full bg-app-accent"
              style={{ width: `${percent}%` }}
            />
          </span>
          <span>
            {t('pms.milestoneSettings.progress', {
              done: milestone.completed_task_count,
              total: milestone.task_count,
            })}
          </span>
        </div>
      </div>
      {canManage ? (
        <>
          <button
            type="button"
            aria-label={t('pms.milestoneSettings.edit', { name: milestone.title })}
            className="text-app-ink/40 opacity-0 transition-all hover:text-app-ink focus-visible:opacity-100 group-hover:opacity-100"
            onClick={onEdit}
          >
            <Pencil size={13} />
          </button>
          <button
            type="button"
            aria-label={t('pms.milestoneSettings.delete', {
              name: milestone.title,
            })}
            className="text-app-ink/40 opacity-0 transition-all hover:text-app-danger-text focus-visible:opacity-100 group-hover:opacity-100"
            onClick={onDelete}
          >
            <Trash2 size={13} />
          </button>
        </>
      ) : null}
    </div>
  );
}
