import { describe, expect, it } from 'vitest';

import {
  buildGanttTimeline,
  getDefaultGanttScale,
  getGanttDayWidth,
} from './gantt-timeline-model';
import { toGanttDateOnly } from './gantt-view-model';

function spansOf(spans: { dayCount: number; start: Date }[]) {
  return spans.map((span) => [toGanttDateOnly(span.start), span.dayCount]);
}

describe('gantt timeline model', () => {
  it('opens one or two months in days, a quarter or half in weeks, a year in months', () => {
    expect(getDefaultGanttScale(31)).toBe('day');
    expect(getDefaultGanttScale(62)).toBe('day');
    expect(getDefaultGanttScale(92)).toBe('week');
    expect(getDefaultGanttScale(184)).toBe('week');
    expect(getDefaultGanttScale(365)).toBe('month');
  });

  it('never shrinks a column below a readable width and widens it to fill', () => {
    // 31 days in 486px would be 15.7px a day; the chart scrolls instead.
    expect(
      getGanttDayWidth({ availableWidth: 486, dayCount: 31, scale: 'day' }),
    ).toBe(24);
    expect(
      getGanttDayWidth({ availableWidth: 1240, dayCount: 31, scale: 'day' }),
    ).toBe(40);
    expect(
      getGanttDayWidth({ availableWidth: 0, dayCount: 31, scale: 'week' }),
    ).toBe(8);
  });

  it('cuts week columns at Mondays and labels them by month', () => {
    // 2026-06-01 is a Monday; 2026-07-02 is a Thursday.
    const timeline = buildGanttTimeline(
      new Date(2026, 5, 3),
      new Date(2026, 6, 2),
      'week',
    );

    expect(timeline.dayCount).toBe(30);
    expect(spansOf(timeline.units)).toEqual([
      ['2026-06-03', 5],
      ['2026-06-08', 7],
      ['2026-06-15', 7],
      ['2026-06-22', 7],
      ['2026-06-29', 4],
    ]);
    expect(spansOf(timeline.groups)).toEqual([
      ['2026-06-03', 28],
      ['2026-07-01', 2],
    ]);
    expect(timeline.units.map((unit) => unit.dayOffset)).toEqual([
      0, 5, 12, 19, 26,
    ]);
  });

  it('groups month columns by year', () => {
    const timeline = buildGanttTimeline(
      new Date(2026, 10, 1),
      new Date(2027, 1, 28),
      'month',
    );

    expect(spansOf(timeline.units)).toEqual([
      ['2026-11-01', 30],
      ['2026-12-01', 31],
      ['2027-01-01', 31],
      ['2027-02-01', 28],
    ]);
    expect(spansOf(timeline.groups)).toEqual([
      ['2026-11-01', 61],
      ['2027-01-01', 59],
    ]);
  });
});
