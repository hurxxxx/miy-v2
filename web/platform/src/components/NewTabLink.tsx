import type { AnchorHTMLAttributes } from "react";
import { useTranslation } from "react-i18next";

type NewTabLinkProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "target" | "rel"> & {
  href: string;
};

export function NewTabLink({ children, title, ...props }: NewTabLinkProps) {
  const { t } = useTranslation("portal");
  return (
    <a
      {...props}
      target="_blank"
      rel="noopener noreferrer"
      title={title ?? t("newTabLink.title")}
      aria-description={t("newTabLink.description")}
    >
      {children}
      <span className="new-tab-indicator" aria-hidden="true">{" ↗"}</span>
    </a>
  );
}
