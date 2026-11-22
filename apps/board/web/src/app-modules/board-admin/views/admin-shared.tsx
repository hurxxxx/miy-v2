import { StatusSlot } from '@miy/ui';

// From AI-DO platform/admin/admin-shared-model.ts and admin-shared-components.tsx.
export const FORM_FIELD_CLASS =
  'app-text-body w-full rounded-lg border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none transition-colors focus:border-app-accent';

export function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return fallback;
}

export function SectionMessage({ error }: { error: string | null }) {
  return (
    <StatusSlot
      message={error}
      tone="error"
      announce="assertive"
      size="regular"
    />
  );
}
