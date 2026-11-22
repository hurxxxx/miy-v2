import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { FeedbackProvider } from "@miy/ui";

/** Toast feedback with translated labels, as AI-DO's AppRoot provides it. */
export function Feedback({ children }: { children: ReactNode }) {
  const { t } = useTranslation("common");
  return (
    <FeedbackProvider
      labels={{
        close: t("actions.close"),
        item: t("feedback.itemLabel"),
        region: t("feedback.regionLabel"),
      }}
    >
      {children}
    </FeedbackProvider>
  );
}
