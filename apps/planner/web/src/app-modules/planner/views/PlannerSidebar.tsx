import { useState, type ReactNode } from 'react';
import { Check, Minus, Pencil } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../../lib/utils';
import type { CalendarSourceType } from '../../../platform/calendar/calendar-types';
import {
  PlannerMiniMonth,
  type PlannerMiniMonthProps,
} from './PlannerMiniMonth';

export interface PlannerSidebarSource {
  type: CalendarSourceType;
  label: string;
  color: string;
  count: number;
  visible: boolean;
}

export interface PlannerSidebarCalendar {
  id: string;
  label: string;
  count: number;
  visible: boolean;
}

/** Connected Google Calendars, listed under their source as one group. */
export interface PlannerSidebarCalendarGroup {
  source: PlannerSidebarSource;
  calendars: PlannerSidebarCalendar[];
  onToggleCalendar: (calendarId: string) => void;
  /** Resolves when saved; rejects to keep the editor open. */
  onRenameCalendar: (calendarId: string, name: string) => Promise<void>;
}

interface PlannerSidebarProps {
  createSlot: ReactNode;
  syncSlot?: ReactNode;
  miniMonth: PlannerMiniMonthProps;
  sources: PlannerSidebarSource[];
  calendarGroup?: PlannerSidebarCalendarGroup | null;
  onToggleSource: (type: CalendarSourceType) => void;
  onToggleAll: (visible: boolean) => void;
}

function FilterCheckbox({
  checked,
  indeterminate = false,
  color,
  onChange,
  label,
}: {
  checked: boolean;
  indeterminate?: boolean;
  color: string;
  onChange: () => void;
  label?: string;
}) {
  const filled = checked || indeterminate;
  return (
    <>
      <input
        type="checkbox"
        className="peer sr-only"
        checked={checked}
        onChange={onChange}
        aria-label={label}
      />
      <span
        aria-hidden="true"
        className="flex size-[18px] shrink-0 items-center justify-center rounded-[4px] border-2 peer-focus-visible:ring-2 peer-focus-visible:ring-app-accent"
        style={{
          borderColor: color,
          backgroundColor: filled ? color : 'transparent',
        }}
      >
        {checked ? (
          <Check size={13} strokeWidth={3} className="text-app-accent-fg" />
        ) : indeterminate ? (
          <Minus size={13} strokeWidth={3} className="text-app-accent-fg" />
        ) : null}
      </span>
    </>
  );
}

function CalendarRow({
  calendar,
  color,
  onToggle,
  onRename,
}: {
  calendar: PlannerSidebarCalendar;
  color: string;
  onToggle: () => void;
  onRename: (name: string) => Promise<void>;
}) {
  const { t } = useTranslation('apps');
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState(false);

  const save = async () => {
    if (draft === null || saving) return;
    if (draft.trim() === calendar.label) {
      setDraft(null);
      return;
    }
    setSaving(true);
    try {
      await onRename(draft.trim());
      setDraft(null);
      setFailed(false);
    } catch {
      setFailed(true);
    } finally {
      setSaving(false);
    }
  };

  if (draft !== null) {
    return (
      <li className="flex items-center gap-2 py-1 pl-9 pr-3">
        <input
          autoFocus
          value={draft}
          maxLength={200}
          disabled={saving}
          aria-label={t('planner.sidebar.calendarName')}
          aria-invalid={failed || undefined}
          title={failed ? t('planner.sidebar.renameFailed') : undefined}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => void save()}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void save();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              setDraft(null);
              setFailed(false);
            }
          }}
          className={cn(
            'app-text-control-sm h-8 min-w-0 flex-1 rounded-lg border bg-app-surface px-2 text-app-ink outline-none focus:ring-2 focus:ring-app-accent',
            failed ? 'border-app-danger' : 'border-app-border',
          )}
        />
      </li>
    );
  }

  return (
    <li className="group flex items-center hover:bg-app-surface-hover">
      <label className="app-text-control-sm flex min-w-0 flex-1 cursor-pointer items-center gap-3 py-2 pl-9 text-app-ink">
        <FilterCheckbox
          checked={calendar.visible}
          color={color}
          onChange={onToggle}
        />
        <span className="flex-1 truncate" title={calendar.label}>
          {calendar.label}
        </span>
      </label>
      <button
        type="button"
        onClick={() => setDraft(calendar.label)}
        aria-label={t('planner.sidebar.rename', { name: calendar.label })}
        title={t('planner.sidebar.rename', { name: calendar.label })}
        className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 opacity-0 hover:text-app-ink focus-visible:opacity-100 group-hover:opacity-100"
      >
        <Pencil size={13} />
      </button>
      <span className="app-text-control-sm w-8 shrink-0 pr-3 text-right text-app-ink/55 tabular-nums">
        {calendar.count}
      </span>
    </li>
  );
}

export function PlannerSidebar({
  createSlot,
  syncSlot,
  miniMonth,
  sources,
  calendarGroup,
  onToggleSource,
  onToggleAll,
}: PlannerSidebarProps) {
  const { t } = useTranslation('apps');
  // Sources with items in the browsed month are listed; a hidden source stays
  // listed even when empty so it can always be turned back on. The connected
  // Google Calendars are shown as their own group instead.
  const groupType = calendarGroup?.source.type;
  const listedSources = sources.filter(
    (source) =>
      source.type !== groupType && (source.count > 0 || !source.visible),
  );
  const group = calendarGroup ?? null;
  const groupVisibleCount = group
    ? group.source.visible
      ? group.calendars.filter((calendar) => calendar.visible).length
      : 0
    : 0;
  const groupAllVisible = group
    ? group.source.visible && groupVisibleCount === group.calendars.length
    : true;
  const visibleCount =
    listedSources.filter((source) => source.visible).length +
    (group ? groupVisibleCount : 0);
  const allVisible =
    listedSources.every((source) => source.visible) && groupAllVisible;
  const total =
    listedSources.reduce((sum, source) => sum + source.count, 0) +
    (group ? group.source.count : 0);

  return (
    <aside className="flex w-60 shrink-0 flex-col gap-6 overflow-y-auto pb-6 pr-3">
      {createSlot}

      <PlannerMiniMonth {...miniMonth} />

      <section>
        <div className="mb-2 flex items-center justify-between pl-1">
          <h2 className="app-text-control font-semibold text-app-ink">
            {t('planner.sidebar.filter')}
          </h2>
          <button
            type="button"
            onClick={() => onToggleAll(true)}
            disabled={allVisible}
            className="app-text-caption text-app-ink/55 hover:text-app-ink disabled:opacity-50 disabled:hover:text-app-ink/55"
          >
            {t('planner.sidebar.selectAll')}
          </button>
        </div>
        <ul className="rounded-xl bg-app-surface py-1">
          <li>
            <label className="app-text-control-sm flex cursor-pointer items-center gap-3 px-3 py-2 font-semibold text-app-ink hover:bg-app-surface-hover">
              <FilterCheckbox
                checked={allVisible}
                indeterminate={!allVisible && visibleCount > 0}
                color="var(--ui-color-ink-subtle)"
                onChange={() => onToggleAll(!allVisible)}
              />
              <span className="size-2 shrink-0 rounded-full bg-app-ink/45" />
              <span className="flex-1 truncate">
                {t('planner.sidebar.all')}
              </span>
              <span className="text-app-ink/55 tabular-nums">{total}</span>
            </label>
          </li>
          {listedSources.map((source) => (
            <li key={source.type}>
              <label className="app-text-control-sm flex cursor-pointer items-center gap-3 px-3 py-2 text-app-ink hover:bg-app-surface-hover">
                <FilterCheckbox
                  checked={source.visible}
                  color={source.color}
                  onChange={() => onToggleSource(source.type)}
                />
                <span
                  className="size-2 shrink-0 rounded-full"
                  style={{ backgroundColor: source.color }}
                />
                <span className="flex-1 truncate">{source.label}</span>
                <span className="text-app-ink/55 tabular-nums">
                  {source.count}
                </span>
              </label>
            </li>
          ))}
          {group ? (
            <li className="mt-1 border-t border-app-border pt-1">
              <label className="app-text-control flex cursor-pointer items-center gap-3 px-3 py-2 font-semibold text-app-ink hover:bg-app-surface-hover">
                <FilterCheckbox
                  checked={groupAllVisible}
                  indeterminate={!groupAllVisible && groupVisibleCount > 0}
                  color={group.source.color}
                  onChange={() => onToggleSource(group.source.type)}
                />
                <span
                  className="size-2 shrink-0 rounded-full"
                  style={{ backgroundColor: group.source.color }}
                />
                <span className="flex-1 truncate">{group.source.label}</span>
                <span className="text-app-ink/55 tabular-nums">
                  {group.source.count}
                </span>
              </label>
              <ul>
                {group.calendars.map((calendar) => (
                  <CalendarRow
                    key={calendar.id}
                    calendar={{
                      ...calendar,
                      visible: group.source.visible && calendar.visible,
                    }}
                    color={group.source.color}
                    onToggle={() => group.onToggleCalendar(calendar.id)}
                    onRename={(name) =>
                      group.onRenameCalendar(calendar.id, name)
                    }
                  />
                ))}
              </ul>
            </li>
          ) : null}
        </ul>
      </section>

      {syncSlot}
    </aside>
  );
}
