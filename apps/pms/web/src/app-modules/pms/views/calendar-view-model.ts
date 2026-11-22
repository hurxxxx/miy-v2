import {
  DEFAULT_TIME_ZONE,
  getZonedDateParts,
  parseDateOnlyParts,
  zonedDateKey,
} from '../../../platform/time/time-utils';
import type { UnifiedCalendarView } from '../../../components/calendar/UnifiedCalendar';
import type {
  CalendarEvent,
  CalendarWorkspaceRef,
} from '../../../platform/calendar/calendar-types';
import type { PmsTask, PmsTaskListStatus } from '../api/pms-api';
import {
  PMS_CALENDAR_FALLBACK_STATUS_COLOR,
  PMS_CALENDAR_STATUS_COLORS,
} from './pms-color-palettes';

type CalendarPmsTask = PmsTask & { workspace?: CalendarWorkspaceRef | null };

export function startOfCalendarMonth(value: Date): Date {
  return addCalendarMonths(value, 0);
}

export function addCalendarMonths(value: Date, months: number): Date {
  const parts = getZonedDateParts(value, DEFAULT_TIME_ZONE);
  if (!parts) return new Date(Number.NaN);
  const target = Date.UTC(parts.year, parts.month - 1 + months, 1);
  // Resolve the calendar's midnight, independent of the browser's timezone.
  const zoned = getZonedDateParts(new Date(target), DEFAULT_TIME_ZONE);
  if (!zoned) return new Date(Number.NaN);
  const offset =
    Date.UTC(zoned.year, zoned.month - 1, zoned.day, zoned.hour, zoned.minute) -
    target;
  return new Date(target - offset);
}

/** How much of the calendar the PMS toolbar shows at once. */
export type PmsCalendarRange = 'month' | 'week' | 'day';

export const PMS_CALENDAR_RANGES: readonly PmsCalendarRange[] = [
  'month',
  'week',
  'day',
];

export const PMS_CALENDAR_RANGE_VIEWS = {
  month: 'dayGridMonth',
  week: 'dayGridWeek',
  day: 'dayGridDay',
} as const satisfies Record<PmsCalendarRange, UnifiedCalendarView>;

export function pmsCalendarRangeForView(view: string): PmsCalendarRange {
  if (view === PMS_CALENDAR_RANGE_VIEWS.week) return 'week';
  if (view === PMS_CALENDAR_RANGE_VIEWS.day) return 'day';
  return 'month';
}

/** Days the toolbar works from, end exclusive. */
export type PmsCalendarPeriod = { end: Date; start: Date };

/** The whole month in month view (not the grid's spill-over days), the shown days otherwise. */
export function getPmsCalendarPeriod(
  range: PmsCalendarRange,
  shown: { currentDate: Date; rangeEnd: Date; rangeStart: Date },
): PmsCalendarPeriod {
  if (range === 'month') {
    const start = startOfCalendarMonth(shown.currentDate);
    return { end: addCalendarMonths(start, 1), start };
  }
  return { end: shown.rangeEnd, start: shown.rangeStart };
}

export function isTodayInPmsCalendarPeriod(
  period: PmsCalendarPeriod,
  today: Date = new Date(),
): boolean {
  return today >= period.start && today < period.end;
}

/** Where switching range lands: today when the shown period holds it, else the period's first day. */
export function resolvePmsCalendarRangeAnchor(
  period: PmsCalendarPeriod,
  today: Date = new Date(),
): Date {
  return isTodayInPmsCalendarPeriod(period, today) ? today : period.start;
}

export function formatPmsCalendarTitle(
  range: PmsCalendarRange,
  period: PmsCalendarPeriod,
  locale: string,
): string {
  if (range === 'month') {
    return new Intl.DateTimeFormat(locale, {
      timeZone: DEFAULT_TIME_ZONE,
      month: 'long',
      year: 'numeric',
    }).format(period.start);
  }
  if (range === 'day') {
    return new Intl.DateTimeFormat(locale, {
      timeZone: DEFAULT_TIME_ZONE,
      day: 'numeric',
      month: 'long',
      weekday: 'long',
      year: 'numeric',
    }).format(period.start);
  }
  // The period end is exclusive; its preceding instant belongs to the last day.
  const lastDay = new Date(period.end.getTime() - 1);
  return new Intl.DateTimeFormat(locale, {
    timeZone: DEFAULT_TIME_ZONE,
    day: 'numeric',
    month: 'long',
    year: 'numeric',
  }).formatRange(period.start, lastDay);
}

/** Whether any task bar touches the period, so an empty week or day can say so. */
export function hasCalendarEventInPeriod(
  events: CalendarEvent[],
  period: PmsCalendarPeriod,
): boolean {
  const start = zonedDateKey(period.start, DEFAULT_TIME_ZONE);
  const end = zonedDateKey(period.end, DEFAULT_TIME_ZONE);
  return events.some((event) => event.start < end && event.end > start);
}

export function getCalendarTaskStatusColor(
  slug: string,
  taskListStatuses?: PmsTaskListStatus[],
): string {
  if (PMS_CALENDAR_STATUS_COLORS[slug]) return PMS_CALENDAR_STATUS_COLORS[slug];
  const status = taskListStatuses?.find((item) => item.slug === slug);
  return status?.color ?? PMS_CALENDAR_FALLBACK_STATUS_COLOR;
}

export function buildPmsCalendarEvents(
  tasks: CalendarPmsTask[],
  taskListStatuses?: PmsTaskListStatus[],
  titleFor?: (task: CalendarPmsTask) => string,
): CalendarEvent[] {
  return tasks
    .map((task) => buildPmsCalendarEvent(task, taskListStatuses, titleFor))
    .filter((event): event is CalendarEvent => event !== null)
    .sort((left, right) => {
      const startDiff = left.start.localeCompare(right.start);
      if (startDiff !== 0) return startDiff;
      const endDiff = left.end.localeCompare(right.end);
      if (endDiff !== 0) return endDiff;
      return left.title.localeCompare(right.title);
    });
}

function buildPmsCalendarEvent(
  task: CalendarPmsTask,
  taskListStatuses?: PmsTaskListStatus[],
  titleFor?: (task: CalendarPmsTask) => string,
): CalendarEvent | null {
  const range = getCalendarTaskRange(task);
  if (!range) return null;

  return {
    id: `pms-task-${task.id}`,
    title: titleFor ? titleFor(task) : task.title,
    start: range.start,
    end: addDateOnlyDays(range.end, 1),
    allDay: true,
    sourceType: 'pms_due',
    sourceId: task.id,
    color: getCalendarTaskDisplayColor(task.status, taskListStatuses),
    workspace: task.workspace ?? null,
    metadata: {
      taskListId: task.list_id,
      status: task.status,
      assigneeIds: task.assignee_ids,
    },
  };
}

function getCalendarTaskRange(
  task: PmsTask,
): { end: string; start: string } | null {
  const start = normalizeDateOnly(task.start_date);
  const due = normalizeDateOnly(task.due_date);
  const fallback = start ?? due;
  if (!fallback) return null;

  const rawStart = start ?? fallback;
  const rawEnd = due ?? fallback;
  return rawStart <= rawEnd
    ? { end: rawEnd, start: rawStart }
    : { end: rawStart, start: rawEnd };
}

function normalizeDateOnly(value: string | null | undefined): string | null {
  const parts = parseDateOnlyParts(value);
  if (!parts) return null;
  return formatDateOnly(parts.year, parts.month, parts.day);
}

function addDateOnlyDays(value: string, days: number): string {
  const parts = parseDateOnlyParts(value);
  if (!parts) return value;
  const date = new Date(Date.UTC(parts.year, parts.month - 1, parts.day + days));
  return formatDateOnly(
    date.getUTCFullYear(),
    date.getUTCMonth() + 1,
    date.getUTCDate(),
  );
}

function formatDateOnly(year: number, month: number, day: number): string {
  return [
    String(year).padStart(4, '0'),
    String(month).padStart(2, '0'),
    String(day).padStart(2, '0'),
  ].join('-');
}

function getCalendarTaskDisplayColor(
  slug: string,
  taskListStatuses?: PmsTaskListStatus[],
): string {
  const statusColor = getCalendarTaskStatusColor(slug, taskListStatuses);
  const rgb = parseHexColor(statusColor);
  if (!rgb) return `color-mix(in srgb, ${statusColor} 18%, white)`;
  return formatRgb(mixRgb(rgb, WHITE_RGB, 0.82));
}

const WHITE_RGB = { blue: 255, green: 255, red: 255 } as const;

function mixRgb(
  color: { blue: number; green: number; red: number },
  target: { blue: number; green: number; red: number },
  targetWeight: number,
): { blue: number; green: number; red: number } {
  const sourceWeight = 1 - targetWeight;
  return {
    blue: Math.round(color.blue * sourceWeight + target.blue * targetWeight),
    green: Math.round(color.green * sourceWeight + target.green * targetWeight),
    red: Math.round(color.red * sourceWeight + target.red * targetWeight),
  };
}

function formatRgb(color: {
  blue: number;
  green: number;
  red: number;
}): string {
  return `rgb(${color.red}, ${color.green}, ${color.blue})`;
}

function parseHexColor(
  value: string | null | undefined,
): { blue: number; green: number; red: number } | null {
  if (!value) return null;
  const trimmed = value.trim();
  const shortMatch = /^#([0-9a-f]{3})$/i.exec(trimmed);
  const longMatch = /^#([0-9a-f]{6})$/i.exec(trimmed);
  const hex = shortMatch
    ? shortMatch[1]
        .split('')
        .map((item) => item + item)
        .join('')
    : longMatch?.[1];
  if (!hex) return null;

  return {
    blue: Number.parseInt(hex.slice(4, 6), 16),
    green: Number.parseInt(hex.slice(2, 4), 16),
    red: Number.parseInt(hex.slice(0, 2), 16),
  };
}
