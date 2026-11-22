// Ported from AI-DO apps/web/src/components/layout/NotificationPanel.tsx
// (0002af062): the same panel, list and actions. Differences: a notification
// shows its app's name where AI-DO showed a body line (miy notifications carry
// no message body), and opening one goes to its app's frame at the app's path.
import { useCallback, useEffect, useReducer } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from '@tanstack/react-router';
import { LazyMotion, domAnimation, m } from 'motion/react';
import { X, Check, CheckCheck, Loader2 } from 'lucide-react';
import { Button } from '@miy/ui';

import { formatRelativeTime } from '../../platform/time/time-utils';
import {
  listNotifications,
  markAllNotificationsRead,
  markNotificationRead,
  type PortalNotification,
} from '../../data/notifications';
import {
  INITIAL_NOTIFICATION_PANEL_STATE,
  countUnreadNotifications,
  notificationPanelReducer,
  notificationTarget,
} from './notification-panel-model';

export function NotificationPanel({
  csrf,
  environmentPath,
  onClose,
  onCountChange,
  refreshKey = 0,
}: {
  csrf: string;
  /** `/environments/<env>/apps/<appId>` without the app: the app is added per item. */
  environmentPath: (appId: string) => string;
  onClose: () => void;
  onCountChange?: (unread: number) => void;
  refreshKey?: number;
}) {
  const { t, i18n } = useTranslation(['shell', 'portal']);
  const navigate = useNavigate();
  const [{ loading, failed, notifications }, dispatch] = useReducer(
    notificationPanelReducer,
    INITIAL_NOTIFICATION_PANEL_STATE,
  );

  useEffect(() => {
    const controller = new AbortController();
    dispatch({ type: 'loading' });
    listNotifications(controller.signal, true)
      .then((page) => {
        dispatch({ type: 'loaded', notifications: page.items });
        onCountChange?.(page.unread);
      })
      .catch(() => {
        if (!controller.signal.aborted) dispatch({ type: 'failed' });
      });
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshKey]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  const handleRead = useCallback(
    async (n: PortalNotification) => {
      if (n.readAt) return;
      try {
        await markNotificationRead(csrf, n.id);
        dispatch({ type: 'mark-read', notificationId: n.id });
        onCountChange?.(
          countUnreadNotifications(notifications.filter((item) => item.id !== n.id)),
        );
      } catch {
        /* the list still shows it unread */
      }
    },
    [csrf, notifications, onCountChange],
  );

  const handleClick = useCallback(
    (n: PortalNotification) => {
      void handleRead(n);
      const target = notificationTarget(n, environmentPath(n.appId));
      if (target) {
        void navigate({ to: target as '/' });
        onClose();
      }
    },
    [environmentPath, handleRead, navigate, onClose],
  );

  const handleReadAll = useCallback(async () => {
    try {
      await markAllNotificationsRead(csrf);
      dispatch({ type: 'mark-all-read' });
      onCountChange?.(0);
    } catch {
      /* unchanged */
    }
  }, [csrf, onCountChange]);

  return (
    <LazyMotion features={domAnimation}>
      <button
        type="button"
        aria-label={t('portal:notifications.closePanel')}
        className="fixed inset-0 z-40 cursor-default"
        onClick={onClose}
        tabIndex={-1}
      />
      <m.div
        role="dialog"
        aria-label={t('shell:notifications.title')}
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -8 }}
        className="fixed left-3 right-3 top-16 z-50 flex max-h-[min(480px,calc(100vh-5rem))] flex-col overflow-hidden rounded-xl border border-app-border bg-app-bg shadow-2xl lg:left-auto lg:right-4 lg:w-80"
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border shrink-0">
          <h3 className="app-text-title-md text-app-ink">
            {t('shell:notifications.title')}
          </h3>
          <div className="flex items-center gap-1">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => void handleReadAll()}
              title={t('shell:notifications.markAllAsRead')}
              aria-label={t('shell:notifications.markAllAsRead')}
            >
              <CheckCheck size={14} />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              onClick={onClose}
              aria-label={t('portal:notifications.closePanel')}
            >
              <X size={14} />
            </Button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto custom-scrollbar">
          {loading ? (
            <div className="flex justify-center py-8">
              <Loader2 size={18} className="animate-spin text-app-ink/40" />
            </div>
          ) : failed ? (
            <p role="alert" className="app-text-body text-center text-app-ink/55 py-8">
              {t('portal:notifications.loadFailed')}
            </p>
          ) : notifications.length === 0 ? (
            <p className="app-text-body text-center text-app-ink/30 py-8">
              {t('shell:notifications.empty')}
            </p>
          ) : (
            notifications.map((n) => (
              <div
                key={n.id}
                className="w-full border-b border-app-border px-4 py-3 text-left transition-colors hover:bg-app-surface-hover"
              >
                <div className="flex items-start gap-2">
                  <button
                    onClick={() => handleClick(n)}
                    className="flex min-w-0 flex-1 items-start gap-2 text-left"
                    type="button"
                  >
                    {!n.readAt && (
                      <span className="mt-1.5 size-2 shrink-0 rounded-full bg-app-accent" />
                    )}
                    <div className="flex-1 min-w-0">
                      <p
                        className={`app-text-body truncate font-medium ${
                          n.readAt ? 'text-app-ink/85' : 'text-app-ink'
                        }`}
                      >
                        {n.title}
                      </p>
                      <p className="app-text-caption mt-0.5 truncate text-app-ink/75">
                        {n.appName}
                      </p>
                      <span className="app-text-micro text-app-ink/70">
                        {formatRelativeTime(n.createdAt, { locale: i18n.language })}
                      </span>
                    </div>
                  </button>
                  {!n.readAt && (
                    <button
                      onClick={() => void handleRead(n)}
                      className="mt-1 shrink-0 text-app-ink/70 hover:text-app-accent"
                      title={t('shell:notifications.markAsRead')}
                      aria-label={t('shell:notifications.markAsRead')}
                      type="button"
                    >
                      <Check size={12} />
                    </button>
                  )}
                </div>
              </div>
            ))
          )}
        </div>
      </m.div>
    </LazyMotion>
  );
}
