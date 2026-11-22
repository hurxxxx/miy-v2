import { useRef } from 'react';
import type * as React from 'react';
import { createPortal } from 'react-dom';
import { Archive, Eye, EyeOff, Pencil, Pin, PinOff, Users } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  useDismissOnOutside,
  useFloatingAnchorPosition,
} from './space-tree-floating';

type SpaceContextMenuProps = {
  open: boolean;
  anchorRef: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onRename?: () => void;
  onManageMembers: () => void;
  canManage: boolean;
  onDelete?: () => void;
  /** Pin/hide only change the current user's own sidebar. */
  pinned?: boolean;
  hidden?: boolean;
  onTogglePinned?: () => void;
  onToggleHidden?: () => void;
};

const MENU_ITEM_CLASS =
  'w-full flex items-center gap-3 px-3 py-2 hover:bg-app-surface-hover transition-colors';

export function SpaceContextMenu({
  open,
  anchorRef,
  onClose,
  onRename,
  onManageMembers,
  canManage,
  onDelete,
  pinned = false,
  hidden = false,
  onTogglePinned,
  onToggleHidden,
}: SpaceContextMenuProps) {
  const { t } = useTranslation('apps');
  const ref = useRef<HTMLDivElement>(null);
  const pos = useFloatingAnchorPosition(open, anchorRef);
  useDismissOnOutside(open, ref, onClose);

  if (!open) return null;

  return createPortal(
    <div
      ref={ref}
      style={{ top: pos.top, left: pos.left }}
      className="fixed z-[9999] w-48 bg-app-bg border border-app-border rounded-lg shadow-xl py-1"
    >
      {onTogglePinned ? (
        <button
          type="button"
          onClick={() => {
            onTogglePinned();
            onClose();
          }}
          className={MENU_ITEM_CLASS}
        >
          {pinned ? (
            <PinOff size={14} className="text-app-ink/45" />
          ) : (
            <Pin size={14} className="text-app-ink/45" />
          )}
          <span className="app-text-control-sm text-app-ink">
            {pinned ? t('pms.sidebar.unpinSpace') : t('pms.sidebar.pinSpace')}
          </span>
        </button>
      ) : null}
      {onToggleHidden ? (
        <button
          type="button"
          onClick={() => {
            onToggleHidden();
            onClose();
          }}
          className={MENU_ITEM_CLASS}
        >
          {hidden ? (
            <Eye size={14} className="text-app-ink/45" />
          ) : (
            <EyeOff size={14} className="text-app-ink/45" />
          )}
          <span className="app-text-control-sm text-app-ink">
            {hidden ? t('pms.sidebar.showSpace') : t('pms.sidebar.hideSpace')}
          </span>
        </button>
      ) : null}
      {onTogglePinned || onToggleHidden ? (
        <div className="my-1 border-t border-app-border" />
      ) : null}
      {onRename ? (
        <button
          type="button"
          onClick={() => {
            onRename();
            onClose();
          }}
          className={MENU_ITEM_CLASS}
        >
          <Pencil size={14} className="text-app-ink/45" />
          <span className="app-text-control-sm text-app-ink">
            {t('common:actions.rename')}
          </span>
        </button>
      ) : null}
      <button
        type="button"
        onClick={() => {
          onManageMembers();
          onClose();
        }}
        className={MENU_ITEM_CLASS}
      >
        <Users size={14} className="text-app-ink/45" />
        <span className="app-text-control-sm text-app-ink">
          {canManage
            ? t('pms.spaceMembers.titleSuffix')
            : t('pms.spaceMembers.viewMembers')}
        </span>
      </button>
      {onDelete ? (
        <button
          type="button"
          onClick={() => {
            onDelete();
            onClose();
          }}
          className={MENU_ITEM_CLASS}
        >
          <Archive size={14} className="text-app-danger-text" />
          <span className="app-text-control-sm text-app-danger-text">
            {t('common:actions.archive')}
          </span>
        </button>
      ) : null}
    </div>,
    document.body,
  );
}
