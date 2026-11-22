import {
  formatDateTime,
  isSameDateInTimeZone,
  normalizeTimeZone,
  parseApiDateTime,
  readStoredTimeZonePreference,
} from '../../../platform/time/time-utils';

// null/undefined defer to the viewer's stored preference, matching formatDateTime.
function resolveDmTimeZone(timeZone: string | null | undefined): string {
  return normalizeTimeZone(timeZone ?? readStoredTimeZonePreference());
}

export function formatDmMessageTime(
  value: string,
  locale: string,
  timeZone: string | null | undefined,
): string {
  return formatDateTime(value, {
    fallback: value,
    hour: '2-digit',
    locale,
    minute: '2-digit',
    timeZone: resolveDmTimeZone(timeZone),
  });
}

/** Date in the user's date-format preference followed by the weekday, e.g. "2026. 10. 20. (화)". */
export function formatDmDateDivider(
  value: string,
  locale: string,
  timeZone: string | null | undefined,
): string {
  const zone = resolveDmTimeZone(timeZone);
  const date = formatDateTime(value, { dateStyle: 'medium', locale, timeZone: zone });
  const weekday = formatDateTime(value, { locale, timeZone: zone, weekday: 'short' });
  return date && weekday ? `${date} (${weekday})` : date;
}

/**
 * Ids of messages that open a new calendar day in the viewer's timezone.
 * Unparsable timestamps never open or break a day group.
 */
export function dmDateGroupStartIds(
  messages: ReadonlyArray<{ created_at: string; id: string }>,
  timeZone: string | null | undefined,
): Set<string> {
  const zone = resolveDmTimeZone(timeZone);
  const startIds = new Set<string>();
  let previous: string | null = null;
  for (const message of messages) {
    if (!parseApiDateTime(message.created_at)) continue;
    if (!previous || !isSameDateInTimeZone(previous, message.created_at, zone)) {
      startIds.add(message.id);
    }
    previous = message.created_at;
  }
  return startIds;
}
