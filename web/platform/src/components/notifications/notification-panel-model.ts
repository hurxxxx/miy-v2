// Ported from AI-DO apps/web/src/components/layout/notification-panel-model.ts
// (0002af062). In miy every notification opens its app's frame at the app's
// path, so AI-DO's DM/issue/route branches become one app destination.
import type { PortalNotification } from '../../data/notifications';

export type NotificationPanelState = {
  loading: boolean;
  failed: boolean;
  notifications: PortalNotification[];
};

export type NotificationPanelAction =
  | { type: 'loading' }
  | { type: 'loaded'; notifications: PortalNotification[] }
  | { type: 'failed' }
  | { type: 'mark-read'; notificationId: string }
  | { type: 'mark-all-read' };

export const INITIAL_NOTIFICATION_PANEL_STATE: NotificationPanelState = {
  loading: true,
  failed: false,
  notifications: [],
};

export function notificationPanelReducer(
  state: NotificationPanelState,
  action: NotificationPanelAction,
): NotificationPanelState {
  const now = new Date().toISOString();
  switch (action.type) {
    case 'loading':
      return { ...state, loading: true, failed: false };
    case 'loaded':
      return { loading: false, failed: false, notifications: action.notifications };
    case 'failed':
      return { ...state, loading: false, failed: true };
    case 'mark-read':
      return {
        ...state,
        notifications: state.notifications.map((item) =>
          item.id === action.notificationId ? { ...item, readAt: item.readAt ?? now } : item,
        ),
      };
    case 'mark-all-read':
      return {
        ...state,
        notifications: state.notifications.map((item) => ({ ...item, readAt: item.readAt ?? now })),
      };
  }
}

export function countUnreadNotifications(notifications: readonly PortalNotification[]): number {
  return notifications.filter((notification) => !notification.readAt).length;
}

/** The portal route that opens the notification's app frame at its path. */
export function notificationTarget(notification: PortalNotification, environmentPath: string): string | null {
  if (!notification.path.startsWith('/') || notification.path.startsWith('//')) return null;
  return `${environmentPath}${notification.path === '/' ? '' : notification.path}`;
}
