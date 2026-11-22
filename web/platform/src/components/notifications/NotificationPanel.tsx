// Ported from AI-DO apps/web/src/components/layout/NotificationPanel.tsx
// (0002af062): the same panel, list and actions. Differences: a notification
// shows its app's name where AI-DO showed a body line (miy notifications carry
// no message body), and opening one goes to its app's frame at the app's path.
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
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
  notificationPanelReducer,
  notificationTarget,
} from './notification-panel-model';

export function NotificationPanel({
  csrf,
  environmentPath,
  onClose,
  onCountChange,
  onRefreshUnread,
  refreshKey = 0,
}: {
  csrf: string;
  /** `/environments/<env>/apps/<appId>` without the app: the app is added per item. */
  environmentPath: (appId: string) => string;
  onClose: () => void;
  onCountChange?: (unread: number) => void;
  onRefreshUnread: () => Promise<void>;
  refreshKey?: number;
}) {
  const { t, i18n } = useTranslation(['shell', 'portal']);
  const navigate = useNavigate();
  const [{ loading, failed, notifications, nextCursor }, dispatch] = useReducer(
    notificationPanelReducer,
    INITIAL_NOTIFICATION_PANEL_STATE,
  );
  const listRequest = useRef<AbortController>(undefined);
  const openingRequest = useRef<AbortController>(undefined);
  const pageDepth = useRef(1);
  const [loadingMore, setLoadingMore] = useState(false);
  const [moreFailed, setMoreFailed] = useState(false);
  const [openFailed, setOpenFailed] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    listRequest.current = controller;
    setLoadingMore(false);
    setMoreFailed(false);
    dispatch({ type: 'loading' });
    const refresh = async () => {
      const signal = controller.signal;
      try {
        const first = await listNotifications({ signal, check: true });
        const items = new Map(first.items.map((item) => [item.id, item]));
        let nextCursor = first.nextCursor;
        for (let index = 1; nextCursor && index < pageDepth.current; index++) {
          const page = await listNotifications({ signal, check: true, cursor: nextCursor });
          for (const item of page.items) items.set(item.id, item);
          nextCursor = page.nextCursor;
        }
        if (signal.aborted) return;
        dispatch({ type: 'loaded', notifications: [...items.values()], nextCursor });
        onCountChange?.(first.unread);
      } catch {
        if (!signal.aborted) dispatch({ type: 'failed' });
      }
    };
    void refresh();
    return () => controller.abort();
  }, [refreshKey, onCountChange]);

  useEffect(() => () => openingRequest.current?.abort(), []);

  const loadMore = async () => {
    const signal = listRequest.current?.signal;
    if (!signal || !nextCursor || loadingMore || loading) return;
    setLoadingMore(true);
    setMoreFailed(false);
    try {
      const page = await listNotifications({ signal, check: true, cursor: nextCursor });
      if (!signal.aborted) {
        pageDepth.current += 1;
        dispatch({ type: 'loaded', notifications: page.items, nextCursor: page.nextCursor, append: true });
      }
    } catch {
      if (!signal.aborted) setMoreFailed(true);
    } finally {
      if (!signal.aborted) setLoadingMore(false);
    }
  };

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
        void onRefreshUnread();
      } catch {
        /* Keep the item unread when the update fails. */
      }
    },
    [csrf, onRefreshUnread],
  );

  const handleClick = useCallback(
    async (n: PortalNotification) => {
      openingRequest.current?.abort();
      const controller = new AbortController();
      openingRequest.current = controller;
      setOpenFailed(false);
      try {
        const { items } = await listNotifications({ signal: controller.signal, check: true, id: n.id });
        if (controller.signal.aborted) return;
        const current = items.find((item) => item.id === n.id);
        const target = current && notificationTarget(current, environmentPath(current.appId));
        if (!current || !target) {
          if (!current) dispatch({ type: 'remove', notificationId: n.id });
          setOpenFailed(true);
          void onRefreshUnread();
          return;
        }
        void handleRead(current);
        void navigate({ to: target as '/' });
        onClose();
      } catch {
        if (!controller.signal.aborted) setOpenFailed(true);
      }
    },
    [environmentPath, handleRead, navigate, onClose, onRefreshUnread],
  );

  const handleReadAll = useCallback(async () => {
    try {
      await markAllNotificationsRead(csrf);
      dispatch({ type: 'mark-all-read' });
      await onRefreshUnread();
    } catch {
      /* unchanged */
    }
  }, [csrf, onRefreshUnread]);

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
          {openFailed && <p role="status" className="app-text-caption px-4 py-2">{t('portal:notifications.openFailed')}</p>}
          {loading && notifications.length === 0 ? (
            <div className="flex justify-center py-8">
              <Loader2 size={18} className="animate-spin text-app-ink/40" />
            </div>
          ) : failed ? (
            <p role="alert" className="app-text-body text-center text-app-ink/55 py-8">
              {t('portal:notifications.loadFailed')}
            </p>
          ) : notifications.length === 0 && !nextCursor ? (
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
                    onClick={() => void handleClick(n)}
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
          {nextCursor && !failed && (
            <div className="flex flex-col items-center gap-2 py-3">
              {moreFailed && <p role="status" className="app-text-caption">{t('portal:notifications.loadFailed')}</p>}
              <Button size="dense" variant="ghost" disabled={loadingMore || loading} onClick={() => void loadMore()}>
                {t('portal:notifications.loadMore')}
              </Button>
            </div>
          )}
        </div>
      </m.div>
    </LazyMotion>
  );
}
