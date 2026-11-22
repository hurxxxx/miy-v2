import { Dialog } from '@miy/ui';
import type { ReactNode } from 'react';

type TaskDetailModalProps = {
  children: ReactNode;
  title: string;
  onClose: () => void;
};

export function TaskDetailModal({
  children,
  title,
  onClose,
}: TaskDetailModalProps) {
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={title}
      embedded
      fullSize
      overlayClassName="z-[calc(var(--ui-z-drawer)-3)]"
      contentClassName="z-[calc(var(--ui-z-drawer)-2)] h-dvh w-full max-w-none rounded-none border-0 bg-app-bg lg:h-[calc(100dvh-3rem)] lg:w-4/5 lg:rounded-xl lg:border"
    >
      {children}
    </Dialog>
  );
}
