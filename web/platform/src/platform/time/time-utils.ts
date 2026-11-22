// Ported from AI-DO apps/web/src/platform/time/time-utils.ts (0002af062). The
// stored preference is the portal's display settings
// (data/display-settings-store.ts) instead of AI-DO's separate time zone and
// date format keys.
import { currentSettings } from '../../data/display-settings-store';
import {
  formatDateOnlyWithPreference,
  formatDateTimeWithPreference,
  formatRelativeTimeWithPreference,
  normalizeDateFormatPreference,
  normalizeTimeZone,
  type DateFormatPreference,
  type RelativeTimeOptions,
  type TemporalInput,
  type ZonedFormatOptions,
} from './zoned-date-formatter';

export {
  DATE_DISPLAY_LOCALE,
  DATE_FORMAT_OPTIONS,
  DEFAULT_TIME_ZONE,
  TIME_ZONE_OPTIONS,
  diffDateOnlyDays,
  getZonedDateParts,
  isSameDateInTimeZone,
  normalizeDateFormatPreference,
  normalizeTimeZone,
  parseApiDateTime,
  parseDateOnlyParts,
  zonedDateKey,
} from './zoned-date-formatter';
export type {
  DateFormatPreference,
  RelativeTimeOptions,
  TemporalInput,
  ZonedFormatOptions,
} from './zoned-date-formatter';

function resolveDateFormat(
  value: string | null | undefined,
): DateFormatPreference {
  return value === null || value === undefined
    ? currentSettings().dateFormat
    : normalizeDateFormatPreference(value);
}

function resolveTimeZone(value: string | null | undefined): string {
  return value === null || value === undefined
    ? currentSettings().timeZone
    : normalizeTimeZone(value);
}

function resolveLocale(value: string | null | undefined): string {
  return value ?? currentSettings().locale;
}

export function formatDateOnly(
  value: string | null | undefined,
  options: ZonedFormatOptions = {},
): string {
  return formatDateOnlyWithPreference(value, {
    ...options,
    locale: resolveLocale(options.locale),
    dateFormat: resolveDateFormat(options.dateFormat),
  });
}

export function formatDateTime(
  value: TemporalInput,
  options: ZonedFormatOptions = {},
): string {
  return formatDateTimeWithPreference(value, {
    ...options,
    locale: resolveLocale(options.locale),
    dateFormat: resolveDateFormat(options.dateFormat),
    timeZone: resolveTimeZone(options.timeZone),
  });
}

export function formatRelativeTime(
  value: TemporalInput,
  options: RelativeTimeOptions = {},
): string {
  return formatRelativeTimeWithPreference(value, {
    ...options,
    locale: resolveLocale(options.locale),
    dateFormat: resolveDateFormat(options.dateFormat),
    timeZone: resolveTimeZone(options.timeZone),
  });
}
