import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Bell } from "lucide-react";
import { useUnreadNotifications } from "../../data/notifications";
import { NotificationPanel } from "./NotificationPanel";

/**
 * The header bell with the unread badge and AI-DO's notification panel. Only
 * the business portals have an inbox; the console renders nothing.
 */
export function NotificationBell({
  csrf,
  enabled,
  environmentPath,
}: {
  csrf: string;
  enabled: boolean;
  environmentPath: (appId: string) => string;
}) {
  const { t } = useTranslation("portal");
  const [open, setOpen] = useState(false);
  const { unread, setUnread, refresh, revision } = useUnreadNotifications(enabled);
  const close = useCallback(() => {
    setOpen(false);
    void refresh();
  }, [refresh]);
  if (!enabled) return null;
  return (
    <>
      <button
        type="button"
        className="notification-bell"
        aria-label={unread > 0 ? t("notifications.bellUnread", { count: unread }) : t("notifications.bell")}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <Bell size={18} aria-hidden="true" />
        {unread > 0 && <span className="notification-badge">{unread > 99 ? "99+" : unread}</span>}
      </button>
      {open && <NotificationPanel csrf={csrf} environmentPath={environmentPath} onClose={close} onCountChange={setUnread} onRefreshUnread={refresh} refreshKey={revision} />}
    </>
  );
}
