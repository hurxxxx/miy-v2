import { expect, it } from 'vitest';
import { resolveZonedWallTime } from './zoned-wall-time';

it('uses the beginning of a Seoul day in the calendar query', () => {
  const from = resolveZonedWallTime('2026-11-22T00:00', 'Asia/Seoul');
  const to = resolveZonedWallTime('2026-11-23T00:00', 'Asia/Seoul');
  expect(from.toISOString()).toBe('2026-11-21T15:00:00.000Z');
  expect(new Date('2026-11-21T23:00Z') >= from).toBe(true);
  expect(new Date('2026-11-21T23:30Z') < to).toBe(true);
});

it('chooses the first repeated minute and advances across a clock gap', () => {
  expect(resolveZonedWallTime('2026-11-01T01:30', 'America/New_York').toISOString())
    .toBe('2026-11-01T05:30:00.000Z');
  expect(resolveZonedWallTime('2026-03-08T02:30', 'America/New_York').toISOString())
    .toBe('2026-03-08T07:30:00.000Z');
});
