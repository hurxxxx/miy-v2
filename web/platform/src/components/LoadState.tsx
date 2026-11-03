import { Button } from "@miy/ui";

export function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div role="alert" className="load-error">
      <p>{message}</p>
      <Button type="button" onClick={onRetry}>
        다시 시도
      </Button>
    </div>
  );
}

export function StaleNotice({ updatedAt, onRetry }: { updatedAt?: number; onRetry: () => void }) {
  return (
    <div role="alert" className="load-error">
      <p>
        최신 앱 상태를 확인하지 못했습니다.
        {updatedAt ? ` 마지막 확인: ${new Date(updatedAt).toLocaleTimeString()}` : ""}
      </p>
      <Button type="button" onClick={onRetry}>
        다시 시도
      </Button>
    </div>
  );
}
