// The dialog of the miy composite components. Not exported from the package:
// the public Dialog and useConfirm are the AI-DO ones in lib/.
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Button, classNames } from "./primitives";

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  closeLabel?: string;
  busy?: boolean;
  size?: "small" | "medium" | "large";
  className?: string;
}

/** Uses this document's modal top layer, including inside an app frame. */
export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  actions,
  closeLabel = "닫기",
  busy = false,
  size = "medium",
  className,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  const onChangeRef = useRef(onOpenChange);
  onChangeRef.current = onOpenChange;
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || !open) return;
    const previous = dialog.ownerDocument.activeElement;
    dialog.showModal();
    // Escape closes this dialog only. An enclosing overlay that listens on the
    // document during capture (a Radix dialog or drawer) leaves a prevented
    // event alone, so prevent it before the document sees it; onKeyDown below
    // then closes this dialog.
    const view = dialog.ownerDocument.defaultView;
    const escape = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !event.isComposing &&
        dialog.contains(event.target as Node | null)
      )
        event.preventDefault();
    };
    view?.addEventListener("keydown", escape, true);
    return () => {
      view?.removeEventListener("keydown", escape, true);
      dialog.close();
      if (previous instanceof HTMLElement && previous.isConnected)
        previous.focus();
    };
  }, [open]);
  return (
    <dialog
      ref={ref}
      className={classNames("miy-dialog", `miy-dialog--${size}`, className)}
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      aria-busy={busy || undefined}
      onKeyDown={(event) => {
        // A control inside that consumed Escape stopped the event before here.
        if (event.key !== "Escape" || event.nativeEvent.isComposing) return;
        event.stopPropagation();
        if (!busy) onChangeRef.current(false);
      }}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onChangeRef.current(false);
      }}
      onClose={() => {
        if (open && ref.current?.open === false) onChangeRef.current(false);
      }}
    >
      <header className="miy-dialog__header">
        <h2 id={titleId}>{title}</h2>
        <Button
          size="icon"
          variant="ghost"
          disabled={busy}
          aria-label={closeLabel}
          onClick={() => onOpenChange(false)}
        >
          ×
        </Button>
      </header>
      {description ? (
        <div id={descriptionId} className="miy-dialog__description">
          {description}
        </div>
      ) : null}
      {children ? <div className="miy-dialog__body">{children}</div> : null}
      {actions ? (
        <footer className="miy-dialog__actions">{actions}</footer>
      ) : null}
    </dialog>
  );
}

export interface ConfirmOptions {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

/** Superseding or unmounting a confirmation always resolves the old request as false. */
export function useConfirm() {
  const [options, setOptions] = useState<ConfirmOptions | null>(null);
  const pending = useRef<((value: boolean) => void) | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pending.current?.(false);
      pending.current = null;
    };
  }, []);
  const settle = useCallback((value: boolean) => {
    const resolve = pending.current;
    pending.current = null;
    setOptions(null);
    resolve?.(value);
  }, []);
  const confirm = useCallback((next: ConfirmOptions) => {
    if (!mounted.current) return Promise.resolve(false);
    pending.current?.(false);
    return new Promise<boolean>((resolve) => {
      pending.current = resolve;
      setOptions(next);
    });
  }, []);
  return {
    confirm,
    confirmDialog: options ? (
      <Dialog
        open
        title={options.title}
        description={options.description}
        onOpenChange={(open) => {
          if (!open) settle(false);
        }}
        actions={
          <>
            <Button onClick={() => settle(false)}>
              {options.cancelLabel ?? "취소"}
            </Button>
            <Button
              variant={options.danger ? "danger" : "primary"}
              onClick={() => settle(true)}
            >
              {options.confirmLabel ?? "확인"}
            </Button>
          </>
        }
      />
    ) : null,
  };
}
