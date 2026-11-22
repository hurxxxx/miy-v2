/**
 * AI-DO views/whiteboard-editor-utils.ts without the collaboration helpers
 * (live status labels, user colors, Yjs state encoding, WebSocket URLs).
 */
export type SaveStatus = 'idle' | 'dirty' | 'saving' | 'saved' | 'error';

export function waitFor(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

export function safeFilename(title: string, extension: string): string {
  const base =
    title
      .trim()
      .replace(/[^a-zA-Z0-9가-힣._-]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'whiteboard';
  return `${base}.${extension}`;
}

export function saveLabel(
  status: SaveStatus,
  t: (key: string) => string,
): string {
  return {
    idle: '',
    dirty: t('common:actions.saving'),
    saving: t('common:actions.saving'),
    saved: t('whiteboard.saved'),
    error: t('whiteboard.saveFailed'),
  }[status];
}
