import { Button, Notice } from "../ui";

export function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <Notice
      role="alert"
      tone="danger"
      actions={
        <Button size="sm" onClick={onRetry}>
          다시 시도
        </Button>
      }
    >
      <p>{message}</p>
    </Notice>
  );
}

export function StaleNotice({ updatedAt, onRetry }: { updatedAt?: number; onRetry: () => void }) {
  return (
    <Notice
      role="alert"
      tone="warning"
      actions={
        <Button size="sm" onClick={onRetry}>
          다시 시도
        </Button>
      }
    >
      <p>
        최신 앱 상태를 확인하지 못했습니다.
        {updatedAt ? ` 마지막 확인: ${new Date(updatedAt).toLocaleTimeString()}` : ""}
      </p>
    </Notice>
  );
}
