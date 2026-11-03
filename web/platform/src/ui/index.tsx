/*
 * Platform-internal controls. Remote apps keep their own published `ui`
 * component; these are never shared with apps.
 */
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
} from "react";

type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" | "md" }) {
  return <button {...props} type={type} className={`ui-button ui-button-${variant} ui-button-${size} ${className}`.trim()} />;
}

export function Panel({
  title,
  actions,
  children,
  className = "",
}: {
  title: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <section className={`ui-panel ${className}`.trim()}>
      <div className="ui-panel-heading">
        <h2>{title}</h2>
        {actions}
      </div>
      {children}
    </section>
  );
}

export type Tone = "neutral" | "info" | "success" | "warning" | "danger";

export function Badge({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`ui-badge ui-tone-${tone}`}>{children}</span>;
}

export function Notice({
  tone = "info",
  children,
  actions,
  ...props
}: HTMLAttributes<HTMLDivElement> & { tone?: Tone; actions?: ReactNode }) {
  return (
    <div {...props} className={`ui-notice ui-tone-${tone} ${props.className ?? ""}`.trim()}>
      <div className="ui-notice-body">{children}</div>
      {actions && <div className="ui-notice-actions">{actions}</div>}
    </div>
  );
}

export function Loading({ children = "불러오는 중…" }: { children?: ReactNode }) {
  return (
    <p className="ui-loading">
      <span className="ui-spinner" aria-hidden="true" />
      {children}
    </p>
  );
}

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="ui-empty">
      <p className="ui-empty-title">{title}</p>
      {children && <p>{children}</p>}
    </div>
  );
}

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="ui-field">
      <span className="ui-field-label">{label}</span>
      {children}
      {hint && <span className="ui-field-hint">{hint}</span>}
    </label>
  );
}

/** First visible character(s) for an app without an icon; unique within `names`. */
export function monograms(names: { key: string; name: string }[]) {
  const chars = (name: string) => Array.from(name.replace(/\s+/g, ""));
  const first = new Map<string, number>();
  for (const { name } of names) {
    const one = (chars(name)[0] ?? "?").toUpperCase();
    first.set(one, (first.get(one) ?? 0) + 1);
  }
  const used = new Set<string>();
  const result = new Map<string, string>();
  for (const { key, name } of names) {
    const list = chars(name);
    let mark = (list[0] ?? "?").toUpperCase();
    if ((first.get(mark) ?? 0) > 1) mark = list.slice(0, 2).join("").toUpperCase() || mark;
    let candidate = mark;
    for (let n = 2; used.has(candidate); n++) candidate = mark.slice(0, 1) + n;
    used.add(candidate);
    result.set(key, candidate);
  }
  return result;
}

const MONOGRAM_COLORS = ["#2563eb", "#0f766e", "#7c3aed", "#d97706", "#dc2626", "#475569", "#0891b2", "#be185d"];
export function monogramColor(key: string) {
  let hash = 0x811c9dc5; // FNV-1a spreads short app IDs across the palette
  for (const c of key) hash = Math.imul(hash ^ c.charCodeAt(0), 0x01000193) >>> 0;
  return MONOGRAM_COLORS[hash % MONOGRAM_COLORS.length]!;
}

export function AppMark({ mark, colorKey, size = "md" }: { mark: string; colorKey: string; size?: "sm" | "md" | "lg" }) {
  return (
    <span className={`app-mark app-mark-${size}`} style={{ background: monogramColor(colorKey) }} aria-hidden="true">
      {mark}
    </span>
  );
}

type ConfirmOptions = {
  title: string;
  /** Element to focus after closing; defaults to the focused element at open. */
  returnFocus?: HTMLElement | null;
  body: ReactNode;
  confirmLabel: string;
  tone?: "danger" | "primary";
};

/**
 * Modal confirmation on the native <dialog>: focus is kept inside while open,
 * Escape cancels, and focus returns to the element that opened it.
 */
export function useConfirm() {
  type PendingConfirmation = ConfirmOptions & {
    resolve: (ok: boolean) => void;
  };
  const [request, setRequest] = useState<PendingConfirmation | null>(null);
  const pending = useRef<PendingConfirmation | null>(null);
  const mounted = useRef(true);
  const settle = useCallback((ok: boolean) => {
    const current = pending.current;
    pending.current = null;
    if (mounted.current) setRequest(null);
    current?.resolve(ok);
  }, []);
  const cancel = useCallback(() => settle(false), [settle]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      cancel();
    };
  }, [cancel]);
  const confirm = useCallback(
    (options: ConfirmOptions) => {
      cancel();
      if (!mounted.current) return Promise.resolve(false);
      return new Promise<boolean>((resolve) => {
        const next = { ...options, resolve };
        pending.current = next;
        setRequest(next);
      });
    },
    [cancel],
  );
  const dialog = request ? (
    <ConfirmDialog {...request} onClose={settle} />
  ) : null;
  return { confirm, dialog, cancel };
}

function ConfirmDialog({ title, body, confirmLabel, tone = "primary", returnFocus, onClose }: ConfirmOptions & { onClose: (ok: boolean) => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const bodyId = useId();
  useEffect(() => {
    const opener = returnFocus ?? (document.activeElement as HTMLElement | null);
    const node = ref.current!;
    if (!node.open) node.showModal();
    node.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();
    return () => {
      if (node.open) node.close();
      // The opener may still be disabled until the caller settles; restore after that render.
      requestAnimationFrame(() => opener?.focus?.());
    };
  }, []);
  return (
    <dialog
      ref={ref}
      className="shell-dialog"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(event) => {
        event.preventDefault();
        onClose(false);
      }}
    >
      <h2 id={titleId}>{title}</h2>
      <div id={bodyId} className="shell-dialog-body">
        {body}
      </div>
      <div className="shell-dialog-actions">
        <Button data-autofocus onClick={() => onClose(false)}>
          취소
        </Button>
        <Button variant={tone === "danger" ? "danger" : "primary"} onClick={() => onClose(true)}>
          {confirmLabel}
        </Button>
      </div>
    </dialog>
  );
}
