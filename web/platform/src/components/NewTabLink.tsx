import type { AnchorHTMLAttributes } from "react";

type NewTabLinkProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "target" | "rel"> & {
  href: string;
};

export function NewTabLink({ children, title = "새 탭에서 열기", ...props }: NewTabLinkProps) {
  return (
    <a {...props} target="_blank" rel="noopener noreferrer" title={title} aria-description="새 탭에서 열립니다">
      {children}
      <span className="new-tab-indicator" aria-hidden="true">{"\u00a0↗"}</span>
    </a>
  );
}
