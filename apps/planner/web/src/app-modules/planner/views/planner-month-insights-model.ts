import type {
  CalendarEvent,
  CalendarSourceType,
} from '../../../platform/calendar/calendar-types';
import {
  addDays,
  formatLocalYmd,
  startOfLocalDay,
} from './planner-calendar-view-model';

// PMS tasks are dense (one per assignee task, often spanning weeks), so the
// month grid shows them as one "PMS N건" / "마감 N건" chip per day instead of
// one bar per task. Meetings and personal events stay individual.
export const PLANNER_DAY_GROUPED_SOURCES: ReadonlySet<CalendarSourceType> =
  new Set(['pms_block', 'pms_due']);

const DAY_GROUP_ID_PREFIX = 'day-group:';
const DATE_ONLY_PATTERN = /^\d{4}-\d{2}-\d{2}$/;

export interface PlannerDayGroup {
  id: string;
  sourceType: CalendarSourceType;
  date: Date;
  items: CalendarEvent[];
}

export interface PlannerMonthDayGroups {
  events: CalendarEvent[];
  groups: ReadonlyMap<string, PlannerDayGroup>;
}

export interface PlannerBusyDay {
  date: Date;
  count: number;
  sources: CalendarSourceType[];
}

function parseEventDate(value: string): Date {
  if (DATE_ONLY_PATTERN.test(value)) {
    const [year, month, day] = value.split('-').map(Number);
    return new Date(year, month - 1, day);
  }
  return new Date(value);
}

/** Local calendar days covered by the event (end is exclusive). */
export function getPlannerEventDaySpan(event: CalendarEvent): {
  first: Date;
  last: Date;
} {
  const first = startOfLocalDay(parseEventDate(event.start));
  const end = parseEventDate(event.end || event.start);
  const last = startOfLocalDay(new Date(end.getTime() - 1));
  return { first, last: last < first ? first : last };
}

function forEachDayInRange(
  event: CalendarEvent,
  rangeStart: Date,
  rangeEnd: Date,
  visit: (day: Date) => void,
) {
  const { first, last } = getPlannerEventDaySpan(event);
  const from = first < rangeStart ? startOfLocalDay(rangeStart) : first;
  for (let day = from; day <= last && day < rangeEnd; day = addDays(day, 1)) {
    visit(day);
  }
}

export function isPlannerDayGroupId(eventId: string): boolean {
  return eventId.startsWith(DAY_GROUP_ID_PREFIX);
}

export function buildPlannerMonthDayGroups(
  events: readonly CalendarEvent[],
  rangeStart: Date,
  rangeEnd: Date,
): PlannerMonthDayGroups {
  const passthrough: CalendarEvent[] = [];
  const groups = new Map<string, PlannerDayGroup>();

  for (const event of events) {
    if (!PLANNER_DAY_GROUPED_SOURCES.has(event.sourceType)) {
      passthrough.push(event);
      continue;
    }
    forEachDayInRange(event, rangeStart, rangeEnd, (day) => {
      const id = `${DAY_GROUP_ID_PREFIX}${event.sourceType}:${formatLocalYmd(day)}`;
      const group = groups.get(id);
      if (group) {
        group.items.push(event);
      } else {
        groups.set(id, {
          id,
          sourceType: event.sourceType,
          date: day,
          items: [event],
        });
      }
    });
  }

  const groupEvents = [...groups.values()].map(
    (group): CalendarEvent => ({
      id: group.id,
      title: group.items[0].title,
      start: formatLocalYmd(group.date),
      end: formatLocalYmd(addDays(group.date, 1)),
      allDay: true,
      sourceType: group.sourceType,
      sourceId: '',
      color: group.items[0].color,
      workspace: null,
      metadata: {},
    }),
  );

  return { events: [...groupEvents, ...passthrough], groups };
}

export function groupPlannerEventsBySource(
  events: readonly CalendarEvent[],
  rangeStart: Date,
  rangeEnd: Date,
): Record<CalendarSourceType, CalendarEvent[]> {
  const groups: Record<CalendarSourceType, CalendarEvent[]> = {
    meeting: [],
    pms_due: [],
    pms_block: [],
    planner_event: [],
    google_calendar: [],
  };
  for (const event of events) {
    const { first, last } = getPlannerEventDaySpan(event);
    if (first < rangeEnd && last >= startOfLocalDay(rangeStart)) {
      groups[event.sourceType].push(event);
    }
  }
  return groups;
}

export function countPlannerEventsBySource(
  events: readonly CalendarEvent[],
  rangeStart: Date,
  rangeEnd: Date,
): Record<CalendarSourceType, number> {
  const groups = groupPlannerEventsBySource(events, rangeStart, rangeEnd);
  return {
    meeting: groups.meeting.length,
    pms_due: groups.pms_due.length,
    pms_block: groups.pms_block.length,
    planner_event: groups.planner_event.length,
    google_calendar: groups.google_calendar.length,
  };
}

/** Google Calendar occurrences overlapping the range, per connected calendar id. */
export function countGoogleCalendarEvents(
  events: readonly CalendarEvent[],
  rangeStart: Date,
  rangeEnd: Date,
): Record<string, number> {
  const counts: Record<string, number> = {};
  const google = groupPlannerEventsBySource(
    events,
    rangeStart,
    rangeEnd,
  ).google_calendar;
  for (const event of google) {
    const calendarId = event.metadata.externalCalendarId;
    if (calendarId) counts[calendarId] = (counts[calendarId] ?? 0) + 1;
  }
  return counts;
}

export function rankPlannerBusyDays(
  events: readonly CalendarEvent[],
  rangeStart: Date,
  rangeEnd: Date,
  limit: number,
): PlannerBusyDay[] {
  const days = new Map<string, PlannerBusyDay>();
  for (const event of events) {
    forEachDayInRange(event, rangeStart, rangeEnd, (day) => {
      const key = formatLocalYmd(day);
      const entry = days.get(key) ?? { date: day, count: 0, sources: [] };
      entry.count += 1;
      if (!entry.sources.includes(event.sourceType)) {
        entry.sources.push(event.sourceType);
      }
      days.set(key, entry);
    });
  }
  return [...days.values()]
    .sort(
      (left, right) =>
        right.count - left.count || left.date.getTime() - right.date.getTime(),
    )
    .slice(0, limit);
}

/** Next meetings, deadlines and personal events (long PMS spans excluded). */
export function listPlannerUpcomingEvents(
  events: readonly CalendarEvent[],
  now: Date,
  limit: number,
): CalendarEvent[] {
  const today = startOfLocalDay(now);
  return events
    .filter((event) => event.sourceType !== 'pms_block')
    .filter((event) =>
      event.allDay || DATE_ONLY_PATTERN.test(event.start)
        ? getPlannerEventDaySpan(event).last >= today
        : parseEventDate(event.end || event.start) > now,
    )
    .sort(
      (left, right) =>
        parseEventDate(left.start).getTime() -
        parseEventDate(right.start).getTime(),
    )
    .slice(0, limit);
}

/** Parses a ``YYYY-MM-DD`` (or ISO) value as a local date. */
export function parsePlannerDate(value: string): Date {
  return parseEventDate(value);
}

export function getPlannerEventStartDate(event: CalendarEvent): Date {
  return parseEventDate(event.start);
}

/** Sunday-start week containing ``date`` (matches the month grid). */
export function getPlannerWeekRange(date: Date): { start: Date; end: Date } {
  const start = addDays(startOfLocalDay(date), -date.getDay());
  return { start, end: addDays(start, 7) };
}
