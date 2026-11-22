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
  | { kind: 'list' }
  | { kind: 'doc'; docId: string }
  | { kind: 'settings' };

function decodeSegment(value: string): string | null {
  try {
    const decoded = decodeURIComponent(value);
    return decoded.trim() ? decoded : null;
  } catch {
    return null;
  }
}

export const SETTINGS_PATH = '/settings';

/**
 * The app's screens. AI-DO's docs routes were `/w/:slug/docs` (the list) and
 * `/w/:slug/docs/:docId` (a doc, `?page=` selects the page); here they are
 * `/` and `/:docId` under the app base path, with the space in `?space=`.
 * `/docs` and `/docs/:docId` (AI-DO's route base) open the same screens.
 * `/settings` holds docs managers and spaces (miy). HTML pages
 * (`/:docId/html/:pageId`) and share links (`/docs/shared/:token`) are not
 * ported. Unknown paths show the list.
 */
export function resolveAppRoute(pathname: string): AppRoute {
  const path = (pathname.replace(/\/+$/, '') || '/').replace(/^\/docs(?=\/|$)/, '') || '/';
  if (path === '/') return { kind: 'list' };
  if (path === SETTINGS_PATH) return { kind: 'settings' };
  const doc = /^\/([^/]+)$/.exec(path);
  if (doc) {
    const docId = decodeSegment(doc[1]);
    if (docId) return { kind: 'doc', docId };
  }
  return { kind: 'list' };
}

export const DOCS_SPACE_PARAM = 'space';
