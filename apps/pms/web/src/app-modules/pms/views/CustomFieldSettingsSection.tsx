import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowDown,
  ArrowUp,
  Check,
  Loader2,
  Pencil,
  Plus,
  Trash2,
  X,
} from 'lucide-react';
import { useConfirm, useFeedback } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { apiErrorMessage } from '../../../platform/api/api-error';
import {
  createCustomField,
  deleteCustomField,
  listCustomFields,
  reorderCustomFields,
  updateCustomField,
  type PmsCustomField,
  type PmsCustomFieldType,
} from '../api/pms-api';
import {
  buildOptionChange,
  CUSTOM_FIELD_TYPES,
  customFieldHasOptions,
  customFieldTypeLabelKey,
  moveCustomField,
  removedOptions,
  type CustomFieldOptionDraft,
} from './custom-field-model';

const INPUT_CLASS =
  'app-text-body min-w-0 rounded border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink focus:border-app-accent focus:outline-none';

/** Options typed one by one: Enter adds, each chip can be renamed or removed. */
function OptionsEditor({
  drafts,
  onChange,
}: {
  drafts: CustomFieldOptionDraft[];
  onChange: (drafts: CustomFieldOptionDraft[]) => void;
}) {
  const { t } = useTranslation('apps');
  const [newOption, setNewOption] = useState('');
  const add = () => {
    const name = newOption.trim();
    if (!name) return;
    if (drafts.some((draft) => draft.name.trim().toLowerCase() === name.toLowerCase()))
      return;
    onChange([...drafts, { original: null, name }]);
    setNewOption('');
  };
  return (
    <div className="grid gap-1.5">
      <span className="app-text-caption text-app-ink/50">
        {t('pms.customFields.options')}
      </span>
      <div className="flex flex-wrap gap-1.5">
        {drafts.map((draft, index) => (
          <span
            key={`${draft.original ?? 'new'}-${index}`}
            className="inline-flex items-center gap-1 rounded-full border border-app-border bg-app-bg px-2 py-0.5"
          >
            <input
              aria-label={t('pms.customFields.renameOption', {
                name: draft.original ?? draft.name,
              })}
              className="app-text-caption w-24 bg-transparent text-app-ink focus:outline-none"
              maxLength={100}
              onChange={(event) =>
                onChange(
                  drafts.map((item, itemIndex) =>
                    itemIndex === index
                      ? { ...item, name: event.target.value }
                      : item,
                  ),
                )
              }
              value={draft.name}
            />
            <button
              type="button"
              aria-label={t('pms.customFields.removeOption', {
                name: draft.name,
              })}
              className="text-app-ink/40 hover:text-app-danger-text"
              onClick={() =>
                onChange(drafts.filter((_, itemIndex) => itemIndex !== index))
              }
            >
              <X size={11} />
            </button>
          </span>
        ))}
      </div>
      <div className="flex items-center gap-2">
        <input
          aria-label={t('pms.customFields.optionsPlaceholder')}
          className={`${INPUT_CLASS} flex-1`}
          maxLength={100}
          onChange={(event) => setNewOption(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
              event.preventDefault();
              add();
            }
          }}
          placeholder={t('pms.customFields.optionsPlaceholder')}
          value={newOption}
        />
        <button
          type="button"
          aria-label={t('pms.customFields.addOption')}
          className="text-app-accent hover:opacity-80 disabled:opacity-40"
          disabled={!newOption.trim()}
          onClick={add}
        >
          <Plus size={14} />
        </button>
      </div>
    </div>
  );
}

/**
 * miy reinforcement: the list's custom fields (AI-DO had the data but no
 * screen). Space owners and admins add, rename, reorder and delete fields and
 * edit the options of select fields; renaming an option keeps the values.
 */
export function CustomFieldSettingsSection({
  taskListId,
  canManage,
  onFieldsChanged,
}: {
  taskListId: string;
  canManage: boolean;
  onFieldsChanged?: (fields: PmsCustomField[]) => void;
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const feedback = useFeedback();
  const { confirm, confirmDialog } = useConfirm();
  const [fields, setFields] = useState<PmsCustomField[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [newName, setNewName] = useState('');
  const [newType, setNewType] = useState<PmsCustomFieldType>('text');
  const [newOptions, setNewOptions] = useState<CustomFieldOptionDraft[]>([]);
  const [creating, setCreating] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editOptions, setEditOptions] = useState<CustomFieldOptionDraft[]>([]);
  const [busy, setBusy] = useState(false);
  const loadRef = useRef(0);
  const onChangedRef = useRef(onFieldsChanged);
  onChangedRef.current = onFieldsChanged;

  useNavigationDirty(
    newName.trim() !== '' || editingId !== null || creating || busy,
  );

  const publish = useCallback((next: PmsCustomField[]) => {
    setFields(next);
    onChangedRef.current?.(next);
  }, []);

  const reload = useCallback(async () => {
    if (!api) return;
    const generation = loadRef.current + 1;
    loadRef.current = generation;
    setLoadError(null);
    try {
      const response = await listCustomFields(api, taskListId);
      if (loadRef.current !== generation) return;
      publish(response.items);
    } catch (caught) {
      if (loadRef.current !== generation) return;
      setLoadError(
        apiErrorMessage(caught, t('pms.customFields.errors.loadFailed')),
      );
    } finally {
      if (loadRef.current === generation) setLoading(false);
    }
  }, [api, publish, t, taskListId]);

  useEffect(() => {
    setLoading(true);
    void reload();
  }, [reload]);

  const newNeedsOptions = customFieldHasOptions(newType);
  const canCreate =
    newName.trim() !== '' &&
    (!newNeedsOptions || newOptions.some((draft) => draft.name.trim()));

  const handleCreate = async () => {
    if (!api || !canCreate || creating) return;
    setCreating(true);
    try {
      await createCustomField(api, taskListId, {
        name: newName.trim(),
        field_type: newType,
        ...(newNeedsOptions
          ? { options: buildOptionChange(newOptions).options }
          : {}),
      });
      setNewName('');
      setNewOptions([]);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.customFields.errors.createFailed')),
      );
    } finally {
      setCreating(false);
    }
  };

  const startEdit = (field: PmsCustomField) => {
    setEditingId(field.id);
    setEditName(field.name);
    setEditOptions(
      (field.options ?? []).map((option) => ({ original: option, name: option })),
    );
  };

  const handleSaveEdit = async (field: PmsCustomField) => {
    if (!api || busy) return;
    const name = editName.trim();
    if (!name) return;
    const withOptions = customFieldHasOptions(field.field_type);
    const change = buildOptionChange(editOptions);
    if (withOptions && change.options.length === 0) {
      feedback.error(t('pms.customFields.optionsRequired'));
      return;
    }
    if (withOptions && removedOptions(field.options ?? [], editOptions).length > 0) {
      const confirmed = await confirm({
        title: t('pms.customFields.optionsRemovedTitle'),
        description: t('pms.customFields.optionsRemovedConfirm'),
        confirmLabel: t('common:actions.save'),
        cancelLabel: t('common:actions.cancel'),
        variant: 'danger',
      });
      if (!confirmed) return;
    }
    setBusy(true);
    try {
      await updateCustomField(api, field.id, {
        name,
        ...(withOptions
          ? { options: change.options, option_renames: change.renames }
          : {}),
      });
      setEditingId(null);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.customFields.errors.updateFailed')),
      );
    } finally {
      setBusy(false);
    }
  };

  const handleMove = async (fieldId: string, offset: -1 | 1) => {
    if (!api || busy) return;
    const order = moveCustomField(fields, fieldId, offset);
    setBusy(true);
    try {
      const response = await reorderCustomFields(api, taskListId, order);
      publish(response.items);
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.customFields.errors.reorderFailed')),
      );
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (field: PmsCustomField) => {
    if (!api) return;
    const confirmed = await confirm({
      title: t('pms.customFields.deleteTitle'),
      description: t('pms.customFields.deleteConfirm', { name: field.name }),
      confirmLabel: t('common:actions.delete'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!confirmed) return;
    try {
      await deleteCustomField(api, field.id);
      if (editingId === field.id) setEditingId(null);
      await reload();
    } catch (caught) {
      feedback.error(
        apiErrorMessage(caught, t('pms.customFields.errors.deleteFailed')),
      );
    }
  };

  return (
    <section className="flex h-full min-h-0 flex-col gap-3">
      <div>
        <h3 className="app-text-overline text-app-ink/50">
          {t('pms.customFields.heading')}
        </h3>
        <p className="app-text-caption mt-1 text-app-ink/50">
          {canManage ? t('pms.customFields.hint') : t('pms.customFields.readOnly')}
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
          {fields.length === 0 ? (
            <p className="app-text-caption py-4 text-center text-app-ink/45">
              {t('pms.customFields.empty')}
            </p>
          ) : null}
          {fields.map((field, index) =>
            editingId === field.id ? (
              <div
                key={field.id}
                className="grid gap-2 rounded-md border border-app-accent/40 bg-app-surface-sidebar px-2.5 py-2"
              >
                <div className="flex items-center gap-2">
                  <input
                    aria-label={t('pms.customFields.edit', { name: field.name })}
                    className={`${INPUT_CLASS} flex-1`}
                    maxLength={100}
                    onChange={(event) => setEditName(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Escape') setEditingId(null);
                    }}
                    value={editName}
                  />
                  <span className="app-text-caption shrink-0 text-app-ink/50">
                    {t(customFieldTypeLabelKey(field.field_type))}
                  </span>
                  <button
                    type="button"
                    aria-label={t('common:actions.save')}
                    className="text-app-accent hover:opacity-80 disabled:opacity-40"
                    disabled={busy || !editName.trim()}
                    onClick={() => void handleSaveEdit(field)}
                  >
                    {busy ? (
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
                {customFieldHasOptions(field.field_type) ? (
                  <OptionsEditor drafts={editOptions} onChange={setEditOptions} />
                ) : null}
              </div>
            ) : (
              <div
                key={field.id}
                className="group flex min-h-11 items-center gap-2 rounded-md border border-app-border bg-app-surface-sidebar px-2.5 py-1.5"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-baseline gap-2">
                    <span className="app-text-body-sm min-w-0 truncate font-medium text-app-ink">
                      {field.name}
                    </span>
                    <span className="app-text-caption shrink-0 text-app-ink/45">
                      {t(customFieldTypeLabelKey(field.field_type))}
                    </span>
                  </div>
                  {field.options?.length ? (
                    <div className="app-text-micro mt-0.5 truncate text-app-ink/45">
                      {field.options.join(' · ')}
                    </div>
                  ) : null}
                </div>
                {canManage ? (
                  <>
                    <button
                      type="button"
                      aria-label={t('pms.customFields.moveUp', { name: field.name })}
                      className="text-app-ink/40 hover:text-app-ink disabled:opacity-30"
                      disabled={busy || index === 0}
                      onClick={() => void handleMove(field.id, -1)}
                    >
                      <ArrowUp size={13} />
                    </button>
                    <button
                      type="button"
                      aria-label={t('pms.customFields.moveDown', {
                        name: field.name,
                      })}
                      className="text-app-ink/40 hover:text-app-ink disabled:opacity-30"
                      disabled={busy || index === fields.length - 1}
                      onClick={() => void handleMove(field.id, 1)}
                    >
                      <ArrowDown size={13} />
                    </button>
                    <button
                      type="button"
                      aria-label={t('pms.customFields.edit', { name: field.name })}
                      className="text-app-ink/40 hover:text-app-ink"
                      onClick={() => startEdit(field)}
                    >
                      <Pencil size={13} />
                    </button>
                    <button
                      type="button"
                      aria-label={t('pms.customFields.delete', { name: field.name })}
                      className="text-app-ink/40 hover:text-app-danger-text"
                      onClick={() => void handleDelete(field)}
                    >
                      <Trash2 size={13} />
                    </button>
                  </>
                ) : null}
              </div>
            ),
          )}
        </div>
      )}
      {canManage ? (
        <div className="grid gap-2 rounded-lg border border-app-border bg-app-surface-sidebar p-3">
          <div className="flex flex-wrap items-center gap-2">
            <input
              aria-label={t('pms.customFields.namePlaceholder')}
              className={`${INPUT_CLASS} flex-1`}
              maxLength={100}
              onChange={(event) => setNewName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing)
                  void handleCreate();
              }}
              placeholder={t('pms.customFields.namePlaceholder')}
              value={newName}
            />
            <select
              aria-label={t('pms.customFields.typeLabel')}
              className="app-field-input-sm max-w-36 shrink-0"
              onChange={(event) =>
                setNewType(event.target.value as PmsCustomFieldType)
              }
              value={newType}
            >
              {CUSTOM_FIELD_TYPES.map((type) => (
                <option key={type} value={type}>
                  {t(customFieldTypeLabelKey(type))}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="app-control"
              disabled={creating || !canCreate}
              onClick={() => void handleCreate()}
            >
              {creating ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Plus size={14} />
              )}
              {t('pms.customFields.add')}
            </button>
          </div>
          {newNeedsOptions ? (
            <OptionsEditor drafts={newOptions} onChange={setNewOptions} />
          ) : null}
        </div>
      ) : null}
      {confirmDialog}
    </section>
  );
}
