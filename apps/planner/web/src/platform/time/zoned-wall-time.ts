import { formatNativeDateInputParts } from './native-date-input';
import { getZonedDateParts } from './zoned-date-formatter';

export function formatZonedWallTime(date: Date, timeZone: string): string {
  const parts = getZonedDateParts(date, timeZone);
  if (!parts) return '';
  const clock = [parts.hour, parts.minute].map((part) => String(part).padStart(2, '0')).join(':');
  return `${formatNativeDateInputParts(parts)}T${clock}`;
}

/**
 * Resolve a civil minute in the given zone. For repeated minutes use the first
 * occurrence. For dates with a clock jump at midnight use the first valid
 * time after the gap, so calendar range boundaries still cover that day.
 */
export function resolveZonedWallTime(value: string, timeZone: string): Date {
  const nominal = Date.parse(`${value}Z`);
  if (!Number.isFinite(nominal) || new Date(nominal).toISOString().slice(0, 16) !== value)
    return new Date(NaN);

  const day = 24 * 60 * 60 * 1000;
  const candidates = [nominal - day, nominal, nominal + day].map((sample) => {
    const inZone = formatZonedWallTime(new Date(sample), timeZone);
    return nominal - (Date.parse(`${inZone}Z`) - sample);
  }).sort((first, second) => first - second);
  const exact = candidates.find((instant) => formatZonedWallTime(new Date(instant), timeZone) === value);
  if (exact !== undefined) return new Date(exact);
  const after = candidates.find((instant) => formatZonedWallTime(new Date(instant), timeZone) > value);
  return new Date(after ?? NaN);
}
