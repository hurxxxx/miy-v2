import { formatZonedWallTime, resolveZonedWallTime } from '../../../platform/time/zoned-wall-time';
import { formatNativeDateInputValue } from '../../../platform/time/native-date-input';

export function defaultTimedRange(now = new Date(), timeZone?: string): { start: Date; end: Date } {
  if (timeZone) {
    const minute = Number(formatZonedWallTime(now, timeZone).slice(14, 16));
    const start = new Date(now.getTime() + (60 - minute) * 60_000 - now.getSeconds() * 1000 - now.getMilliseconds());
    return { start, end: new Date(start.getTime() + 60 * 60_000) };
  }
  const start = new Date(now);
  start.setMinutes(0, 0, 0);
  start.setHours(start.getHours() + 1);
  const end = new Date(start.getTime());
  end.setHours(end.getHours() + 1);
  return { start, end };
}

export function copyDateWithTime(date: Date, timeSource: Date): Date {
  return new Date(
    date.getFullYear(),
    date.getMonth(),
    date.getDate(),
    timeSource.getHours(),
    timeSource.getMinutes(),
    0,
    0,
  );
}

export function defaultTimedRangeForDate(
  date: Date,
  now = new Date(),
  timeZone?: string,
): { start: Date; end: Date } {
  if (timeZone) {
    const { start: nextHour } = defaultTimedRange(now, timeZone);
    const time = formatZonedWallTime(nextHour, timeZone).slice(11);
    const start = resolveZonedWallTime(`${formatNativeDateInputValue(date)}T${time}`, timeZone);
    return { start, end: new Date(start.getTime() + 60 * 60_000) };
  }
  const defaultRange = defaultTimedRange(now);
  return {
    start: copyDateWithTime(date, defaultRange.start),
    end: copyDateWithTime(date, defaultRange.end),
  };
}
