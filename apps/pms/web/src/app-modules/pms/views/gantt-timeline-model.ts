import { addGanttDays, diffGanttDays } from './gantt-view-model';

/** How much time one column of the Gantt time axis covers. */
export type GanttScale = 'day' | 'week' | 'month';

export const GANTT_SCALES: readonly GanttScale[] = ['day', 'week', 'month'];

type GanttSpanUnit = GanttScale | 'year';

// The narrowest column that still reads. Columns widen to fill a wide screen
// but never shrink below this; the chart scrolls sideways instead (#44).
// At these widths a month of days and a year of months fit a 1600px screen.
const MIN_COLUMN_WIDTH: Record<GanttScale, number> = {
  day: 24,
  week: 56,
  month: 64,
};
const AVERAGE_COLUMN_DAYS: Record<GanttScale, number> = {
  day: 1,
  week: 7,
  month: 30.4375,
};

// Months and years label the row above day/week and month columns.
const GROUP_UNIT: Record<GanttScale, GanttSpanUnit> = {
  day: 'month',
  week: 'month',
  month: 'year',
};

/** One header cell: where it starts and how many days it covers. */
export type GanttTimelineSpan = {
  dayCount: number;
  dayOffset: number;
  start: Date;
};

export type GanttTimeline = {
  dayCount: number;
  groups: GanttTimelineSpan[];
  units: GanttTimelineSpan[];
};

/** Scale a freshly picked range opens with: roughly 30 columns or fewer. */
export function getDefaultGanttScale(dayCount: number): GanttScale {
  if (dayCount <= 62) return 'day';
  if (dayCount <= 186) return 'week';
  return 'month';
}

export function getGanttDayWidth({
  availableWidth,
  dayCount,
  scale,
}: {
  availableWidth: number;
  dayCount: number;
  scale: GanttScale;
}): number {
  const minDayWidth = MIN_COLUMN_WIDTH[scale] / AVERAGE_COLUMN_DAYS[scale];
  if (dayCount <= 0 || availableWidth <= 0) return minDayWidth;
  return Math.max(minDayWidth, availableWidth / dayCount);
}

function nextSpanStart(value: Date, unit: GanttSpanUnit): Date {
  if (unit === 'day') return addGanttDays(value, 1);
  if (unit === 'week') {
    // Weeks start on Monday.
    const daysSinceMonday = (value.getDay() + 6) % 7;
    return addGanttDays(value, 7 - daysSinceMonday);
  }
  if (unit === 'month') {
    return new Date(value.getFullYear(), value.getMonth() + 1, 1);
  }
  return new Date(value.getFullYear() + 1, 0, 1);
}

function buildSpans(
  start: Date,
  end: Date,
  unit: GanttSpanUnit,
): GanttTimelineSpan[] {
  const spans: GanttTimelineSpan[] = [];
  const rangeEnd = addGanttDays(end, 1);
  for (let cursor = start; cursor < rangeEnd; ) {
    const next = nextSpanStart(cursor, unit);
    const spanEnd = next < rangeEnd ? next : rangeEnd;
    spans.push({
      dayCount: diffGanttDays(spanEnd, cursor),
      dayOffset: diffGanttDays(cursor, start),
      start: cursor,
    });
    cursor = next;
  }
  return spans;
}

/** Header cells for the visible range; edge cells are cut to the range. */
export function buildGanttTimeline(
  start: Date,
  end: Date,
  scale: GanttScale,
): GanttTimeline {
  if (end < start) return { dayCount: 0, groups: [], units: [] };
  return {
    dayCount: diffGanttDays(end, start) + 1,
    groups: buildSpans(start, end, GROUP_UNIT[scale]),
    units: buildSpans(start, end, scale),
  };
}

/** Days to keep in view before today when the chart scrolls to it. */
export const GANTT_TODAY_LEAD_DAYS: Record<GanttScale, number> = {
  day: 3,
  week: 14,
  month: 31,
};
