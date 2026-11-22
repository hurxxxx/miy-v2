import { useState, useRef, useEffect } from 'react';
import { X, ChevronDown, Trash2, Archive, Copy } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useFeedback } from '@miy/ui';
import { UserOptionRow } from '../../../platform/users/UserSearchMultiSelect';
import { selectUserOptionsForPicker } from '../../../platform/users/user-option-picker-model';
import {
  bulkUpdateTasks,
  type PmsTaskListMember,
  type PmsLabel,
  type PmsTaskListStatus,
  type TaskCopyResult,
} from '../api/pms-api';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import { StatusIconGlyph } from './StatusIcon';
import { BulkDatesForm } from './BulkDatesForm';
import { TaskCopyDialog } from './TaskCopyDialog';
import {
  buildBulkActionPayload,
  buildBulkLabelActions,
  buildBulkStatusOptions,
  buildBulkUpdateRequest,
  BULK_UPDATE_MAX_TASKS,
  exceedsBulkUpdateLimit,
  selectedTaskIdsToArray,
  type BulkUpdateActionPayload,
} from './bulk-action-bar-model';
import {
  taskCopyUnmatchedParts,
  type TaskCopyTarget,
  type TaskCopyUnmatchedKind,
} from './task-copy-model';

const COPY_UNMATCHED_LABEL_KEYS = {
  assignees: 'pms.bulk.copy.unmatchedAssignees',
  labels: 'pms.bulk.copy.unmatchedLabels',
  milestones: 'pms.bulk.copy.unmatchedMilestones',
  custom_fields: 'pms.bulk.copy.unmatchedCustomFields',
} as const satisfies Record<TaskCopyUnmatchedKind, string>;

const PRIORITY_OPTIONS = [
  { value: 'low', labelKey: 'pms.priorityLow' },
  { value: 'medium', labelKey: 'pms.priorityMedium' },
  { value: 'high', labelKey: 'pms.priorityHigh' },
  { value: 'critical', labelKey: 'pms.priorityCritical' },
] as const;

type MemberUserOption = PmsTaskListMember & { id: string };

function memberUserOption(member: PmsTaskListMember): MemberUserOption {
  return { ...member, id: member.user_id };
}

function isFloatingLayerTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest('[data-ui-floating-layer]') !== null
  );
}

function ActionDropdown({
  label,
  children,
  panelClassName = 'min-w-[160px] max-h-48 overflow-y-auto custom-scrollbar py-1',
}: {
  label: string;
  children: (close: () => void) => React.ReactNode;
  panelClassName?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handler(e: MouseEvent) {
      // A date picker calendar opens in a portal; picking a day there is not
      // a click outside the menu.
      if (isFloatingLayerTarget(e.target)) return;
      if (ref.current && !ref.current.contains(e.target as Node))
        setOpen(false);
    }
    if (open) document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="app-text-control-sm flex items-center gap-1 rounded px-3 py-1.5 text-white/80 transition-colors hover:bg-white/10 hover:text-white"
      >
        {label}
        <ChevronDown size={12} />
      </button>
      {open && (
        <div
          className={`absolute bottom-full left-0 mb-1 z-40 bg-app-bg border border-app-border rounded-lg shadow-xl ${panelClassName}`}
        >
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

export const BulkActionBar = ({
  workspaceSlug,
  taskListId,
  selectedIds,
  totalCount,
  onSelectAll,
  onDeselectAll,
  onDone,
  members,
  labels,
  taskListStatuses,
}: {
  workspaceSlug?: string | null;
  taskListId: string;
  selectedIds: Set<string>;
  totalCount: number;
  onSelectAll: () => void;
  onDeselectAll: () => void;
  onDone: () => void;
  members: PmsTaskListMember[];
  labels: PmsLabel[];
  taskListStatuses?: PmsTaskListStatus[];
}) => {
  const { t } = useTranslation('apps');
  const statusOptions = buildBulkStatusOptions({
    taskListStatuses,
    translate: t,
  });
  const labelActions = buildBulkLabelActions(labels);
  const api = useAppApi();
  const user = usePmsCurrentUser();
  const feedback = useFeedback();
  const [loading, setLoading] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [copyOpen, setCopyOpen] = useState(false);
  const [assigneeQuery, setAssigneeQuery] = useState('');

  const ids = selectedTaskIdsToArray(selectedIds);
  const assigneeOptions = selectUserOptionsForPicker({
    users: members.map(memberUserOption),
    query: assigneeQuery,
    currentUserId: user?.id,
    limit: 12,
  });

  /** Warns instead of sending a bulk update the server would reject. */
  const warnIfOverBulkLimit = () => {
    if (!exceedsBulkUpdateLimit(ids.length)) return false;
    feedback.warning(
      t('pms.bulk.tooMany', {
        max: BULK_UPDATE_MAX_TASKS,
        selected: ids.length,
      }),
    );
    return true;
  };

  const exec = async (payload: BulkUpdateActionPayload) => {
    if (!api || ids.length === 0) return;
    if (warnIfOverBulkLimit()) return;
    setLoading(true);
    try {
      await bulkUpdateTasks(
        api,
        taskListId,
        buildBulkUpdateRequest(ids, payload),
        workspaceSlug,
      );
      onDone();
    } catch (error) {
      feedback.error(
        t('pms.bulk.failed'),
        error instanceof Error && error.message ? error.message : undefined,
      );
    } finally {
      setLoading(false);
    }
  };

  const handleCopied = (result: TaskCopyResult, target: TaskCopyTarget) => {
    const unmatched = taskCopyUnmatchedParts(result.unmatched).map((part) =>
      t(COPY_UNMATCHED_LABEL_KEYS[part.kind], { count: part.count }),
    );
    feedback.success(
      t('pms.bulk.copy.copied', {
        count: result.copied_count,
        list: target.name,
      }),
      unmatched.length > 0
        ? t('pms.bulk.copy.unmatched', { items: unmatched })
        : undefined,
    );
    onDone();
  };

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex w-max max-w-[calc(100vw-2rem)] items-center gap-1 px-4 py-2 bg-app-bg-strong border border-app-border-strong rounded-xl shadow-2xl shadow-black/40">
      <span className="app-text-control-sm mr-2 whitespace-nowrap font-medium text-white">
        {t('pms.bulk.selectedCount', { count: selectedIds.size })}
      </span>

      <div className="h-4 w-px bg-app-border-strong mx-1" />

      <button
        type="button"
        onClick={onSelectAll}
        className="app-text-caption whitespace-nowrap px-2 py-1 text-white/60 transition-colors hover:text-white"
      >
        {t('pms.bulk.selectAll', { count: totalCount })}
      </button>
      <button
        type="button"
        onClick={onDeselectAll}
        className="app-text-caption whitespace-nowrap px-2 py-1 text-white/60 transition-colors hover:text-white"
      >
        {t('pms.bulk.selectNone')}
      </button>

      <div className="h-4 w-px bg-app-border-strong mx-1" />

      {/* Status */}
      <ActionDropdown label={t('pms.filter.statusLabel')}>
        {(close) => (
          <>
            {statusOptions.map((opt) => (
              <button
                key={opt.value}
                type="button"
                onClick={() => {
                  exec(
                    buildBulkActionPayload({
                      type: 'status',
                      status: opt.value,
                    }),
                  );
                  close();
                }}
                className="app-text-control-sm flex w-full items-center gap-2 px-3 py-1.5 text-left text-app-ink hover:bg-app-surface-hover"
              >
                <StatusIconGlyph
                  label={opt.label}
                  status={opt.value}
                  taskListStatuses={taskListStatuses}
                />
                {opt.label}
              </button>
            ))}
          </>
        )}
      </ActionDropdown>

      {/* Priority */}
      <ActionDropdown label={t('pms.filter.priorityLabel')}>
        {(close) => (
          <>
            {PRIORITY_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                type="button"
                onClick={() => {
                  exec(
                    buildBulkActionPayload({
                      type: 'priority',
                      priority: opt.value,
                    }),
                  );
                  close();
                }}
                className="app-text-control-sm w-full px-3 py-1.5 text-left text-app-ink hover:bg-app-surface-hover"
              >
                {t(opt.labelKey)}
              </button>
            ))}
          </>
        )}
      </ActionDropdown>

      {/* Assignee */}
      <ActionDropdown label={t('pms.filter.assigneeLabel')}>
        {(close) => (
          <>
            <div className="px-2 py-1">
              <input
                aria-label={t('pms.searchUser')}
                className="app-field-input-sm bg-app-surface-sidebar py-1.5"
                onChange={(event) => setAssigneeQuery(event.target.value)}
                placeholder={t('pms.searchUser')}
                value={assigneeQuery}
              />
            </div>
            <button
              type="button"
              onClick={() => {
                exec(
                  buildBulkActionPayload({
                    type: 'assignee',
                    assigneeId: null,
                  }),
                );
                close();
              }}
              className="app-text-control-sm w-full px-3 py-1.5 text-left text-app-ink/50 hover:bg-app-surface-hover"
            >
              {t('pms.bulk.unassign')}
            </button>
            {assigneeOptions.map((member) => (
              <UserOptionRow
                key={member.user_id}
                currentUserId={user?.id}
                currentUserLabel={t('pms.taskDetail.me')}
                density="compact"
                onClick={() => {
                  exec(
                    buildBulkActionPayload({
                      type: 'assignee',
                      assigneeId: member.user_id,
                    }),
                  );
                  close();
                }}
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
      </ActionDropdown>

      {/* Dates */}
      <ActionDropdown label={t('pms.bulk.dates.action')} panelClassName="">
        {(close) => (
          <BulkDatesForm
            disabled={loading}
            onApply={(action) => {
              exec(buildBulkActionPayload(action));
              close();
            }}
          />
        )}
      </ActionDropdown>

      {/* Labels */}
      {labels.length > 0 && (
        <ActionDropdown label={t('pms.bulk.labelsLabel')}>
          {(close) => (
            <>
              <div className="app-text-overline px-3 py-1 text-app-ink/40">
                {t('common:actions.add')}
              </div>
              {labelActions.add.map((action) => (
                <button
                  key={action.key}
                  type="button"
                  onClick={() => {
                    exec(action.payload);
                    close();
                  }}
                  className="app-text-control-sm flex w-full items-center gap-2 px-3 py-1.5 text-left text-app-ink hover:bg-app-surface-hover"
                >
                  <span
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: action.label.color }}
                  />
                  {action.label.name}
                </button>
              ))}
              <hr className="border-app-border my-1" />
              <div className="app-text-overline px-3 py-1 text-app-ink/40">
                {t('pms.bulk.remove')}
              </div>
              {labelActions.remove.map((action) => (
                <button
                  key={action.key}
                  type="button"
                  onClick={() => {
                    exec(action.payload);
                    close();
                  }}
                  className="app-text-control-sm flex w-full items-center gap-2 px-3 py-1.5 text-left text-app-danger-text/70 hover:bg-app-surface-hover"
                >
                  <span
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: action.label.color }}
                  />
                  {action.label.name}
                </button>
              ))}
            </>
          )}
        </ActionDropdown>
      )}

      <div className="h-4 w-px bg-app-border-strong mx-1" />

      {/* Copy */}
      <button
        type="button"
        onClick={() => setCopyOpen(true)}
        disabled={loading}
        className="app-text-control-sm flex items-center gap-1 rounded px-3 py-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-white"
      >
        <Copy size={13} />
        {t('pms.bulk.copy.action')}
      </button>

      {/* Archive */}
      <button
        type="button"
        onClick={() => exec(buildBulkActionPayload({ type: 'archive' }))}
        disabled={loading}
        className="app-text-control-sm flex items-center gap-1 rounded px-3 py-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-yellow-400"
      >
        <Archive size={13} />
        {t('common:actions.archive')}
      </button>
      <button
        type="button"
        onClick={() => exec(buildBulkActionPayload({ type: 'restore' }))}
        disabled={loading}
        className="app-text-control-sm flex items-center gap-1 rounded px-3 py-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-app-success-text"
      >
        <Archive size={13} />
        {t('pms.bulk.restore')}
      </button>

      {/* Delete */}
      {confirmDelete ? (
        <div className="flex items-center gap-1">
          <span className="app-text-caption whitespace-nowrap px-1 text-white/60">
            {t('pms.bulk.deleteIrreversible')}
          </span>
          <button
            type="button"
            onClick={() => {
              exec(buildBulkActionPayload({ type: 'delete' }));
              setConfirmDelete(false);
            }}
            disabled={loading}
            className="app-text-control-sm rounded px-3 py-1.5 font-medium text-app-danger-text transition-colors hover:bg-app-danger/20"
          >
            {t('pms.bulk.deleteConfirm', { count: ids.length })}
          </button>
          <button
            type="button"
            onClick={() => setConfirmDelete(false)}
            className="app-text-control-sm px-2 py-1.5 text-white/40 transition-colors hover:text-white"
          >
            {t('common:actions.cancel')}
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => {
            if (!warnIfOverBulkLimit()) setConfirmDelete(true);
          }}
          disabled={loading}
          className="app-text-control-sm flex items-center gap-1 rounded px-3 py-1.5 text-white/60 transition-colors hover:bg-white/10 hover:text-app-danger-text"
        >
          <Trash2 size={13} />
          {t('common:actions.delete')}
        </button>
      )}

      <div className="h-4 w-px bg-app-border-strong mx-1" />

      {/* Close */}
      <button
        type="button"
        onClick={onDeselectAll}
        className="p-1 text-white/40 hover:text-white transition-colors"
        aria-label={t('pms.bulk.closeSelection')}
      >
        <X size={14} />
      </button>

      {/* The dialog renders in a portal, so it does not sit inside the bar. */}
      <TaskCopyDialog
        workspaceSlug={workspaceSlug}
        isOpen={copyOpen}
        onClose={() => setCopyOpen(false)}
        onCopied={handleCopied}
        sourceTaskListId={taskListId}
        taskIds={ids}
      />
    </div>
  );
};
