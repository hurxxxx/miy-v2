import {
  MessageSquare,
  Flag,
  Eye,
  CheckSquare,
  CornerDownRight,
  GitBranch,
  ArrowDown,
  ArrowUp,
} from 'lucide-react';
import { Badge, Button } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { DateInput } from '../../../components/date/DateInput';
import {
  customFieldParam,
  type PmsCustomField,
  type PmsTask,
  type PmsTaskListStatus,
  type PmsTaskSort,
} from '../api/pms-api';
import type { PmsTaskListMember } from '../api/pms-api';
import { CustomFieldValueEditor } from './CustomFieldValueEditor';
import { formatCustomFieldValue } from './custom-field-model';
import {
  getStatusTone,
  PRIORITY_COLOR,
  formatDate,
} from './pms-constants';
import { StatusIconGlyph } from './StatusIcon';
import { TaskCard } from './TaskCard';
import { TaskAssigneeStack } from './TaskAssigneeStack';
import { TaskSelectAllCheckbox } from './TaskSelectAllCheckbox';
import { buildTaskTablePresentationModel } from './task-list-presentation-model';

export const TableView = ({
  tasks,
  onSelectIssue,
  selectedIds,
  onToggleSelect,
  onSelectTasks,
  members,
  taskListStatuses,
  canEdit = false,
  onUpdateIssue,
  customFields = [],
  onUpdateCustomField,
  sort,
  onSortChange,
}: {
  tasks: PmsTask[];
  onSelectIssue: (task: PmsTask) => void;
  selectedIds?: Set<string>;
  onToggleSelect?: (taskId: string) => void;
  /** Selects or clears several rows at once (the header checkbox). */
  onSelectTasks?: (taskIds: readonly string[], selected: boolean) => void;
  members?: readonly PmsTaskListMember[];
  taskListStatuses?: PmsTaskListStatus[];
  canEdit?: boolean;
  onUpdateIssue?: (
    taskId: string,
    payload: Record<string, unknown>,
  ) => Promise<void> | void;
  /** miy: the list's custom fields as columns, edited in place. */
  customFields?: readonly PmsCustomField[];
  onUpdateCustomField?: (
    taskId: string,
    fieldId: string,
    value: string,
  ) => Promise<boolean> | void;
  /** The list sort; a custom field header sorts by that field. */
  sort?: PmsTaskSort;
  onSortChange?: (sort: PmsTaskSort) => void;
}) => {
  const { t } = useTranslation('apps');
  const completionDateEditable = canEdit && Boolean(onUpdateIssue);
  const tableModel = buildTaskTablePresentationModel({
    taskListStatuses,
    tasks,
  });
  const rowIds = tableModel.rows.map((row) => row.task.id);
  return (
    <>
      <div className="space-y-2 lg:hidden">
        {tableModel.rows.map((row) => {
          const { childCount, task } = row;
          return (
            <TaskCard
              key={task.id}
              childCount={childCount}
              completedDateLabel={t('pms.taskDetail.completedDate')}
              depth={row.cardDepth}
              task={task}
              onSelectIssue={onSelectIssue}
              onToggleSelect={onToggleSelect}
              selectTaskLabel={(reference) =>
                t('pms.list.selectIssue', { reference })
              }
              selected={selectedIds?.has(task.id) ?? false}
              members={members}
              taskListStatuses={taskListStatuses}
              unassignedLabel={t('pms.taskDetail.unassigned')}
              viewDetailsLabel={(reference) =>
                t('pms.list.viewDetails', { reference })
              }
            />
          );
        })}
      </div>

      <div className="hidden overflow-hidden rounded-lg border border-app-border bg-app-surface lg:block">
        <div className="overflow-x-auto custom-scrollbar">
          <table className="app-text-body-sm w-full text-left">
            <thead>
              <tr className="app-text-overline border-b border-app-border bg-app-surface-sidebar/50 text-app-ink/50">
                {onToggleSelect && (
                  <th className="w-10 px-4 py-3">
                    {onSelectTasks ? (
                      <TaskSelectAllCheckbox
                        label={t('pms.list.selectAllRows', {
                          count: rowIds.length,
                        })}
                        onChange={(selected) => onSelectTasks(rowIds, selected)}
                        selectedIds={selectedIds ?? new Set()}
                        taskIds={rowIds}
                      />
                    ) : (
                      <span className="sr-only">
                        {t('pms.bulk.selectAll', {
                          count: tableModel.rows.length,
                        })}
                      </span>
                    )}
                  </th>
                )}
                <th className="py-3 px-4 min-w-[250px]">{t('pms.taskName')}</th>
                <th className="py-3 px-4">{t('pms.list.status')}</th>
                <th className="py-3 px-4">{t('pms.list.assignee')}</th>
                <th className="py-3 px-4">{t('pms.list.dueDate')}</th>
                <th className="py-3 px-4">
                  {t('pms.taskDetail.completedDate')}
                </th>
                <th className="py-3 px-4">{t('pms.list.priority')}</th>
                <th className="py-3 px-4">{t('pms.bulk.labelsLabel')}</th>
                {customFields.map((field) => {
                  const sortKey = customFieldParam(field.id);
                  const sorted = sort?.field === sortKey ? sort.direction : null;
                  return (
                    <th
                      key={field.id}
                      aria-sort={
                        sorted === 'asc'
                          ? 'ascending'
                          : sorted === 'desc'
                            ? 'descending'
                            : undefined
                      }
                      className="py-3 px-4 min-w-[9rem]"
                    >
                      {onSortChange ? (
                        <button
                          type="button"
                          className="inline-flex items-center gap-1 uppercase hover:text-app-ink"
                          title={t('pms.customFields.sortBy', { name: field.name })}
                          onClick={() =>
                            onSortChange({
                              field: sortKey,
                              direction: sorted === 'asc' ? 'desc' : 'asc',
                            })
                          }
                        >
                          {field.name}
                          {sorted === 'asc' ? <ArrowUp size={11} /> : null}
                          {sorted === 'desc' ? <ArrowDown size={11} /> : null}
                        </button>
                      ) : (
                        field.name
                      )}
                    </th>
                  );
                })}
                <th className="py-3 px-4 text-right">
                  {t('pms.table.actions')}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-app-border">
              {tableModel.rows.map((row) => {
                const { childCount, progress, statusLabel, tableDepth, task } =
                  row;
                return (
                  <tr
                    key={task.id}
                    onClick={() => onSelectIssue(task)}
                    className="hover:bg-app-surface-hover transition-colors group cursor-pointer"
                  >
                    {onToggleSelect && (
                      // The whole cell selects the row, not just the checkbox.
                      <td
                        className="cursor-pointer py-3 px-4"
                        onClick={(e) => {
                          e.stopPropagation();
                          onToggleSelect(task.id);
                        }}
                      >
                        <input
                          aria-label={t('pms.list.selectIssue', {
                            reference: task.reference,
                          })}
                          type="checkbox"
                          checked={selectedIds?.has(task.id) ?? false}
                          onChange={(e) => {
                            e.stopPropagation();
                            onToggleSelect(task.id);
                          }}
                          onClick={(e) => e.stopPropagation()}
                          className="size-3.5 cursor-pointer rounded accent-app-accent"
                        />
                      </td>
                    )}
                    <td className="py-3 px-4">
                      <div
                        className="flex min-w-0 items-center gap-2"
                        style={
                          tableDepth > 0
                            ? { paddingLeft: tableDepth * 20 }
                            : undefined
                        }
                      >
                        {tableDepth > 0 ? (
                          <CornerDownRight
                            size={13}
                            className="shrink-0 text-app-ink/35"
                          />
                        ) : null}
                        <span className="min-w-0 truncate font-medium text-app-ink">
                          {task.title}
                        </span>
                        {childCount > 0 ? (
                          <span className="app-text-micro inline-flex items-center gap-1 text-app-ink/35">
                            <GitBranch size={11} />
                            {childCount}
                          </span>
                        ) : null}
                        {task.comments_count > 0 && (
                          <div className="flex items-center gap-1 text-app-ink/40">
                            <MessageSquare size={12} />
                            <span className="app-text-micro">
                              {task.comments_count}
                            </span>
                          </div>
                        )}
                        {progress.hasChecklistProgress && (
                          <div className="flex items-center gap-0.5 text-app-ink/40">
                            <CheckSquare size={11} />
                            <span className="app-text-micro">
                              {progress.checklistProgressLabel}
                            </span>
                          </div>
                        )}
                      </div>
                    </td>
                    <td className="py-3 px-4">
                      <span className="inline-flex items-center gap-2">
                        <StatusIconGlyph
                          label={statusLabel}
                          status={task.status}
                          taskListStatuses={taskListStatuses}
                        />
                        <Badge
                          tone={getStatusTone(task.status, taskListStatuses)}
                        >
                          {statusLabel}
                        </Badge>
                      </span>
                    </td>
                    <td className="py-3 px-4">
                      <TaskAssigneeStack
                        members={members}
                        showName
                        task={task}
                        unassignedLabel="-"
                      />
                    </td>
                    <td className="py-3 px-4 text-app-ink/60">
                      {formatDate(task.due_date) || '-'}
                    </td>
                    <td className="py-3 px-4 text-app-ink/60">
                      {completionDateEditable ? (
                        <div onClick={(event) => event.stopPropagation()}>
                          <DateInput
                            aria-label={t('pms.taskDetail.completedDate')}
                            className="app-text-caption min-w-[7.5rem] rounded-md border border-app-border bg-app-surface-sidebar px-2 py-1 text-app-ink"
                            onValueChange={(value) => {
                              void onUpdateIssue?.(task.id, {
                                completed_date: value || null,
                              });
                            }}
                            value={task.completed_date}
                          />
                        </div>
                      ) : (
                        formatDate(task.completed_date) || '-'
                      )}
                    </td>
                    <td className="py-3 px-4">
                      <div className="flex items-center gap-2">
                        <Flag
                          size={14}
                          className={
                            PRIORITY_COLOR[task.priority] ?? 'text-app-ink/55'
                          }
                        />
                        <span className="text-app-ink/50">
                          {task.priority_label}
                        </span>
                      </div>
                    </td>
                    <td className="py-3 px-4">
                      <div className="flex flex-wrap gap-1">
                        {task.labels.map((label) => (
                          <Badge key={label.id} tone="neutral">
                            {label.name}
                          </Badge>
                        ))}
                      </div>
                    </td>
                    {customFields.map((field) => {
                      const value = task.custom_fields?.[field.id];
                      return (
                        <td
                          key={field.id}
                          className="py-3 px-4 text-app-ink/70"
                        >
                          {canEdit && onUpdateCustomField ? (
                            <div onClick={(event) => event.stopPropagation()}>
                              <CustomFieldValueEditor
                                compact
                                field={field}
                                members={members}
                                onCommit={(next) =>
                                  onUpdateCustomField(task.id, field.id, next)
                                }
                                value={value}
                              />
                            </div>
                          ) : (
                            formatCustomFieldValue(field, value, members) || '-'
                          )}
                        </td>
                      );
                    })}
                    <td className="py-3 px-4 text-right">
                      <Button
                        aria-label={t('pms.list.viewDetails', {
                          reference: task.reference,
                        })}
                        title={t('pms.list.viewDetails', {
                          reference: task.reference,
                        })}
                        variant="ghost"
                        size="icon"
                        className="opacity-0 group-hover:opacity-100 focus:opacity-100"
                        onClick={(event) => {
                          event.stopPropagation();
                          onSelectIssue(task);
                        }}
                      >
                        <Eye size={14} />
                      </Button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
};
