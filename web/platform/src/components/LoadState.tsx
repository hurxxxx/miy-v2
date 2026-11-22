import { useTranslation } from "react-i18next";
import { Button, Notice } from "../ui";

export function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  const { t } = useTranslation("portal");
  return (
    <Notice
      role="alert"
      tone="danger"
      actions={
        <Button size="sm" onClick={onRetry}>
          {t("common:actions.retry")}
        </Button>
      }
    >
      <p>{message}</p>
    </Notice>
  );
}

export function StaleNotice({ updatedAt, onRetry }: { updatedAt?: number; onRetry: () => void }) {
  const { t, i18n } = useTranslation("portal");
  return (
    <Notice
      role="alert"
      tone="warning"
      actions={
        <Button size="sm" onClick={onRetry}>
          {t("common:actions.retry")}
        </Button>
      }
    >
      <p>
        {updatedAt
          ? t("loadState.staleAt", { time: new Date(updatedAt).toLocaleTimeString(i18n.language) })
          : t("loadState.stale")}
      </p>
    </Notice>
  );
}
