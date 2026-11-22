/**
 * The one definition of the portal ↔ app frame bridge (docs/APP-FRAMES.md §3.4).
 * The bridge carries screen state only: location, navigation, input state, display
 * settings, run state and the one-time renew code. It never carries API requests, URLs to
 * fetch, cookies or portal CSRF values.
 */
export const PROTOCOL = "miy.frame/v1" as const;
/** Upper bound of one serialized message. */
export const MAX_MESSAGE_BYTES = 16 * 1024;

export interface FrameLocation {
  pathname: string;
  search: string;
  hash: string;
}
/** `unknown` until the current frame reports; a legacy guard keeps it unknown. */
export type InputState = "clean" | "dirty" | "unknown";

export type RenewalFailure =
  | "frame_session_ended"
  | "frame_access_denied"
  | "release_changed_reload_required"
  | "token_renewal_unavailable";

/** Only the renewal endpoints can establish why a frame cannot continue. */
export function renewalFailure(status: number, code?: unknown): RenewalFailure {
  if (status === 409 && code === "release_changed_reload_required") return code;
  if (status === 401 || status === 404) return "frame_session_ended";
  if (status === 403) return "frame_access_denied";
  return "token_renewal_unavailable";
}

/**
 * Display settings a person chooses in the portal. Values and names follow
 * AI-DO's personal settings: theme preference, language, time zone and date format.
 */
export const THEME_PREFERENCES = ["system", "light", "dark"] as const;
export type ThemePreference = (typeof THEME_PREFERENCES)[number];
export const LOCALES = ["ko-KR", "en-US"] as const;
export type Locale = (typeof LOCALES)[number];
export const DATE_FORMATS = ["korean", "iso", "us", "european", "locale"] as const;
export type DateFormat = (typeof DATE_FORMATS)[number];
export interface FrameSettings {
  theme: ThemePreference;
  locale: Locale;
  /** IANA time zone name. */
  timeZone: string;
  dateFormat: DateFormat;
}
export const DEFAULT_SETTINGS: FrameSettings = {
  theme: "system",
  locale: "ko-KR",
  timeZone: "Asia/Seoul",
  dateFormat: "korean",
};

/** Messages the portal sends into a frame. */
export type ParentMessage =
  | { type: "route.changed"; payload: FrameLocation }
  | { type: "settings.changed"; payload: FrameSettings }
  | { type: "token.code"; payload: { code: string } }
  | { type: "token.failed"; payload: { reason: RenewalFailure } };

/** Messages a frame sends to its portal. */
export type FrameMessage =
  | { type: "frame.ready"; payload: Record<string, never> }
  | { type: "frame.error"; payload: { code: string } }
  | { type: "frame.relaunch"; payload: Record<string, never> }
  | { type: "release.changed"; payload: Record<string, never> }
  | { type: "route.navigate"; payload: { path: string; replace: boolean } }
  | { type: "guard.state"; payload: { state: InputState; message?: string } }
  | { type: "token.renew"; payload: Record<string, never> }
  /**
   * A platform keyboard shortcut pressed while the frame has focus. Keys typed in
   * a frame never reach the portal document, so the runtime forwards only these.
   */
  | { type: "shortcut"; payload: { name: PlatformShortcut } };

/** Platform shortcuts a frame forwards: Ctrl/⌘+K opens the command palette. */
export const PLATFORM_SHORTCUTS = ["commandPalette"] as const;
export type PlatformShortcut = (typeof PLATFORM_SHORTCUTS)[number];

/** Whether a key event is a platform shortcut, and which. */
export function platformShortcut(event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">): PlatformShortcut | null {
  return (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === "k"
    ? "commandPalette"
    : null;
}

export type Envelope<M> = M & { protocol: typeof PROTOCOL; id: string };

let sequence = 0;
export function envelope<M extends ParentMessage | FrameMessage>(message: M): Envelope<M> {
  sequence += 1;
  return { ...message, protocol: PROTOCOL, id: `${Date.now().toString(36)}-${sequence}` };
}

type Shape = Record<string, unknown>;
const object = (value: unknown): value is Shape =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown, max = 2048): value is string =>
  typeof value === "string" && value.length <= max && !/[\u0000-\u001f\u007f]/.test(value);
const empty = (value: unknown) => object(value) && Object.keys(value).length === 0;

function header(data: unknown): Shape | null {
  if (!object(data) || data.protocol !== PROTOCOL || !text(data.id, 64) || !text(data.type, 32))
    return null;
  try {
    if (JSON.stringify(data).length > MAX_MESSAGE_BYTES) return null;
  } catch {
    return null;
  }
  return data;
}

/** An app-relative location: starts with one `/`, no scheme, backslash or control. */
export function relativePath(path: unknown): path is string {
  return text(path) && path.startsWith("/") && !path.startsWith("//") && !path.includes("\\");
}
export function location(value: unknown): value is FrameLocation {
  return (
    object(value) &&
    relativePath(value.pathname) &&
    text(value.search) &&
    (value.search === "" || value.search.startsWith("?")) &&
    text(value.hash) &&
    (value.hash === "" || value.hash.startsWith("#"))
  );
}

const oneOf = <T extends string>(values: readonly T[], value: unknown): value is T =>
  typeof value === "string" && (values as readonly string[]).includes(value);
/** An IANA-shaped zone name. Whether the zone exists is the reader's concern. */
export function timeZoneName(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z][A-Za-z0-9_+\-]*(\/[A-Za-z0-9_+\-]+)*$/.test(value) && value.length <= 64;
}
/** Complete, valid display settings, or null. */
export function settings(value: unknown): FrameSettings | null {
  return object(value) &&
    oneOf(THEME_PREFERENCES, value.theme) &&
    oneOf(LOCALES, value.locale) &&
    timeZoneName(value.timeZone) &&
    oneOf(DATE_FORMATS, value.dateFormat)
    ? { theme: value.theme, locale: value.locale, timeZone: value.timeZone, dateFormat: value.dateFormat }
    : null;
}

/** A validated message from the parent, or null (ignored and counted by callers). */
export function readParentMessage(data: unknown): ParentMessage | null {
  const m = header(data);
  if (!m) return null;
  const p = m.payload;
  switch (m.type) {
    case "route.changed":
      return location(p) ? { type: m.type, payload: { pathname: p.pathname, search: p.search, hash: p.hash } } : null;
    case "settings.changed": {
      const value = settings(p);
      return value ? { type: m.type, payload: value } : null;
    }
    case "token.code":
      return object(p) && text(p.code, 64) ? { type: m.type, payload: { code: p.code } } : null;
    case "token.failed":
      return object(p) &&
        (p.reason === "frame_session_ended" || p.reason === "frame_access_denied" ||
          p.reason === "release_changed_reload_required" || p.reason === "token_renewal_unavailable")
        ? { type: m.type, payload: { reason: p.reason } } : null;
  }
  return null;
}

/** A validated message from a frame, or null (ignored and counted by callers). */
export function readFrameMessage(data: unknown): FrameMessage | null {
  const m = header(data);
  if (!m) return null;
  const p = m.payload;
  switch (m.type) {
    case "frame.ready":
    case "frame.relaunch":
    case "release.changed":
    case "token.renew":
      return empty(p) ? ({ type: m.type, payload: {} } as FrameMessage) : null;
    case "frame.error":
      return object(p) && text(p.code, 64) ? { type: m.type, payload: { code: p.code } } : null;
    case "route.navigate":
      return object(p) && relativePath(p.path) && typeof p.replace === "boolean"
        ? { type: m.type, payload: { path: p.path, replace: p.replace } }
        : null;
    case "guard.state":
      return object(p) &&
        (p.state === "clean" || p.state === "dirty" || p.state === "unknown") &&
        (p.message === undefined || text(p.message, 512))
        ? { type: m.type, payload: { state: p.state, ...(p.message ? { message: p.message } : {}) } }
        : null;
    case "shortcut":
      return object(p) && (PLATFORM_SHORTCUTS as readonly unknown[]).includes(p.name)
        ? { type: m.type, payload: { name: p.name as PlatformShortcut } }
        : null;
  }
  return null;
}
