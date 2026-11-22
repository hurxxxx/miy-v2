/**
 * The messenger's screens inside the app frame. AI-DO opened DM only as a
 * floating panel of its shell (personal-widget-registry → FloatingDmWidget);
 * the miy app owns the whole frame, and notifications open a conversation
 * by path.
 */
export type AppRoute =
  | { kind: 'messenger'; threadId: string }
  | { kind: 'panel'; threadId: string }
  | { kind: 'safety'; reportId: string | null };

export const PANEL_PATH = '/widgets/conversations';

function segment(value: string | undefined): string {
  if (!value) return '';
  try {
    return decodeURIComponent(value);
  } catch {
    return '';
  }
}

export function resolveAppRoute(pathname: string): AppRoute {
  const parts = (pathname.replace(/\/+$/, '') || '/').split('/').filter(Boolean);
  if (parts[0] === 'conversations' && parts.length === 2) {
    return { kind: 'messenger', threadId: segment(parts[1]) };
  }
  if (parts[0] === 'widgets' && parts[1] === 'conversations' && parts.length <= 3) {
    return { kind: 'panel', threadId: segment(parts[2]) };
  }
  if (parts[0] === 'safety' && parts.length === 1) {
    return { kind: 'safety', reportId: null };
  }
  if (parts[0] === 'safety' && parts[1] === 'reports' && parts.length === 3) {
    return { kind: 'safety', reportId: segment(parts[2]) || null };
  }
  return { kind: 'messenger', threadId: '' };
}

/** The app path of a conversation in the full screen or in the side panel. */
export function conversationPath(kind: 'messenger' | 'panel', threadId: string): string {
  const base = kind === 'panel' ? PANEL_PATH : '';
  if (!threadId) return kind === 'panel' ? PANEL_PATH : '/';
  return `${base}${kind === 'panel' ? '/' : '/conversations/'}${encodeURIComponent(threadId)}`;
}
