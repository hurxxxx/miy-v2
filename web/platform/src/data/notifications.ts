import { useCallback, useEffect, useRef, useState } from "react";
import { request } from "@miy/sdk";

/** One entry of GET /api/v1/notifications (this portal's environment only). */
export type PortalNotification = {
  id: string;
  installationId: string;
  appId: string;
  appName: string;
  kind: string;
  groupKey: string;
  /** Display text from the app; render as text only. */
  title: string;
  /** App-relative destination inside the app's frame. */
  path: string;
  resourceId: string | null;
  createdAt: string;
  readAt: string | null;
};
export type NotificationPage = { items: PortalNotification[]; unread: number; nextCursor: string | null };
export type NotificationMute = { appId: string; kind: string };

/** With `check`, the Core first asks the apps whether each item may still be opened. */
export function listNotifications({
  signal,
  check = false,
  cursor,
  id,
}: { signal?: AbortSignal; check?: boolean; cursor?: string; id?: string } = {}) {
  const query = new URLSearchParams({ limit: "50" });
  if (check) query.set("check", "true");
  if (cursor) query.set("cursor", cursor);
  if (id) query.set("id", id);
  return request<NotificationPage>(`/api/v1/notifications?${query}`, undefined, { signal });
}
export function markNotificationRead(csrf: string, id: string) {
  return request<void>(`/api/v1/notifications/${encodeURIComponent(id)}/read`, csrf, { method: "POST" });
}
export function markAllNotificationsRead(csrf: string) {
  return request<void>("/api/v1/notifications/read-all", csrf, { method: "POST" });
}
export function readNotificationSettings() {
  return request<{ mutes: NotificationMute[] }>("/api/v1/notifications/settings");
}
export function saveNotificationSettings(csrf: string, mutes: NotificationMute[]) {
  return request<{ mutes: NotificationMute[] }>("/api/v1/notifications/settings", csrf, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ mutes }),
  });
}

const POLL_MS = 60_000;
const SESSION_ENDED = 4401;

/**
 * The unread badge. The portal's live channel (`/api/v1/live`) pushes the count
 * when the inbox changes; while it is not connected the count is polled every
 * minute and when the page becomes visible again. A failed read keeps the last
 * count rather than showing zero.
 */
export function useUnreadNotifications(enabled: boolean) {
  const [unread, setUnread] = useState(0);
  const [revision, setRevision] = useState(0);
  const latest = useRef(0);
  const refresh = useCallback(async () => {
    if (!enabled) return;
    const sequence = ++latest.current;
    try {
      const value = await request<{ unread: number }>("/api/v1/notifications/unread-count");
      if (sequence === latest.current && typeof value.unread === "number") setUnread(value.unread);
    } catch {
      /* keep the last count */
    }
  }, [enabled]);
  useEffect(() => {
    if (!enabled) return;
    let stopped = false;
    let socket: WebSocket | null = null;
    let live = false;
    let attempt = 0;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const connect = () => {
      if (stopped || typeof WebSocket === "undefined") return;
      const url = new URL("/api/v1/live", window.location.href);
      url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const current = new WebSocket(url.href);
      socket = current;
      current.onmessage = (event) => {
        if (typeof event.data !== "string") return;
        try {
          const value = JSON.parse(event.data) as { type?: string; unread?: unknown };
          if (value.type === "notifications.unread" && typeof value.unread === "number") {
            live = true;
            attempt = 0;
            latest.current += 1;
            setUnread(value.unread);
            setRevision((value) => value + 1);
          }
        } catch {
          /* not a channel message */
        }
      };
      current.onclose = (event) => {
        live = false;
        if (stopped || event.code === SESSION_ENDED) return;
        attempt += 1;
        retry = setTimeout(connect, Math.min(60_000, 2_000 * 2 ** Math.min(attempt, 5)));
      };
    };
    void refresh();
    connect();
    const timer = setInterval(() => {
      if (!live) void refresh();
    }, POLL_MS);
    const visible = () => {
      if (document.visibilityState === "visible" && !live) void refresh();
    };
    document.addEventListener("visibilitychange", visible);
    return () => {
      stopped = true;
      latest.current += 1;
      clearTimeout(retry);
      clearInterval(timer);
      socket?.close(1000);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [enabled, refresh]);
  return { unread, setUnread, refresh, revision };
}
