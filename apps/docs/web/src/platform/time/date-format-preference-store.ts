import {
  DEFAULT_DATE_FORMAT,
  type DateFormatPreference,
  normalizeDateFormatPreference,
} from './zoned-date-formatter';

export const DATE_FORMAT_STORAGE_KEY = 'ai-do:date-format';

// miy: AppRoot syncs `runtime.settings.dateFormat` here on every render. A
// frame whose storage is blocked keeps the synced value in memory instead.
let unstoredDateFormat: DateFormatPreference | null = null;

export function readStoredDateFormatPreference(): DateFormatPreference {
  if (typeof window === 'undefined') return DEFAULT_DATE_FORMAT;
  try {
    return normalizeDateFormatPreference(
      window.localStorage.getItem(DATE_FORMAT_STORAGE_KEY),
    );
  } catch {
    return unstoredDateFormat ?? DEFAULT_DATE_FORMAT;
  }
}

function persistDateFormatPreference(dateFormat: DateFormatPreference): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(DATE_FORMAT_STORAGE_KEY, dateFormat);
  } catch {
    unstoredDateFormat = dateFormat;
  }
}

export function syncDateFormatPreference(
  value: string | null | undefined,
): DateFormatPreference {
  const normalized = normalizeDateFormatPreference(value);
  persistDateFormatPreference(normalized);
  return normalized;
}
