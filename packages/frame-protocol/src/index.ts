/**
 * The one definition of the portal ↔ app frame bridge (docs/APP-FRAMES.md §3.4).
 * The bridge carries screen state only: location, navigation, input state, theme,
 * run state and the one-time renew code. It never carries API requests, URLs to
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

/** Messages the portal sends into a frame. */
export type ParentMessage =
  | { type: "route.changed"; payload: FrameLocation }
  | { type: "theme.changed"; payload: { theme: "light" | "dark" } }
  | { type: "token.code"; payload: { code: string } }
  | { type: "token.failed"; payload: { reason: string } };

/** Messages a frame sends to its portal. */
export type FrameMessage =
  | { type: "frame.ready"; payload: Record<string, never> }
  | { type: "frame.error"; payload: { code: string } }
  | { type: "frame.relaunch"; payload: Record<string, never> }
  | { type: "release.changed"; payload: Record<string, never> }
  | { type: "route.navigate"; payload: { path: string; replace: boolean } }
  | { type: "guard.state"; payload: { state: InputState; message?: string } }
  | { type: "token.renew"; payload: Record<string, never> };

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

/** A validated message from the parent, or null (ignored and counted by callers). */
export function readParentMessage(data: unknown): ParentMessage | null {
  const m = header(data);
  if (!m) return null;
  const p = m.payload;
  switch (m.type) {
    case "route.changed":
      return location(p) ? { type: m.type, payload: { pathname: p.pathname, search: p.search, hash: p.hash } } : null;
    case "theme.changed":
      return object(p) && (p.theme === "light" || p.theme === "dark") ? { type: m.type, payload: { theme: p.theme } } : null;
    case "token.code":
      return object(p) && text(p.code, 64) ? { type: m.type, payload: { code: p.code } } : null;
    case "token.failed":
      return object(p) && text(p.reason, 64) ? { type: m.type, payload: { reason: p.reason } } : null;
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
  }
  return null;
}
