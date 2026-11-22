import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowLeft,
  Check,
  Loader2,
  Pencil,
  Plus,
  Trash2,
  X,
} from 'lucide-react';
import { Button, useConfirm, useFeedback } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { BlockEditor, type BlockContent } from '../../../components/editor';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { apiErrorMessage } from '../../../platform/api/api-error';
import {
  createSpaceTaskTemplate,
  deleteTaskTemplate,
  listSpaceTaskTemplates,
  listTaskListLabels,
  updateTaskTemplate,
  type PmsLabel,
  type PmsTaskListStatus,
  type PmsTaskTemplate,
  type PmsTaskTemplateInput,
  type PmsTemplateLabel,
  type PmsTemplateSubtask,
} from '../api/pms-api';
import { getDefaultTaskStatus } from './pms-constants';
import {
  createTemplateDraft,
  isTemplateDraftDirty,
  templateDraftToInput,
  type TemplateDraft,
} from './template-settings-model';

const PRIORITIES = ['low', 'medium', 'high', 'critical'] as const;
const PRIORITY_LABEL_KEYS: Record<string, string> = {
  critical: 'pms.priorityCritical',
  high: 'pms.priorityHigh',
  low: 'pms.priorityLow',
  medium: 'pms.priorityMedium',
};
const INPUT_CLASS =
  'app-text-body min-w-0 rounded border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink focus:border-app-accent focus:outline-none';

/**
 * miy reinforcement: the task templates of a space (AI-DO kept templates per
 * list and had no screen to make them). Members, admins and owners write a
 * template by hand or edit one saved from a task; viewers read them.
 */
export function TemplateSettingsSection({
  spaceId,
  taskListId,
  canEdit,
  taskListStatuses = [],
}: {
  spaceId: string;
  /** Labels offered come from this list (labels are matched by name). */
  taskListId?: string | null;
  canEdit: boolean;
  taskListStatuses?: PmsTaskListStatus[];
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const feedback = useFeedback();
  const { confirm, confirmDialog } = useConfirm();
  const [templates, setTemplates] = useState<PmsTaskTemplate[]>([]);
  const [listLabels, setListLabels] = useState<PmsLabel[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editing, setEditing] = useState<{
    template: PmsTaskTemplate | null;
    draft: TemplateDraft;
    initial: TemplateDraft;
  } | null>(null);
  const [saving, setSaving] = useState(false);
  const loadRef = useRef(0);

  useNavigationDirty(
    saving || (editing !== null && isTemplateDraftDirty(editing.initial, editing.draft)),
  );

  const reload = useCallback(async () => {
    if (!api) return;
    const generation = loadRef.current + 1;
    loadRef.current = generation;
    setLoadError(null);
    try {
      const [templateResponse, labelResponse] = await Promise.all([
        listSpaceTaskTemplates(api, spaceId),
        taskListId
          ? listTaskListLabels(api, taskListId)
          : Promise.resolve({ items: [] as PmsLabel[] }),
      ]);
      if (loadRef.current !== generation) return;
      setTemplates(templateResponse.items);
      setListLabels(labelResponse.items);
    } catch (caught) {
      if (loadRef.current !== generation) return;
      setLoadError(
        apiErrorMessage(caught, t('pms.templateSettings.errors.loadFailed')),
      );
    } finally {
      if (loadRef.current === generation) setLoading(false);
    }
  }, [api, spaceId, t, taskListId]);

  useEffect(() => {
    setLoading(true);
    void reload();
  }, [reload]);

  const open = (template: PmsTaskTemplate | null) => {
    const draft = createTemplateDraft(
      template,
      getDefaultTaskStatus(taskListStatuses),
    );
    setEditing({ template, draft, initial: draft });
  };

  const handleSave = async () => {
    if (!api || !editing || saving) return;
    const input: PmsTaskTemplateInput = templateDraftToInput(editing.draft);
    if (!input.name) return;
    setSaving(true);
    try {
      if (editing.template) {
        await updateTaskTemplate(api, editing.template.id, input);
      } else {
        await createSpaceTaskTemplate(api, spaceId, input);
      }
      setEditing(null);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.templateSettings.errors.saveFailed')),
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (template: PmsTaskTemplate) => {
    if (!api) return;
    const confirmed = await confirm({
      title: t('pms.templateSettings.deleteTitle'),
      description: t('pms.templateSettings.deleteConfirm', {
        name: template.name,
      }),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!confirmed) return;
    try {
      await deleteTaskTemplate(api, template.id);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.templateSettings.errors.deleteFailed')),
      );
    }
  };

  if (editing) {
    return (
      <section className="flex h-full min-h-0 flex-col gap-3">
        <div className="flex items-center gap-2">
          <button
            type="button"
            aria-label={t('common:actions.cancel')}
            className="text-app-ink/50 hover:text-app-ink"
            onClick={() => setEditing(null)}
          >
            <ArrowLeft size={14} />
          </button>
          <h3 className="app-text-overline min-w-0 flex-1 truncate text-app-ink/50">
            {editing.template
              ? t('pms.templateSettings.edit', { name: editing.template.name })
              : t('pms.templateSettings.create')}
          </h3>
        </div>
        <TemplateEditor
          draft={editing.draft}
          listLabels={listLabels}
          onChange={(draft) => setEditing({ ...editing, draft })}
          taskListStatuses={taskListStatuses}
        />
        <div className="flex justify-end gap-2 border-t border-app-border pt-3">
          <Button onClick={() => setEditing(null)} variant="ghost">
            {t('common:actions.cancel')}
          </Button>
          <Button
            disabled={saving || !editing.draft.name.trim()}
            onClick={() => void handleSave()}
          >
            {saving ? <Loader2 className="animate-spin" size={14} /> : <Check size={14} />}
            {t('common:actions.save')}
          </Button>
        </div>
        {confirmDialog}
      </section>
    );
  }

  return (
    <section className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="app-text-overline text-app-ink/50">
            {t('pms.templateSettings.heading')}
          </h3>
          <p className="app-text-caption mt-1 text-app-ink/50">
            {canEdit
              ? t('pms.templateSettings.hint')
              : t('pms.templateSettings.readOnly')}
          </p>
        </div>
        {canEdit ? (
          <button
            type="button"
            className="app-control shrink-0"
            onClick={() => open(null)}
          >
            <Plus size={14} />
            {t('pms.templateSettings.create')}
          </button>
        ) : null}
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
          {templates.length === 0 ? (
            <p className="app-text-caption py-4 text-center text-app-ink/45">
              {t('pms.templateSettings.empty')}
            </p>
          ) : null}
          {templates.map((template) => (
            <div
              key={template.id}
              className="group flex min-h-11 items-center gap-2 rounded-md border border-app-border bg-app-surface-sidebar px-2.5 py-1.5"
            >
              <div className="min-w-0 flex-1">
                <div className="app-text-body-sm truncate font-medium text-app-ink">
                  {template.name}
                </div>
                <div className="app-text-micro truncate text-app-ink/45">
                  {t('pms.templateSettings.summary', {
                    subtasks: template.subtask_count ?? 0,
                    checklist: template.checklist_count ?? 0,
                  })}
                  {template.labels?.length
                    ? ` · ${template.labels.map((label) => label.name).join(', ')}`
                    : ''}
                </div>
              </div>
              {canEdit ? (
                <>
                  <button
                    type="button"
                    aria-label={t('pms.templateSettings.edit', {
                      name: template.name,
                    })}
                    className="text-app-ink/40 hover:text-app-ink"
                    onClick={() => open(template)}
                  >
                    <Pencil size={13} />
                  </button>
                  <button
                    type="button"
                    aria-label={t('pms.templateSettings.delete', {
                      name: template.name,
                    })}
                    className="text-app-ink/40 hover:text-app-danger-text"
                    onClick={() => void handleDelete(template)}
                  >
                    <Trash2 size={13} />
                  </button>
                </>
              ) : null}
            </div>
          ))}
        </div>
      )}
      {confirmDialog}
    </section>
  );
}

function TemplateEditor({
  draft,
  listLabels,
  onChange,
  taskListStatuses,
}: {
  draft: TemplateDraft;
  listLabels: PmsLabel[];
  onChange: (draft: TemplateDraft) => void;
  taskListStatuses: PmsTaskListStatus[];
}) {
  const { t } = useTranslation('apps');
  const [checklistInput, setChecklistInput] = useState('');
  const [subtaskInput, setSubtaskInput] = useState('');
  const set = (patch: Partial<TemplateDraft>) => onChange({ ...draft, ...patch });
  const labelKeys = new Set(draft.labels.map((label) => label.name.toLowerCase()));
  const toggleLabel = (label: PmsTemplateLabel) => {
    const key = label.name.toLowerCase();
    set({
      labels: labelKeys.has(key)
        ? draft.labels.filter((item) => item.name.toLowerCase() !== key)
        : [...draft.labels, { name: label.name, color: label.color }],
    });
  };
  const otherLabels = draft.labels.filter(
    (label) =>
      !listLabels.some((item) => item.name.toLowerCase() === label.name.toLowerCase()),
  );
  const statusOptions = taskListStatuses.length
    ? taskListStatuses.map((status) => ({ value: status.slug, label: status.name }))
    : [{ value: draft.defaultStatus, label: draft.defaultStatus }];
  if (!statusOptions.some((option) => option.value === draft.defaultStatus)) {
    statusOptions.unshift({ value: draft.defaultStatus, label: draft.defaultStatus });
  }

  return (
    <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1 custom-scrollbar">
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="grid gap-1">
          <span className="app-text-caption text-app-ink/50">
            {t('pms.templateSettings.name')}
          </span>
          <input
            className={INPUT_CLASS}
            maxLength={140}
            onChange={(event) => set({ name: event.target.value })}
            value={draft.name}
          />
        </label>
        <label className="grid gap-1">
          <span className="app-text-caption text-app-ink/50">
            {t('pms.templateSettings.taskTitle')}
          </span>
          <input
            className={INPUT_CLASS}
            maxLength={180}
            onChange={(event) => set({ title: event.target.value })}
            placeholder={t('pms.templateSettings.taskTitleHint')}
            value={draft.title}
          />
        </label>
        <label className="grid gap-1">
          <span className="app-text-caption text-app-ink/50">
            {t('pms.templateSettings.priority')}
          </span>
          <select
            className="app-field-input-sm"
            onChange={(event) => set({ defaultPriority: event.target.value })}
            value={draft.defaultPriority}
          >
            {PRIORITIES.map((priority) => (
              <option key={priority} value={priority}>
                {t(PRIORITY_LABEL_KEYS[priority])}
              </option>
            ))}
          </select>
        </label>
        <label className="grid gap-1">
          <span className="app-text-caption text-app-ink/50">
            {t('pms.templateSettings.status')}
          </span>
          <select
            className="app-field-input-sm"
            onChange={(event) => set({ defaultStatus: event.target.value })}
            value={draft.defaultStatus}
          >
            {statusOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="grid gap-1">
        <span className="app-text-caption text-app-ink/50">
          {t('pms.templateSettings.description')}
        </span>
        <div className="overflow-hidden rounded-lg border border-app-border">
          <BlockEditor
            initialContent={draft.descriptionBlocks as BlockContent | undefined}
            onChange={(blocks) =>
              set({
                descriptionBlocks: blocks as unknown as Record<string, unknown>[],
              })
            }
            placeholder={t('pms.descriptionPlaceholder')}
            className="[&_.bn-editor]:min-h-[80px] [&_.bn-editor]:px-3 [&_.bn-editor]:py-2"
          />
        </div>
      </div>

      <div className="grid gap-1.5">
        <span className="app-text-caption text-app-ink/50">
          {t('pms.templateSettings.checklist')}
        </span>
        {draft.checklist.map((item, index) => (
          <div key={index} className="flex items-center gap-2">
            <input
              aria-label={t('pms.templateSettings.checklist')}
              className={`${INPUT_CLASS} flex-1`}
              maxLength={500}
              onChange={(event) =>
                set({
                  checklist: draft.checklist.map((text, itemIndex) =>
                    itemIndex === index ? event.target.value : text,
                  ),
                })
              }
              value={item}
            />
            <button
              type="button"
              aria-label={t('pms.templateSettings.removeItem', { name: item })}
              className="text-app-ink/40 hover:text-app-danger-text"
              onClick={() =>
                set({
                  checklist: draft.checklist.filter(
                    (_, itemIndex) => itemIndex !== index,
                  ),
                })
              }
            >
              <X size={12} />
            </button>
          </div>
        ))}
        <input
          aria-label={t('pms.templateSettings.checklistPlaceholder')}
          className={INPUT_CLASS}
          maxLength={500}
          onChange={(event) => setChecklistInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
              event.preventDefault();
              const text = checklistInput.trim();
              if (!text) return;
              set({ checklist: [...draft.checklist, text] });
              setChecklistInput('');
            }
          }}
          placeholder={t('pms.templateSettings.checklistPlaceholder')}
          value={checklistInput}
        />
      </div>

      <div className="grid gap-1.5">
        <span className="app-text-caption text-app-ink/50">
          {t('pms.templateSettings.labels')}
        </span>
        <div className="flex flex-wrap gap-1.5">
          {listLabels.map((label) => {
            const selected = labelKeys.has(label.name.toLowerCase());
            return (
              <button
                aria-pressed={selected}
                className={`app-text-caption inline-flex items-center gap-1 rounded px-2 py-0.5 font-medium ${
                  selected ? 'text-white' : 'border border-app-border text-app-ink/60'
                }`}
                key={label.id}
                onClick={() => toggleLabel(label)}
                style={selected ? { backgroundColor: label.color } : undefined}
                type="button"
              >
                {selected ? <Check size={10} /> : null}
                {label.name}
              </button>
            );
          })}
          {otherLabels.map((label) => (
            <button
              aria-pressed
              className="app-text-caption inline-flex items-center gap-1 rounded px-2 py-0.5 font-medium text-white"
              key={label.name}
              onClick={() => toggleLabel(label)}
              style={{ backgroundColor: label.color }}
              type="button"
            >
              <X size={10} />
              {label.name}
            </button>
          ))}
        </div>
        <p className="app-text-micro text-app-ink/40">
          {t('pms.templateSettings.labelsHint')}
        </p>
      </div>

      <div className="grid gap-1.5">
        <span className="app-text-caption text-app-ink/50">
          {t('pms.templateSettings.subtasks')}
        </span>
        {draft.subtasks.map((subtask, index) => (
          <SubtaskRow
            key={index}
            onChange={(next) =>
              set({
                subtasks: draft.subtasks.map((item, itemIndex) =>
                  itemIndex === index ? next : item,
                ),
              })
            }
            onRemove={() =>
              set({
                subtasks: draft.subtasks.filter((_, itemIndex) => itemIndex !== index),
              })
            }
            subtask={subtask}
          />
        ))}
        <input
          aria-label={t('pms.templateSettings.subtaskPlaceholder')}
          className={INPUT_CLASS}
          maxLength={180}
          onChange={(event) => setSubtaskInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
              event.preventDefault();
              const title = subtaskInput.trim();
              if (title.length < 2) return;
              set({
                subtasks: [
                  ...draft.subtasks,
                  {
                    title,
                    description: '',
                    description_blocks: null,
                    priority: 'medium',
                    checklist_items: [],
                    labels: [],
                  },
                ],
              });
              setSubtaskInput('');
            }
          }}
          placeholder={t('pms.templateSettings.subtaskPlaceholder')}
          value={subtaskInput}
        />
      </div>
    </div>
  );
}

function SubtaskRow({
  onChange,
  onRemove,
  subtask,
}: {
  onChange: (subtask: PmsTemplateSubtask) => void;
  onRemove: () => void;
  subtask: PmsTemplateSubtask;
}) {
  const { t } = useTranslation('apps');
  const details = subtask.checklist_items.length + subtask.labels.length > 0;
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1.5">
      <input
        aria-label={t('pms.templateSettings.subtasks')}
        className={`${INPUT_CLASS} min-w-[10rem] flex-1`}
        maxLength={180}
        onChange={(event) => onChange({ ...subtask, title: event.target.value })}
        value={subtask.title}
      />
      <select
        aria-label={t('pms.templateSettings.priority')}
        className="app-field-input-sm max-w-28"
        onChange={(event) => onChange({ ...subtask, priority: event.target.value })}
        value={subtask.priority}
      >
        {PRIORITIES.map((priority) => (
          <option key={priority} value={priority}>
            {t(PRIORITY_LABEL_KEYS[priority])}
          </option>
        ))}
      </select>
      {details ? (
        <span className="app-text-micro text-app-ink/45">
          {t('pms.templateSettings.subtaskDetails', {
            checklist: subtask.checklist_items.length,
            labels: subtask.labels.length,
          })}
        </span>
      ) : null}
      <button
        type="button"
        aria-label={t('pms.templateSettings.removeItem', { name: subtask.title })}
        className="ml-auto text-app-ink/40 hover:text-app-danger-text"
        onClick={onRemove}
      >
        <X size={12} />
      </button>
    </div>
  );
}
