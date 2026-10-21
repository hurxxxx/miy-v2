import type { ReactNode, ButtonHTMLAttributes } from "react";
export function Panel({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section
      style={{
        border: "1px solid #dce3e9",
        borderRadius: 12,
        padding: 24,
        background: "white",
      }}
    >
      <h2>{title}</h2>
      {children}
    </section>
  );
}
export function Button(props: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      {...props}
      style={{
        padding: "9px 16px",
        borderRadius: 8,
        border: "1px solid #ccd5df",
        background: "white",
        cursor: "pointer",
        ...props.style,
      }}
    />
  );
}
