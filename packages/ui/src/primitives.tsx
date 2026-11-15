import {
  useId,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
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

export function IconButton({
  label,
  ...props
}: Omit<ButtonProps, "size"> & { label: string }) {
  return <Button {...props} size="icon" aria-label={label} />;
}

export interface PanelProps extends HTMLAttributes<HTMLElement> {
  title?: string;
  actions?: ReactNode;
}

export function Panel({
  title,
  actions,
  children,
  className,
  ...props
}: PanelProps) {
  const titleId = useId();
  return (
    <section
      {...props}
      aria-labelledby={title ? titleId : props["aria-labelledby"]}
      className={classNames("miy-panel", className)}
    >
      {title || actions ? (
        <header className="miy-panel__header">
          {title ? <h2 id={titleId}>{title}</h2> : null}
          {actions}
        </header>
      ) : null}
      {children}
    </section>
  );
}

export interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: ReactNode;
  description?: ReactNode;
  error?: ReactNode;
  ref?: Ref<HTMLInputElement>;
}

export function Field({
  label,
  description,
  error,
  id,
  className,
  ref,
  ...props
}: FieldProps) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const describedBy =
    [
      props["aria-describedby"],
      description ? `${inputId}-description` : null,
      error ? `${inputId}-error` : null,
    ]
      .filter(Boolean)
      .join(" ") || undefined;
  return (
    <div className={classNames("miy-field", className)}>
      <label htmlFor={inputId}>
        {label}
        {props.required ? <span aria-hidden="true"> *</span> : null}
      </label>
      <input
        {...props}
        ref={ref}
        id={inputId}
        className="miy-input"
        aria-describedby={describedBy}
        aria-invalid={error ? true : props["aria-invalid"]}
      />
      {description ? (
        <span id={`${inputId}-description`} className="miy-field__description">
          {description}
        </span>
      ) : null}
      {error ? (
        <span id={`${inputId}-error`} role="alert" className="miy-field__error">
          {error}
        </span>
      ) : null}
    </div>
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
