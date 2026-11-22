import { useCallback, useMemo } from 'react';

import { useRuntime } from './runtime-context';

/**
 * Replaces react-router's `useSearchParams` / `useNavigate` in the ported
 * screens. The app runs in a frame without the history API: the portal owns
 * the address and hands the app `runtime.location`; changes go through
 * `runtime.navigate` (app-relative paths under the app's base path).
 */
export function useAppSearchParams(): [
  URLSearchParams,
  (next: URLSearchParams, options?: { replace?: boolean }) => void,
] {
  const { location, navigate } = useRuntime();
  const { pathname, search } = location;
  const params = useMemo(() => new URLSearchParams(search), [search]);
  const setParams = useCallback(
    (next: URLSearchParams, options?: { replace?: boolean }) => {
      const query = next.toString();
      navigate(`${pathname}${query ? `?${query}` : ''}`, {
        replace: options?.replace,
      });
    },
    [navigate, pathname],
  );
  return [params, setParams];
}

/** `navigate(path, { replace })` like react-router's, on `runtime.navigate`. */
export function useAppNavigate(): (
  path: string,
  options?: { replace?: boolean },
) => void {
  const { navigate } = useRuntime();
  return useCallback(
    (path: string, options?: { replace?: boolean }) =>
      navigate(path, { replace: options?.replace }),
    [navigate],
  );
}

export type AppRoute =
  | { kind: 'hub' }
  | { kind: 'board'; whiteboardId: string };

function decodeSegment(value: string): string | null {
  try {
    const decoded = decodeURIComponent(value);
    return decoded.trim() ? decoded : null;
  } catch {
    return null;
  }
}

/**
 * The app's screens. AI-DO's whiteboard routes were
 * `/w/:workspaceSlug/whiteboard` (the hub) and
 * `/w/:workspaceSlug/whiteboard/:whiteboardId` (a board). The app frame is
 * the whiteboard, so they are `/` and `/:whiteboardId`; the space is the
 * `?space=` query (absent for the app-global space). Deeper paths show the hub.
 */
export function resolveAppRoute(pathname: string): AppRoute {
  const path = pathname.replace(/\/+$/, '') || '/';
  const board = /^\/([^/]+)$/.exec(path);
  if (board) {
    const whiteboardId = decodeSegment(board[1]);
    if (whiteboardId) return { kind: 'board', whiteboardId };
  }
  return { kind: 'hub' };
}

/** The app-global space (AI-DO app_global), the server's reserved ID. */
export const GLOBAL_SPACE_ID = 'global';

/** The current space from `?space=` (the global space when absent). */
export function readSpaceId(searchParams: URLSearchParams): string {
  return searchParams.get('space') || GLOBAL_SPACE_ID;
}
