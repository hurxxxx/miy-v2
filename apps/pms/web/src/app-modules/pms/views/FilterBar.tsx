import { useState, useRef, useEffect, useMemo } from 'react';
import {
  X,
  Filter,
  ChevronDown,
  Save,
  BookmarkCheck,
  Search,
  Star,
  Pencil,
  Users,
  RefreshCw,
  Check,
} from 'lucide-react';
import { DetailDrawer } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { DateInput } from '../../../components/date/DateInput';
import { UserOptionRow } from '../../../platform/users/UserSearchMultiSelect';
import { selectUserOptionsForPicker } from '../../../platform/users/user-option-picker-model';
import type {
  TaskFilterParams,
  PmsCustomField,
  PmsTaskListMember,
  PmsMilestone,
  PmsLabel,
  PmsTaskListStatus,
  PmsSavedFilter,
  PmsSavedFilterInput,
} from '../api/pms-api';
import {
  DEFAULT_ISSUE_ARCHIVED_STATE,
  getEffectiveTaskStatusFilter,
} from '../api/pms-filters';
import {
  buildTaskFilterPills,
  createClearedTaskFilterParams,
  createLoadedSavedTaskFilterParams,
  createSavedTaskFilterParams,
  isFilterActive,
  type FilterBarStatusOption,
} from './filter-bar-model';
import { withCustomFieldFilter } from './custom-field-model';
import type { PmsSavedFiltersController } from './usePmsSavedFilters';

const DEFAULT_statusOptions = [
  {
    value: 'todo',
    category: 'not_started',
    labelKey: 'pms.filter.status.todo',
  },
  {
    value: 'in_progress',
    category: 'active',
    labelKey: 'pms.filter.status.inProgress',
  },
  {
    value: 'review',
    category: 'active',
    labelKey: 'pms.filter.status.review',
  },
  {
    value: 'done',
    category: 'done',
    labelKey: 'pms.filter.status.done',
  },
  {
    value: 'complete',
    category: 'closed',
    labelKey: 'pms.filter.status.complete',
  },
] as const;

const PRIORITY_OPTIONS = [
  { value: 'low', labelKey: 'pms.priorityLow' },
  { value: 'medium', labelKey: 'pms.priorityMedium' },
  { value: 'high', labelKey: 'pms.priorityHigh' },
  { value: 'critical', labelKey: 'pms.priorityCritical' },
] as const;

const ARCHIVE_OPTIONS = [
  { value: 'active', labelKey: 'pms.filter.archive.active' },
  { value: 'archived', labelKey: 'pms.filter.archive.archived' },
  { value: 'all', labelKey: 'pms.filter.all' },
] as const;

const FILTER_TRIGGER_CLASS = 'app-control h-8 shrink-0 px-2.5';
const FILTER_MENU_ITEM_CLASS = 'app-menu-item';
const FILTER_CHECKBOX_ITEM_CLASS = 'app-menu-checkbox-item';
const FILTER_FIELD_CLASS = 'app-field-input-sm bg-app-surface-sidebar';
const FILTER_META_LABEL_CLASS = 'app-text-overline block text-app-ink/50';
const FILTER_INLINE_ACTION_CLASS =
  'app-text-caption text-app-ink/50 transition-colors';

function isFloatingLayerTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest('[data-ui-floating-layer]') !== null
  );
}

/** Reusable dropdown wrapper. */
function Dropdown({
  label,
  active,
  children,
  block = false,
  alignEnd = false,
}: {
  label: string;
  active: boolean;
  block?: boolean;
  /** Open toward the left with more room, for the last control in the filter row. */
  alignEnd?: boolean;
  children: (close: () => void) => React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handler(e: MouseEvent) {
      if (isFloatingLayerTarget(e.target)) return;
      if (ref.current && !ref.current.contains(e.target as Node))
        setOpen(false);
    }
    if (open) document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  return (
    <div ref={ref} className={block ? 'relative w-full' : 'relative'}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className={`${FILTER_TRIGGER_CLASS} ${block ? 'w-full justify-between' : ''} ${
          active
            ? 'app-control-active'
            : 'border-app-border text-app-ink/60 hover:border-app-ink/30 hover:text-app-ink'
        }`}
      >
        {label}
        <ChevronDown size={12} />
      </button>
      {open && (
        <div
          className={`${block ? 'left-0 right-0' : alignEnd ? 'right-0 min-w-[260px]' : 'left-0 min-w-[180px]'} absolute top-full z-[var(--ui-z-popover)] mt-1 max-h-60 overflow-y-auto rounded-lg border border-app-border bg-app-bg py-1 shadow-xl custom-scrollbar`}
        >
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

type FilterBarProps = {
  /** AI-DO keyed browser-stored saved filters by it; miy keeps them on the
   *  server (`savedFilters`). */
  taskListId: string;
  filterParams: TaskFilterParams;
  setFilterParams: (params: TaskFilterParams) => void;
  members: PmsTaskListMember[];
  currentUserId?: string | null;
  milestones: PmsMilestone[];
  labels: PmsLabel[];
  taskListStatuses?: PmsTaskListStatus[];
  /** miy: the list's custom fields, offered as filter conditions. */
  customFields?: PmsCustomField[];
  /** miy: the view's saved filters on the server. */
  savedFilters?: PmsSavedFiltersController;
  /** What a new or replaced saved filter holds besides the filter bar (sort,
   *  grouping); the filter bar without the search is always saved. */
  captureSavedFilter?: () => Omit<PmsSavedFilterInput, 'name' | 'shared' | 'params'>;
  /** Apply a saved filter (its sort and grouping too); without it only the
   *  filter bar changes. */
  onApplySavedFilter?: (filter: PmsSavedFilter) => void;
  /** Space owners and admins can share a filter with the space. */
  canShareSavedFilters?: boolean;
};

type MemberUserOption = PmsTaskListMember & { id: string };

function memberUserOption(member: PmsTaskListMember): MemberUserOption {
  return { ...member, id: member.user_id };
}

export function FilterBar(props: FilterBarProps) {
  const model = useFilterBarModel(props);
  return renderFilterBarContent(model);
}

function useFilterBarModel({
  taskListId,
  filterParams,
  setFilterParams,
  members,
  currentUserId,
  milestones,
  labels,
  taskListStatuses,
  customFields = [],
  savedFilters: savedFilterController,
  captureSavedFilter,
  onApplySavedFilter,
  canShareSavedFilters = false,
}: FilterBarProps) {
  const { t } = useTranslation('apps');
  const configuredStatuses = taskListStatuses ?? [];
  const hasConfiguredStatuses = configuredStatuses.length > 0;
  const statusOptions: FilterBarStatusOption[] = hasConfiguredStatuses
    ? configuredStatuses.map((s) => ({ value: s.slug, label: s.name }))
    : DEFAULT_statusOptions.map((option) => ({
        value: option.value,
        label: t(option.labelKey),
      }));
  const statusDefinitions = hasConfiguredStatuses
    ? configuredStatuses
    : DEFAULT_statusOptions.map((option) => ({
        category: option.category,
        slug: option.value,
      }));
  const selectedStatusSlugs =
    getEffectiveTaskStatusFilter(filterParams.status, statusDefinitions) ?? [];
  const selectedStatusSet = new Set(selectedStatusSlugs);
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  const [shareOnSave, setShareOnSave] = useState(false);
  const [savingFilter, setSavingFilter] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState('');
  const [mobileFiltersOpen, setMobileFiltersOpen] = useState(false);
  const [filterName, setFilterName] = useState('');
  const [assigneeQuery, setAssigneeQuery] = useState('');

  const active = isFilterActive(filterParams);
  const savedFilters = savedFilterController?.items ?? [];
  void taskListId;

  const clearAll = () => {
    setFilterParams(createClearedTaskFilterParams(filterParams));
  };

  const captureCurrent = (): Omit<PmsSavedFilterInput, 'name' | 'shared'> => ({
    ...captureSavedFilter?.(),
    params: createSavedTaskFilterParams(filterParams),
  });

  // The name stays in the box until the server has the filter, so a failed
  // or slow save keeps what was typed.
  const handleSave = async () => {
    const name = filterName.trim();
    if (!name || !savedFilterController || savingFilter) return;
    setSavingFilter(true);
    const saved = await savedFilterController.create({
      ...captureCurrent(),
      name,
      shared: canShareSavedFilters && shareOnSave,
    });
    setSavingFilter(false);
    if (!saved) return;
    setFilterName('');
    setShareOnSave(false);
    setSaveDialogOpen(false);
  };

  const handleDeleteSaved = (filter: PmsSavedFilter) => {
    void savedFilterController?.remove(filter);
  };

  const handleToggleDefaultSaved = (filter: PmsSavedFilter) => {
    void savedFilterController?.setDefault(filter.is_default ? null : filter);
  };

  const handleToggleSharedSaved = (filter: PmsSavedFilter) => {
    void savedFilterController?.update(filter, { shared: !filter.shared });
  };

  const handleReplaceSaved = (filter: PmsSavedFilter) => {
    void savedFilterController?.update(filter, captureCurrent());
  };

  const startRenameSaved = (filter: PmsSavedFilter) => {
    setRenamingId(filter.id);
    setRenameDraft(filter.name);
  };

  const handleRenameSaved = async (filter: PmsSavedFilter) => {
    const name = renameDraft.trim();
    if (!name || name === filter.name) {
      setRenamingId(null);
      return;
    }
    const saved = await savedFilterController?.update(filter, { name });
    if (saved) setRenamingId(null);
  };

  const handleLoadSaved = (saved: PmsSavedFilter) => {
    if (onApplySavedFilter) {
      onApplySavedFilter(saved);
      return;
    }
    setFilterParams(createLoadedSavedTaskFilterParams(saved.params, filterParams));
  };

  const setCustomFieldFilter = (fieldId: string, value: string) => {
    setFilterParams(withCustomFieldFilter(filterParams, fieldId, value));
  };

  const toggleStatus = (status: string) => {
    const next = selectedStatusSet.has(status)
      ? selectedStatusSlugs.filter((selected) => selected !== status)
      : [...selectedStatusSlugs, status];
    setFilterParams({ ...filterParams, status: next });
  };

  const pills = buildTaskFilterPills({
    customFields,
    labels,
    members,
    milestones,
    params: filterParams,
    statusOptions,
    translate: (key, options) => t(key, options),
  }).map((pill) => ({
    label: pill.label,
    clear: () => setFilterParams(pill.clearParams),
  }));
  const assigneeOptions = useMemo(
    () =>
      selectUserOptionsForPicker({
        users: members.map(memberUserOption),
        query: assigneeQuery,
        currentUserId,
        limit: 12,
      }),
    [assigneeQuery, currentUserId, members],
  );

  return {
    active,
    assigneeOptions,
    assigneeQuery,
    canSaveFilters: Boolean(savedFilterController),
    canShareSavedFilters,
    clearAll,
    currentUserId,
    customFields,
    filterName,
    filterParams,
    handleDeleteSaved,
    handleLoadSaved,
    handleRenameSaved,
    handleReplaceSaved,
    handleSave,
    handleToggleDefaultSaved,
    handleToggleSharedSaved,
    labels,
    renameDraft,
    renamingId,
    savedFilterError: savedFilterController?.actionError ?? null,
    savingFilter,
    setCustomFieldFilter,
    setRenameDraft,
    setRenamingId,
    setShareOnSave,
    shareOnSave,
    startRenameSaved,
    members,
    milestones,
    mobileFiltersOpen,
    pills,
    saveDialogOpen,
    savedFilters,
    setAssigneeQuery,
    setFilterName,
    setFilterParams,
    setMobileFiltersOpen,
    setSaveDialogOpen,
    selectedStatusSet,
    statusOptions,
    t,
    toggleStatus,
  };
}

/** Saved filters in the '저장됨' menu: apply, open the view with it, and for
 *  the filters the person manages rename, replace with the current conditions,
 *  share with the space (owners and admins) or remove. miy keeps them on the
 *  server; AI-DO kept them in browser storage. */
function SavedFilterMenuItems({
  canShare,
  close,
  onDelete,
  onLoad,
  onRename,
  onReplace,
  onToggleDefault,
  onToggleShared,
  renameDraft,
  renamingId,
  savedFilters,
  setRenameDraft,
  setRenamingId,
  startRename,
  t,
}: {
  canShare: boolean;
  close: () => void;
  onDelete: (filter: PmsSavedFilter) => void;
  onLoad: (filter: PmsSavedFilter) => void;
  onRename: (filter: PmsSavedFilter) => void;
  onReplace: (filter: PmsSavedFilter) => void;
  onToggleDefault: (filter: PmsSavedFilter) => void;
  onToggleShared: (filter: PmsSavedFilter) => void;
  renameDraft: string;
  renamingId: string | null;
  savedFilters: PmsSavedFilter[];
  setRenameDraft: (value: string) => void;
  setRenamingId: (value: string | null) => void;
  startRename: (filter: PmsSavedFilter) => void;
  t: ReturnType<typeof useFilterBarModel>['t'];
}) {
  const iconButton =
    'p-0.5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 text-app-ink/30';
  return (
    <>
      {savedFilters.map((sf) =>
        renamingId === sf.id ? (
          <div key={sf.id} className="flex items-center gap-1 px-3 py-1.5">
            <input
              aria-label={t('pms.savedFilters.rename', { label: sf.name })}
              autoFocus
              className={`${FILTER_FIELD_CLASS} min-w-0 flex-1`}
              maxLength={80}
              onChange={(event) => setRenameDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing)
                  onRename(sf);
                if (event.key === 'Escape') setRenamingId(null);
              }}
              value={renameDraft}
            />
            <button
              type="button"
              aria-label={t('common:actions.save')}
              className="p-0.5 text-app-accent"
              onClick={() => onRename(sf)}
            >
              <Check size={12} />
            </button>
            <button
              type="button"
              aria-label={t('common:actions.cancel')}
              className="p-0.5 text-app-ink/40 hover:text-app-ink"
              onClick={() => setRenamingId(null)}
            >
              <X size={12} />
            </button>
          </div>
        ) : (
          <div
            key={sf.id}
            className="flex items-center justify-between gap-1 px-3 py-1.5 hover:bg-app-surface-hover group"
          >
            <button
              type="button"
              onClick={() => {
                onLoad(sf);
                close();
              }}
              className="app-text-control-sm flex min-w-0 flex-1 items-center gap-2 text-left text-app-ink"
              title={
                sf.shared && !sf.is_mine
                  ? t('pms.savedFilters.sharedBy', { name: sf.owner_name })
                  : undefined
              }
            >
              <BookmarkCheck size={12} className="text-app-accent shrink-0" />
              <span className="truncate">{sf.name}</span>
              {sf.is_default ? (
                <span className="shrink-0 rounded bg-app-accent/10 px-1 app-text-caption text-app-accent">
                  {t('pms.filter.defaultSavedFilter')}
                </span>
              ) : null}
              {sf.shared ? (
                <span className="shrink-0 rounded bg-app-surface-hover px-1 app-text-caption text-app-ink/60">
                  {t('pms.savedFilters.shared')}
                </span>
              ) : null}
            </button>
            <button
              type="button"
              aria-pressed={sf.is_default}
              onClick={(e) => {
                e.stopPropagation();
                onToggleDefault(sf);
              }}
              className={`p-0.5 focus-visible:opacity-100 ${
                sf.is_default
                  ? 'text-app-accent'
                  : 'opacity-0 group-hover:opacity-100 text-app-ink/30 hover:text-app-accent'
              }`}
              aria-label={t(
                sf.is_default
                  ? 'pms.filter.unsetDefaultSavedFilter'
                  : 'pms.filter.setDefaultSavedFilter',
                { label: sf.name },
              )}
            >
              <Star
                size={11}
                className={sf.is_default ? 'fill-current' : undefined}
              />
            </button>
            {sf.can_manage ? (
              <>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    startRename(sf);
                  }}
                  className={`${iconButton} hover:text-app-ink`}
                  aria-label={t('pms.savedFilters.rename', { label: sf.name })}
                >
                  <Pencil size={11} />
                </button>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onReplace(sf);
                  }}
                  className={`${iconButton} hover:text-app-ink`}
                  aria-label={t('pms.savedFilters.update', { label: sf.name })}
                >
                  <RefreshCw size={11} />
                </button>
                {canShare ? (
                  <button
                    type="button"
                    aria-pressed={sf.shared}
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggleShared(sf);
                    }}
                    className={`p-0.5 focus-visible:opacity-100 ${
                      sf.shared
                        ? 'text-app-accent'
                        : 'opacity-0 group-hover:opacity-100 text-app-ink/30 hover:text-app-accent'
                    }`}
                    aria-label={t(
                      sf.shared
                        ? 'pms.savedFilters.unshareAction'
                        : 'pms.savedFilters.shareAction',
                      { label: sf.name },
                    )}
                  >
                    <Users size={11} />
                  </button>
                ) : null}
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(sf);
                  }}
                  className={`${iconButton} hover:text-app-danger-text`}
                  aria-label={t('pms.filter.removeFilter', {
                    label: sf.name,
                  })}
                >
                  <X size={11} />
                </button>
              </>
            ) : null}
          </div>
        ),
      )}
      <p className="border-t border-app-border px-3 py-1.5 app-text-caption text-app-ink/50">
        {t('pms.filter.defaultSavedFilterHint')}
      </p>
    </>
  );
}

/** miy: one dropdown for the list's custom fields; each field takes one
 *  condition (text contains, an option, a person, a number or a date). */
function CustomFieldFilterMenu({
  customFields,
  filterParams,
  members,
  onChange,
  t,
}: {
  customFields: PmsCustomField[];
  filterParams: TaskFilterParams;
  members: PmsTaskListMember[];
  onChange: (fieldId: string, value: string) => void;
  t: ReturnType<typeof useFilterBarModel>['t'];
}) {
  return (
    <div className="grid gap-2 px-3 py-2">
      {customFields.map((field) => {
        const value = filterParams.custom_fields?.[field.id] ?? '';
        const choices =
          field.field_type === 'select' || field.field_type === 'multi_select'
            ? (field.options ?? []).map((option) => ({ value: option, label: option }))
            : field.field_type === 'user'
              ? members.map((member) => ({
                  value: member.user_id,
                  label: member.full_name,
                }))
              : null;
        return (
          <label key={field.id} className="grid gap-1">
            <span className={FILTER_META_LABEL_CLASS}>{field.name}</span>
            {choices ? (
              <select
                className={FILTER_FIELD_CLASS}
                onChange={(event) => onChange(field.id, event.target.value)}
                value={value}
              >
                <option value="">{t('pms.filter.all')}</option>
                {choices.map((choice) => (
                  <option key={choice.value} value={choice.value}>
                    {choice.label}
                  </option>
                ))}
              </select>
            ) : field.field_type === 'date' ? (
              <DateInput
                aria-label={field.name}
                className={FILTER_FIELD_CLASS}
                onValueChange={(next) => onChange(field.id, next)}
                value={value}
              />
            ) : (
              <CustomFieldTextFilter
                fieldName={field.name}
                numeric={field.field_type === 'number'}
                onCommit={(next) => onChange(field.id, next)}
                placeholder={t('pms.customFields.filterPlaceholder')}
                value={value}
              />
            )}
          </label>
        );
      })}
    </div>
  );
}

/** Applies on Enter or when leaving the box, not on every key. */
function CustomFieldTextFilter({
  fieldName,
  numeric,
  onCommit,
  placeholder,
  value,
}: {
  fieldName: string;
  numeric: boolean;
  onCommit: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => {
    setDraft(value);
  }, [value]);
  const commit = () => {
    if (numeric && draft.trim() && !Number.isFinite(Number(draft.trim()))) return;
    if (draft !== value) onCommit(draft);
  };
  return (
    <input
      aria-label={fieldName}
      className={FILTER_FIELD_CLASS}
      inputMode={numeric ? 'decimal' : undefined}
      onBlur={commit}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === 'Enter' && !event.nativeEvent.isComposing) commit();
      }}
      placeholder={placeholder}
      type="text"
      value={draft}
    />
  );
}

function renderFilterBarContent({
  active,
  assigneeOptions,
  assigneeQuery,
  canSaveFilters,
  canShareSavedFilters,
  clearAll,
  currentUserId,
  customFields,
  filterName,
  filterParams,
  handleDeleteSaved,
  handleLoadSaved,
  handleRenameSaved,
  handleReplaceSaved,
  handleSave,
  handleToggleDefaultSaved,
  handleToggleSharedSaved,
  labels,
  renameDraft,
  renamingId,
  savedFilterError,
  savingFilter,
  setCustomFieldFilter,
  setRenameDraft,
  setRenamingId,
  setShareOnSave,
  shareOnSave,
  startRenameSaved,
  members,
  milestones,
  mobileFiltersOpen,
  pills,
  saveDialogOpen,
  savedFilters,
  setAssigneeQuery,
  setFilterName,
  setFilterParams,
  setMobileFiltersOpen,
  setSaveDialogOpen,
  selectedStatusSet,
  statusOptions,
  t,
  toggleStatus,
}: ReturnType<typeof useFilterBarModel>) {
  return (
    <>
      <div className="border-b border-app-border bg-app-bg/70 px-4 py-3 lg:hidden">
        <div className="flex items-center gap-2">
          <div className="app-text-body-sm flex h-9 min-w-0 flex-1 items-center gap-2 rounded-lg border border-app-border bg-app-surface-sidebar px-3 text-app-ink focus-within:border-app-accent">
            <Search size={15} className="shrink-0 text-app-ink/40" />
            <input
              type="text"
              aria-label={t('pms.filter.searchPlaceholder')}
              placeholder={t('pms.filter.searchPlaceholder')}
              value={filterParams.q ?? ''}
              onChange={(e) =>
                setFilterParams({
                  ...filterParams,
                  q: e.target.value || undefined,
                })
              }
              className="min-w-0 flex-1 bg-transparent text-inherit focus:outline-none placeholder:text-app-ink/40"
            />
            {filterParams.q && (
              <button
                type="button"
                onClick={() =>
                  setFilterParams({ ...filterParams, q: undefined })
                }
                className="shrink-0 text-app-ink/40 hover:text-app-ink"
                aria-label={t('pms.filter.clearSearch')}
              >
                <X size={13} />
              </button>
            )}
          </div>
          <button
            type="button"
            onClick={() => setMobileFiltersOpen(true)}
            className={`app-control h-9 shrink-0 rounded-lg px-3 ${
              active
                ? 'app-control-active'
                : 'border-app-border text-app-ink/70 hover:bg-app-surface-hover hover:text-app-ink'
            }`}
          >
            <Filter size={15} />
            <span>{t('pms.filter.filter')}</span>
            {pills.length > 0 ? (
              <span className="app-text-micro rounded-full bg-app-accent px-1.5 py-0.5 text-app-accent-fg">
                {pills.length}
              </span>
            ) : null}
          </button>
        </div>

        {pills.length > 0 ? (
          <div className="mt-2 flex gap-1.5 overflow-x-auto pb-0.5">
            {pills.map((pill) => (
              <span
                key={pill.label}
                className="app-text-caption inline-flex shrink-0 items-center gap-1 rounded-full bg-app-accent/10 px-2 py-1 text-app-accent"
              >
                {pill.label}
                <button
                  type="button"
                  onClick={pill.clear}
                  className="hover:text-app-ink transition-colors"
                  aria-label={t('pms.filter.removeFilter', {
                    label: pill.label,
                  })}
                >
                  <X size={10} />
                </button>
              </span>
            ))}
          </div>
        ) : null}
      </div>

      <div className="hidden items-center gap-2 flex-wrap px-6 py-2 border-b border-app-border bg-app-bg/50 lg:flex">
        {/* Search */}
        <div className="app-text-body-sm flex h-8 items-center gap-1.5 rounded-md border border-app-border bg-app-surface-sidebar px-2.5 text-app-ink focus-within:border-app-accent">
          <Search size={13} className="text-app-ink/40 shrink-0" />
          <input
            type="text"
            aria-label={t('pms.filter.searchPlaceholder')}
            placeholder={t('pms.filter.searchPlaceholder')}
            value={filterParams.q ?? ''}
            onChange={(e) =>
              setFilterParams({
                ...filterParams,
                q: e.target.value || undefined,
              })
            }
            className="w-36 bg-transparent text-inherit focus:outline-none placeholder:text-app-ink/40"
          />
          {filterParams.q && (
            <button
              type="button"
              onClick={() => setFilterParams({ ...filterParams, q: undefined })}
              className="text-app-ink/40 hover:text-app-ink shrink-0"
              aria-label={t('pms.filter.clearSearch')}
            >
              <X size={11} />
            </button>
          )}
        </div>
        <div className="h-4 w-px bg-app-border" />

        <Filter size={13} className="text-app-ink/40 shrink-0" />

        {/* Status (multi-select) */}
        <Dropdown
          label={t('pms.filter.statusLabel')}
          active={selectedStatusSet.size > 0}
        >
          {() => (
            <>
              {statusOptions.map((opt) => {
                const checked = selectedStatusSet.has(opt.value);
                return (
                  <label key={opt.value} className={FILTER_CHECKBOX_ITEM_CLASS}>
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleStatus(opt.value)}
                      className="accent-app-accent"
                    />
                    {opt.label}
                  </label>
                );
              })}
            </>
          )}
        </Dropdown>

        {/* Priority */}
        <Dropdown
          label={t('pms.filter.priorityLabel')}
          active={!!filterParams.priority}
        >
          {(close) => (
            <>
              <button
                type="button"
                onClick={() => {
                  setFilterParams({ ...filterParams, priority: undefined });
                  close();
                }}
                className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.priority ? 'text-app-accent' : 'text-app-ink'}`}
              >
                {t('pms.filter.all')}
              </button>
              {PRIORITY_OPTIONS.map((opt) => (
                <button
                  type="button"
                  key={opt.value}
                  onClick={() => {
                    setFilterParams({ ...filterParams, priority: opt.value });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} ${filterParams.priority === opt.value ? 'text-app-accent' : 'text-app-ink'}`}
                >
                  {t(opt.labelKey)}
                </button>
              ))}
            </>
          )}
        </Dropdown>

        {/* Assignee */}
        <Dropdown
          label={t('pms.filter.assigneeLabel')}
          active={!!filterParams.assignee_id}
        >
          {(close) => (
            <>
              <div className="px-2 py-1">
                <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1.5">
                  <Search size={13} className="shrink-0 text-app-ink/40" />
                  <input
                    aria-label={t('pms.searchUser')}
                    className="app-menu-search-input flex-1"
                    onChange={(event) => setAssigneeQuery(event.target.value)}
                    placeholder={t('pms.searchUser')}
                    value={assigneeQuery}
                  />
                  {assigneeQuery ? (
                    <button
                      aria-label={t('pms.filter.clearSearch')}
                      className="text-app-ink/40 hover:text-app-ink"
                      onClick={() => setAssigneeQuery('')}
                      type="button"
                    >
                      <X size={11} />
                    </button>
                  ) : null}
                </div>
              </div>
              <button
                type="button"
                onClick={() => {
                  setFilterParams({ ...filterParams, assignee_id: undefined });
                  close();
                }}
                className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.assignee_id ? 'text-app-accent' : 'text-app-ink'}`}
              >
                {t('pms.filter.all')}
              </button>
              {assigneeOptions.map((member) => (
                <UserOptionRow
                  key={member.user_id}
                  currentUserId={currentUserId}
                  currentUserLabel={t('pms.taskDetail.me')}
                  density="compact"
                  onClick={() => {
                    setFilterParams({
                      ...filterParams,
                      assignee_id: member.user_id,
                    });
                    close();
                  }}
                  selected={filterParams.assignee_id === member.user_id}
                  user={member}
                />
              ))}
              {assigneeOptions.length === 0 ? (
                <p className="app-text-caption px-3 py-2 text-app-ink/40">
                  {t('pms.noMatchingUsers')}
                </p>
              ) : null}
            </>
          )}
        </Dropdown>

        {/* Label */}
        <Dropdown
          label={t('pms.filter.labelLabel')}
          active={!!filterParams.label_id}
        >
          {(close) => (
            <>
              <button
                type="button"
                onClick={() => {
                  setFilterParams({ ...filterParams, label_id: undefined });
                  close();
                }}
                className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.label_id ? 'text-app-accent' : 'text-app-ink'}`}
              >
                {t('pms.filter.all')}
              </button>
              {labels.map((l) => (
                <button
                  type="button"
                  key={l.id}
                  onClick={() => {
                    setFilterParams({ ...filterParams, label_id: l.id });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} flex items-center gap-2 ${filterParams.label_id === l.id ? 'text-app-accent' : 'text-app-ink'}`}
                >
                  <span
                    className="size-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: l.color }}
                  />
                  {l.name}
                </button>
              ))}
            </>
          )}
        </Dropdown>

        {/* Milestone */}
        {milestones.length > 0 && (
          <Dropdown
            label={t('pms.filter.milestoneLabel')}
            active={!!filterParams.milestone_id}
          >
            {(close) => (
              <>
                <div className="px-2 py-1">
                  <div className="flex items-center gap-2 rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1.5">
                    <Search size={13} className="shrink-0 text-app-ink/40" />
                    <input
                      aria-label={t('pms.searchUser')}
                      className="app-menu-search-input flex-1"
                      onChange={(event) => setAssigneeQuery(event.target.value)}
                      placeholder={t('pms.searchUser')}
                      value={assigneeQuery}
                    />
                    {assigneeQuery ? (
                      <button
                        aria-label={t('pms.filter.clearSearch')}
                        className="text-app-ink/40 hover:text-app-ink"
                        onClick={() => setAssigneeQuery('')}
                        type="button"
                      >
                        <X size={11} />
                      </button>
                    ) : null}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setFilterParams({
                      ...filterParams,
                      milestone_id: undefined,
                    });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.milestone_id ? 'text-app-accent' : 'text-app-ink'}`}
                >
                  {t('pms.filter.all')}
                </button>
                {milestones.map((m) => (
                  <button
                    type="button"
                    key={m.id}
                    onClick={() => {
                      setFilterParams({ ...filterParams, milestone_id: m.id });
                      close();
                    }}
                    className={`${FILTER_MENU_ITEM_CLASS} ${filterParams.milestone_id === m.id ? 'text-app-accent' : 'text-app-ink'}`}
                  >
                    {m.title}
                  </button>
                ))}
              </>
            )}
          </Dropdown>
        )}

        {/* Start Date Range */}
        <Dropdown
          label={t('pms.filter.startDateLabel')}
          active={
            !!(filterParams.start_date_from || filterParams.start_date_to)
          }
        >
          {() => (
            <div className="px-3 py-2 space-y-2">
              <label className={FILTER_META_LABEL_CLASS}>
                {t('pms.filter.from')}
              </label>
              <DateInput
                value={filterParams.start_date_from ?? ''}
                onValueChange={(value) =>
                  setFilterParams({
                    ...filterParams,
                    start_date_from: value || undefined,
                  })
                }
                className={FILTER_FIELD_CLASS}
              />
              <label className={FILTER_META_LABEL_CLASS}>
                {t('pms.filter.to')}
              </label>
              <DateInput
                value={filterParams.start_date_to ?? ''}
                onValueChange={(value) =>
                  setFilterParams({
                    ...filterParams,
                    start_date_to: value || undefined,
                  })
                }
                className={FILTER_FIELD_CLASS}
              />
            </div>
          )}
        </Dropdown>

        {/* Due Date Range */}
        <Dropdown
          label={t('pms.filter.dueDateLabel')}
          active={!!(filterParams.due_date_from || filterParams.due_date_to)}
        >
          {() => (
            <div className="px-3 py-2 space-y-2">
              <label className={FILTER_META_LABEL_CLASS}>
                {t('pms.filter.from')}
              </label>
              <DateInput
                value={filterParams.due_date_from ?? ''}
                onValueChange={(value) =>
                  setFilterParams({
                    ...filterParams,
                    due_date_from: value || undefined,
                  })
                }
                className={FILTER_FIELD_CLASS}
              />
              <label className={FILTER_META_LABEL_CLASS}>
                {t('pms.filter.to')}
              </label>
              <DateInput
                value={filterParams.due_date_to ?? ''}
                onValueChange={(value) =>
                  setFilterParams({
                    ...filterParams,
                    due_date_to: value || undefined,
                  })
                }
                className={FILTER_FIELD_CLASS}
              />
            </div>
          )}
        </Dropdown>

        {/* miy: custom fields */}
        {customFields.length > 0 && (
          <Dropdown
            label={t('pms.customFields.filterLabel')}
            active={Object.keys(filterParams.custom_fields ?? {}).length > 0}
          >
            {() => (
              <CustomFieldFilterMenu
                customFields={customFields}
                filterParams={filterParams}
                members={members}
                onChange={setCustomFieldFilter}
                t={t}
              />
            )}
          </Dropdown>
        )}

        {/* Archived visibility */}
        <Dropdown
          label={t('pms.filter.archiveLabel')}
          active={
            !!(
              filterParams.archived_state &&
              filterParams.archived_state !== DEFAULT_ISSUE_ARCHIVED_STATE
            )
          }
        >
          {(close) => (
            <>
              {ARCHIVE_OPTIONS.map((option) => (
                <button
                  type="button"
                  key={option.value}
                  onClick={() => {
                    setFilterParams({
                      ...filterParams,
                      archived_state: option.value,
                    });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} ${
                    (filterParams.archived_state ??
                      DEFAULT_ISSUE_ARCHIVED_STATE) === option.value
                      ? 'text-app-accent'
                      : 'text-app-ink'
                  }`}
                >
                  {t(option.labelKey)}
                </button>
              ))}
            </>
          )}
        </Dropdown>

        {/* Separator + actions */}
        {active && (
          <>
            <div className="h-4 w-px bg-app-border mx-1" />
            <button
              type="button"
              onClick={clearAll}
              className={`${FILTER_INLINE_ACTION_CLASS} hover:text-app-ink`}
            >
              {t('pms.filter.clearAll')}
            </button>
            {canSaveFilters ? (
              <button
                type="button"
                onClick={() => setSaveDialogOpen(true)}
                className={`${FILTER_INLINE_ACTION_CLASS} flex items-center gap-1 hover:text-app-accent`}
              >
                <Save size={11} />
                {t('common:actions.save')}
              </button>
            ) : null}
          </>
        )}

        {/* Saved filters */}
        {savedFilters.length > 0 && (
          <Dropdown
            alignEnd
            label={t('pms.filter.savedCount', { count: savedFilters.length })}
            active={false}
          >
            {(close) => (
              <SavedFilterMenuItems
                canShare={canShareSavedFilters}
                close={close}
                onDelete={handleDeleteSaved}
                onLoad={handleLoadSaved}
                onRename={(filter) => void handleRenameSaved(filter)}
                onReplace={handleReplaceSaved}
                onToggleDefault={handleToggleDefaultSaved}
                onToggleShared={handleToggleSharedSaved}
                renameDraft={renameDraft}
                renamingId={renamingId}
                savedFilters={savedFilters}
                setRenameDraft={setRenameDraft}
                setRenamingId={setRenamingId}
                startRename={startRenameSaved}
                t={t}
              />
            )}
          </Dropdown>
        )}

        {savedFilterError ? (
          <span className="app-text-caption ml-1 text-app-danger-text" role="alert">
            {savedFilterError}
          </span>
        ) : null}

        {/* Save dialog */}
        {saveDialogOpen && (
          <div className="flex items-center gap-2 ml-2">
            <input
              type="text"
              aria-label={t('pms.filter.filterNamePlaceholder')}
              value={filterName}
              onChange={(e) => setFilterName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.nativeEvent.isComposing)
                  void handleSave();
                if (e.key === 'Escape') setSaveDialogOpen(false);
              }}
              placeholder={t('pms.filter.filterNamePlaceholder')}
              className={`${FILTER_FIELD_CLASS} w-32`}
            />
            {canShareSavedFilters ? (
              <label className="app-text-caption flex items-center gap-1 text-app-ink/60">
                <input
                  checked={shareOnSave}
                  className="size-3.5 accent-app-accent"
                  onChange={(e) => setShareOnSave(e.target.checked)}
                  type="checkbox"
                />
                {t('pms.savedFilters.share')}
              </label>
            ) : null}
            <button
              type="button"
              disabled={savingFilter || !filterName.trim()}
              onClick={() => void handleSave()}
              className="app-text-control-sm text-app-accent hover:text-app-accent/80 disabled:opacity-40"
            >
              {t('common:actions.save')}
            </button>
            <button
              type="button"
              onClick={() => setSaveDialogOpen(false)}
              className="app-text-control-sm text-app-ink/40 hover:text-app-ink"
            >
              {t('common:actions.cancel')}
            </button>
          </div>
        )}

        {/* Active filter pills */}
        {pills.length > 0 && (
          <div className="flex items-center gap-1.5 ml-2 flex-wrap">
            {pills.map((pill) => (
              <span
                key={pill.label}
                className="app-text-caption inline-flex items-center gap-1 rounded-full bg-app-accent/10 px-2 py-0.5 text-app-accent"
              >
                {pill.label}
                <button
                  type="button"
                  onClick={pill.clear}
                  className="hover:text-app-ink transition-colors"
                  aria-label={t('pms.filter.removeFilter', {
                    label: pill.label,
                  })}
                >
                  <X size={10} />
                </button>
              </span>
            ))}
          </div>
        )}
      </div>
      <DetailDrawer
        actions={
          <div className="flex gap-2">
            <button
              type="button"
              onClick={clearAll}
              className="app-control flex-1"
            >
              {t('pms.filter.clear')}
            </button>
            <button
              type="button"
              onClick={() => setMobileFiltersOpen(false)}
              className="app-control-primary flex-1"
            >
              {t('pms.filter.done')}
            </button>
          </div>
        }
        closeLabel={t('pms.filter.closeFilters')}
        contentClassName="border-app-border bg-app-bg"
        description={t('pms.filter.drawerDescription')}
        onOpenChange={setMobileFiltersOpen}
        open={mobileFiltersOpen}
        title={t('pms.filter.filters')}
      >
        <div className="space-y-3">
          {pills.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {pills.map((pill) => (
                <span
                  key={pill.label}
                  className="app-text-caption inline-flex items-center gap-1 rounded-full bg-app-accent/10 px-2 py-1 text-app-accent"
                >
                  {pill.label}
                  <button
                    type="button"
                    onClick={pill.clear}
                    className="hover:text-app-ink transition-colors"
                    aria-label={t('pms.filter.removeFilter', {
                      label: pill.label,
                    })}
                  >
                    <X size={10} />
                  </button>
                </span>
              ))}
            </div>
          ) : null}

          <Dropdown
            block
            label={t('pms.filter.statusLabel')}
            active={selectedStatusSet.size > 0}
          >
            {() => (
              <>
                {statusOptions.map((opt) => {
                  const checked = selectedStatusSet.has(opt.value);
                  return (
                    <label
                      key={opt.value}
                      className={FILTER_CHECKBOX_ITEM_CLASS}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggleStatus(opt.value)}
                        className="accent-app-accent"
                      />
                      {opt.label}
                    </label>
                  );
                })}
              </>
            )}
          </Dropdown>

          <Dropdown
            block
            label={t('pms.filter.priorityLabel')}
            active={!!filterParams.priority}
          >
            {(close) => (
              <>
                <button
                  type="button"
                  onClick={() => {
                    setFilterParams({ ...filterParams, priority: undefined });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.priority ? 'text-app-accent' : 'text-app-ink'}`}
                >
                  {t('pms.filter.all')}
                </button>
                {PRIORITY_OPTIONS.map((opt) => (
                  <button
                    type="button"
                    key={opt.value}
                    onClick={() => {
                      setFilterParams({ ...filterParams, priority: opt.value });
                      close();
                    }}
                    className={`${FILTER_MENU_ITEM_CLASS} ${filterParams.priority === opt.value ? 'text-app-accent' : 'text-app-ink'}`}
                  >
                    {t(opt.labelKey)}
                  </button>
                ))}
              </>
            )}
          </Dropdown>

          <Dropdown
            block
            label={t('pms.filter.assigneeLabel')}
            active={!!filterParams.assignee_id}
          >
            {(close) => (
              <>
                <button
                  type="button"
                  onClick={() => {
                    setFilterParams({
                      ...filterParams,
                      assignee_id: undefined,
                    });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.assignee_id ? 'text-app-accent' : 'text-app-ink'}`}
                >
                  {t('pms.filter.all')}
                </button>
                {assigneeOptions.map((member) => (
                  <UserOptionRow
                    key={member.user_id}
                    currentUserId={currentUserId}
                    currentUserLabel={t('pms.taskDetail.me')}
                    density="compact"
                    onClick={() => {
                      setFilterParams({
                        ...filterParams,
                        assignee_id: member.user_id,
                      });
                      close();
                    }}
                    selected={filterParams.assignee_id === member.user_id}
                    user={member}
                  />
                ))}
                {assigneeOptions.length === 0 ? (
                  <p className="app-text-caption px-3 py-2 text-app-ink/40">
                    {t('pms.noMatchingUsers')}
                  </p>
                ) : null}
              </>
            )}
          </Dropdown>

          <Dropdown
            block
            label={t('pms.filter.labelLabel')}
            active={!!filterParams.label_id}
          >
            {(close) => (
              <>
                <button
                  type="button"
                  onClick={() => {
                    setFilterParams({ ...filterParams, label_id: undefined });
                    close();
                  }}
                  className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.label_id ? 'text-app-accent' : 'text-app-ink'}`}
                >
                  {t('pms.filter.all')}
                </button>
                {labels.map((l) => (
                  <button
                    type="button"
                    key={l.id}
                    onClick={() => {
                      setFilterParams({ ...filterParams, label_id: l.id });
                      close();
                    }}
                    className={`${FILTER_MENU_ITEM_CLASS} flex items-center gap-2 ${filterParams.label_id === l.id ? 'text-app-accent' : 'text-app-ink'}`}
                  >
                    <span
                      className="size-2.5 shrink-0 rounded-full"
                      style={{ backgroundColor: l.color }}
                    />
                    {l.name}
                  </button>
                ))}
              </>
            )}
          </Dropdown>

          {customFields.length > 0 && (
            <Dropdown
              block
              label={t('pms.customFields.filterLabel')}
              active={Object.keys(filterParams.custom_fields ?? {}).length > 0}
            >
              {() => (
                <CustomFieldFilterMenu
                  customFields={customFields}
                  filterParams={filterParams}
                  members={members}
                  onChange={setCustomFieldFilter}
                  t={t}
                />
              )}
            </Dropdown>
          )}

          {milestones.length > 0 && (
            <Dropdown
              block
              label={t('pms.filter.milestoneLabel')}
              active={!!filterParams.milestone_id}
            >
              {(close) => (
                <>
                  <button
                    type="button"
                    onClick={() => {
                      setFilterParams({
                        ...filterParams,
                        milestone_id: undefined,
                      });
                      close();
                    }}
                    className={`${FILTER_MENU_ITEM_CLASS} ${!filterParams.milestone_id ? 'text-app-accent' : 'text-app-ink'}`}
                  >
                    {t('pms.filter.all')}
                  </button>
                  {milestones.map((m) => (
                    <button
                      type="button"
                      key={m.id}
                      onClick={() => {
                        setFilterParams({
                          ...filterParams,
                          milestone_id: m.id,
                        });
                        close();
                      }}
                      className={`${FILTER_MENU_ITEM_CLASS} ${filterParams.milestone_id === m.id ? 'text-app-accent' : 'text-app-ink'}`}
                    >
                      {m.title}
                    </button>
                  ))}
                </>
              )}
            </Dropdown>
          )}

          <Dropdown
            block
            label={t('pms.filter.startDateLabel')}
            active={
              !!(filterParams.start_date_from || filterParams.start_date_to)
            }
          >
            {() => (
              <div className="px-3 py-2 space-y-2">
                <label className={FILTER_META_LABEL_CLASS}>
                  {t('pms.filter.from')}
                </label>
                <DateInput
                  value={filterParams.start_date_from ?? ''}
                  onValueChange={(value) =>
                    setFilterParams({
                      ...filterParams,
                      start_date_from: value || undefined,
                    })
                  }
                  className={FILTER_FIELD_CLASS}
                />
                <label className={FILTER_META_LABEL_CLASS}>
                  {t('pms.filter.to')}
                </label>
                <DateInput
                  value={filterParams.start_date_to ?? ''}
                  onValueChange={(value) =>
                    setFilterParams({
                      ...filterParams,
                      start_date_to: value || undefined,
                    })
                  }
                  className={FILTER_FIELD_CLASS}
                />
              </div>
            )}
          </Dropdown>

          <Dropdown
            block
            label={t('pms.filter.dueDateLabel')}
            active={!!(filterParams.due_date_from || filterParams.due_date_to)}
          >
            {() => (
              <div className="px-3 py-2 space-y-2">
                <label className={FILTER_META_LABEL_CLASS}>
                  {t('pms.filter.from')}
                </label>
                <DateInput
                  value={filterParams.due_date_from ?? ''}
                  onValueChange={(value) =>
                    setFilterParams({
                      ...filterParams,
                      due_date_from: value || undefined,
                    })
                  }
                  className={FILTER_FIELD_CLASS}
                />
                <label className={FILTER_META_LABEL_CLASS}>
                  {t('pms.filter.to')}
                </label>
                <DateInput
                  value={filterParams.due_date_to ?? ''}
                  onValueChange={(value) =>
                    setFilterParams({
                      ...filterParams,
                      due_date_to: value || undefined,
                    })
                  }
                  className={FILTER_FIELD_CLASS}
                />
              </div>
            )}
          </Dropdown>

          <Dropdown
            block
            label={t('pms.filter.archiveLabel')}
            active={
              !!(
                filterParams.archived_state &&
                filterParams.archived_state !== DEFAULT_ISSUE_ARCHIVED_STATE
              )
            }
          >
            {(close) => (
              <>
                {ARCHIVE_OPTIONS.map((option) => (
                  <button
                    type="button"
                    key={option.value}
                    onClick={() => {
                      setFilterParams({
                        ...filterParams,
                        archived_state: option.value,
                      });
                      close();
                    }}
                    className={`${FILTER_MENU_ITEM_CLASS} ${
                      (filterParams.archived_state ??
                        DEFAULT_ISSUE_ARCHIVED_STATE) === option.value
                        ? 'text-app-accent'
                        : 'text-app-ink'
                    }`}
                  >
                    {t(option.labelKey)}
                  </button>
                ))}
              </>
            )}
          </Dropdown>

          {active && canSaveFilters ? (
            <button
              type="button"
              onClick={() => setSaveDialogOpen(true)}
              className={`${FILTER_INLINE_ACTION_CLASS} flex items-center gap-1 hover:text-app-accent`}
            >
              <Save size={11} />
              {t('pms.filter.saveCurrentFilters')}
            </button>
          ) : null}

          {savedFilters.length > 0 ? (
            <Dropdown
              block
              label={t('pms.filter.savedCount', { count: savedFilters.length })}
              active={false}
            >
              {(close) => (
                <SavedFilterMenuItems
                  canShare={canShareSavedFilters}
                  close={close}
                  onDelete={handleDeleteSaved}
                  onLoad={handleLoadSaved}
                  onRename={(filter) => void handleRenameSaved(filter)}
                  onReplace={handleReplaceSaved}
                  onToggleDefault={handleToggleDefaultSaved}
                  onToggleShared={handleToggleSharedSaved}
                  renameDraft={renameDraft}
                  renamingId={renamingId}
                  savedFilters={savedFilters}
                  setRenameDraft={setRenameDraft}
                  setRenamingId={setRenamingId}
                  startRename={startRenameSaved}
                  t={t}
                />
              )}
            </Dropdown>
          ) : null}

          {saveDialogOpen ? (
            <div className="rounded-lg border border-app-border bg-app-surface p-3">
              <input
                type="text"
                aria-label={t('pms.filter.filterNamePlaceholder')}
                value={filterName}
                onChange={(e) => setFilterName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.nativeEvent.isComposing)
                    void handleSave();
                  if (e.key === 'Escape') setSaveDialogOpen(false);
                }}
                placeholder={t('pms.filter.filterNamePlaceholder')}
                className={FILTER_FIELD_CLASS}
              />
              {canShareSavedFilters ? (
                <label className="app-text-caption mt-2 flex items-center gap-1.5 text-app-ink/60">
                  <input
                    checked={shareOnSave}
                    className="size-3.5 accent-app-accent"
                    onChange={(e) => setShareOnSave(e.target.checked)}
                    type="checkbox"
                  />
                  {t('pms.savedFilters.share')}
                </label>
              ) : null}
              <div className="mt-2 flex gap-2">
                <button
                  type="button"
                  disabled={savingFilter || !filterName.trim()}
                  onClick={() => void handleSave()}
                  className="app-control-primary flex-1"
                >
                  {t('common:actions.save')}
                </button>
                <button
                  type="button"
                  onClick={() => setSaveDialogOpen(false)}
                  className="app-control flex-1 text-app-ink/60 hover:text-app-ink"
                >
                  {t('common:actions.cancel')}
                </button>
              </div>
            </div>
          ) : null}
        </div>
      </DetailDrawer>
    </>
  );
}
