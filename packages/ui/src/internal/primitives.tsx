// Building blocks of the miy composite components (directory, sharing, files).
// Not exported from the package: the public primitives are the AI-DO ones in lib/.
import {
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
  type Ref,
} from "react";

export function classNames(
  ...values: Array<string | false | null | undefined>
): string {
  return values.filter(Boolean).join(" ");
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "dense" | "comfortable" | "icon";
  busy?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

export function Button({
  variant = "secondary",
  size = "dense",
  busy = false,
  disabled,
  type = "button",
  className,
  children,
  ref,
  ...props
}: ButtonProps) {
  return (
    <button
      {...props}
      ref={ref}
      type={type}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      className={classNames(
        "miy-button",
        `miy-button--${variant}`,
        `miy-button--${size}`,
        className,
      )}
    >
      {busy ? <span className="miy-spinner" aria-hidden="true" /> : null}
      {children}
    </button>
  );
}

export function Badge({
  tone = "neutral",
  className,
  ...props
}: HTMLAttributes<HTMLSpanElement> & {
  tone?: "neutral" | "info" | "success" | "warning" | "danger";
}) {
  return (
    <span
      {...props}
      className={classNames("miy-badge", `miy-badge--${tone}`, className)}
    />
  );
}

export function Loading({
  label = "불러오는 중…",
  className,
}: {
  label?: string;
  className?: string;
}) {
  return (
    <span
      className={classNames("miy-loading", className)}
      role="status"
      aria-live="polite"
    >
      <span className="miy-spinner" aria-hidden="true" />
      {label}
    </span>
  );
}

export interface ContentStateProps {
  kind: "loading" | "empty" | "error" | "unavailable";
  title: ReactNode;
  description?: ReactNode;
  action?: { label: string; onClick: () => void; disabled?: boolean };
  className?: string;
}

export function ContentState({
  kind,
  title,
  description,
  action,
  className,
}: ContentStateProps) {
  return (
    <div
      className={classNames(
        "miy-content-state",
        `miy-content-state--${kind}`,
        className,
      )}
      role={
        kind === "error" ? "alert" : kind === "loading" ? "status" : undefined
      }
      aria-busy={kind === "loading" || undefined}
    >
      {kind === "loading" ? (
        <span className="miy-spinner" aria-hidden="true" />
      ) : null}
      <strong>{title}</strong>
      {description ? <p>{description}</p> : null}
      {action ? (
        <Button onClick={action.onClick} disabled={action.disabled}>
          {action.label}
        </Button>
      ) : null}
    </div>
  );
}
