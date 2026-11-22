/** Display only: server limits and byte accounting remain authoritative. */
export function formatByteSize(bytes: number, locale = "ko-KR"): string {
  if (!Number.isFinite(bytes) || bytes < 0)
    throw new RangeError("Byte size must be a non-negative finite number");
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let value = bytes,
    unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: unit ? 1 : 0 }).format(value)} ${units[unit]}`;
}

/**
 * A random version 4 UUID for request and client identifiers.
 * `crypto.randomUUID` exists only in a secure context. An app frame served over
 * internal HTTP is not one, while `getRandomValues` works in both.
 */
export function requestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (value) =>
    value.toString(16).padStart(2, "0"),
  ).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** Returns a new list. Invalid or identical positions leave the original order intact. */
export function moveItem<T>(
  items: readonly T[],
  from: number,
  to: number,
): T[] {
  const next = [...items];
  if (
    !Number.isInteger(from) ||
    !Number.isInteger(to) ||
    from < 0 ||
    to < 0 ||
    from >= next.length ||
    to >= next.length ||
    from === to
  )
    return next;
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}
