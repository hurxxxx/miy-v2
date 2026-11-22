import { describe, expect, it } from 'vitest';

import type { CalendarEvent } from '../../../platform/calendar/calendar-types';

import {
  buildPlannerMonthDayGroups,
  countGoogleCalendarEvents,
  countPlannerEventsBySource,
  getPlannerWeekRange,
  isPlannerDayGroupId,
  listPlannerUpcomingEvents,
  rankPlannerBusyDays,
} from './planner-month-insights-model';

function event(overrides: Partial<CalendarEvent> = {}): CalendarEvent {
  return {
    id: 'event-1',
    sourceId: 'source-1',
    sourceType: 'meeting',
    title: 'Event',
    start: '2026-10-20',
    end: '2026-10-21',
    allDay: true,
    color: '#3b82f6',
    workspace: null,
    metadata: {},
    ...overrides,
  };
}

const RANGE_START = new Date(2026, 9, 18);
const RANGE_END = new Date(2026, 9, 25);

describe('buildPlannerMonthDayGroups', () => {
  it('groups PMS tasks per day and keeps meetings individual', () => {
    const block = event({
      id: 'pms-block-1',
      sourceType: 'pms_block',
      title: 'Long task',
      start: '2026-10-19',
      end: '2026-10-22',
    });
    const sameDayBlock = event({
      id: 'pms-block-2',
      sourceType: 'pms_block',
      start: '2026-10-20',
      end: '2026-10-21',
    });
    const meeting = event({
      id: 'meeting-1',
      allDay: false,
      start: '2026-10-20T10:00:00+09:00',
      end: '2026-10-20T11:00:00+09:00',
    });

    const result = buildPlannerMonthDayGroups(
      [block, sameDayBlock, meeting],
      RANGE_START,
      RANGE_END,
    );

    const groupEvents = result.events.filter((item) =>
      isPlannerDayGroupId(item.id),
    );
    expect(groupEvents.map((item) => item.start)).toEqual([
      '2026-10-19',
      '2026-10-20',
      '2026-10-21',
    ]);
    expect(
      result.groups
        .get('day-group:pms_block:2026-10-20')
        ?.items.map((item) => item.id),
    ).toEqual(['pms-block-1', 'pms-block-2']);
    expect(result.events).toContain(meeting);
  });

  it('clips spans to the visible range', () => {
    const result = buildPlannerMonthDayGroups(
      [
        event({
          sourceType: 'pms_due',
          start: '2026-10-01',
          end: '2026-10-19',
        }),
      ],
      RANGE_START,
      RANGE_END,
    );
    expect([...result.groups.keys()]).toEqual(['day-group:pms_due:2026-10-18']);
  });
});

describe('planner insights', () => {
  const events = [
    event({
      id: 'a',
      sourceType: 'pms_due',
      start: '2026-10-21',
      end: '2026-10-22',
    }),
    event({
      id: 'b',
      sourceType: 'pms_block',
      start: '2026-10-20',
      end: '2026-10-23',
    }),
    event({
      id: 'c',
      sourceType: 'planner_event',
      start: '2026-10-28',
      end: '2026-10-29',
    }),
  ];

  it('counts events overlapping a week by source', () => {
    const week = getPlannerWeekRange(new Date(2026, 9, 21));
    expect(week.start).toEqual(new Date(2026, 9, 18));
    expect(countPlannerEventsBySource(events, week.start, week.end)).toEqual({
      meeting: 0,
      pms_due: 1,
      pms_block: 1,
      planner_event: 0,
      google_calendar: 0,
    });
  });

  it('ranks the busiest days first', () => {
    const ranked = rankPlannerBusyDays(events, RANGE_START, RANGE_END, 2);
    expect(ranked[0]).toMatchObject({
      date: new Date(2026, 9, 21),
      count: 2,
      sources: ['pms_due', 'pms_block'],
    });
    expect(ranked[1].date).toEqual(new Date(2026, 9, 20));
  });

  it('lists upcoming items without long PMS spans', () => {
    const upcoming = listPlannerUpcomingEvents(
      events,
      new Date(2026, 9, 21, 12),
      5,
    );
    expect(upcoming.map((item) => item.id)).toEqual(['a', 'c']);
  });
});

describe('countGoogleCalendarEvents', () => {
  it('counts Google occurrences in the range per connected calendar', () => {
    const google = (id: string, calendarId: string | null, start: string) =>
      event({
        id,
        sourceType: 'google_calendar',
        start,
        end: start,
        allDay: false,
        metadata: { externalCalendarId: calendarId },
      });
    const events = [
      google('a', 'personal', '2026-10-20T01:00:00Z'),
      google('b', 'personal', '2026-10-21T01:00:00Z'),
      google('c', 'team', '2026-10-22T01:00:00Z'),
      google('outside', 'team', '2026-11-20T01:00:00Z'),
      google('unknown', null, '2026-10-20T01:00:00Z'),
      event({ id: 'meeting', metadata: { externalCalendarId: 'personal' } }),
    ];

    expect(countGoogleCalendarEvents(events, RANGE_START, RANGE_END)).toEqual({
      personal: 2,
      team: 1,
    });
  });
});
