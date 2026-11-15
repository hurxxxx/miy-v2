import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { Button, Field, classNames } from "./primitives";

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
  initialFocusRef?: RefObject<HTMLElement | null>;
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
  initialFocusRef,
  className,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  const onChangeRef = useRef(onOpenChange);
  onChangeRef.current = onOpenChange;
  const focusRef = useRef(initialFocusRef);
  focusRef.current = initialFocusRef;
  useEffect(() => {
    const dialog = ref.current;
    if (!dialog || !open) return;
    const previous = dialog.ownerDocument.activeElement;
    dialog.showModal();
    focusRef.current?.current?.focus();
    return () => {
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

export interface PromptOptions {
  title: string;
  label: string;
  description?: string;
  defaultValue?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  validate?: (value: string) => string | null;
  maxLength?: number;
}

export function usePrompt() {
  const [options, setOptions] = useState<PromptOptions | null>(null);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const pending = useRef<((value: string | null) => void) | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pending.current?.(null);
      pending.current = null;
    };
  }, []);
  const settle = useCallback((result: string | null) => {
    const resolve = pending.current;
    pending.current = null;
    setOptions(null);
    resolve?.(result);
  }, []);
  const prompt = useCallback((next: PromptOptions) => {
    if (!mounted.current) return Promise.resolve(null);
    pending.current?.(null);
    setValue(next.defaultValue ?? "");
    setError(null);
    return new Promise<string | null>((resolve) => {
      pending.current = resolve;
      setOptions(next);
    });
  }, []);
  const submit = () => {
    const normalized = value.trim();
    const issue = normalized
      ? (options?.validate?.(normalized) ?? null)
      : "값을 입력해 주세요.";
    if (issue) {
      setError(issue);
      input.current?.focus();
      return;
    }
    settle(normalized);
  };
  return {
    prompt,
    promptDialog: options ? (
      <Dialog
        open
        title={options.title}
        description={options.description}
        initialFocusRef={input}
        onOpenChange={(open) => {
          if (!open) settle(null);
        }}
        actions={
          <>
            <Button onClick={() => settle(null)}>
              {options.cancelLabel ?? "취소"}
            </Button>
            <Button variant="primary" onClick={submit}>
              {options.confirmLabel ?? "확인"}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <Field
            ref={input}
            label={options.label}
            value={value}
            maxLength={options.maxLength ?? 200}
            error={error}
            onChange={(event) => {
              setValue(event.target.value);
              setError(null);
            }}
          />
        </form>
      </Dialog>
    ) : null,
  };
}
