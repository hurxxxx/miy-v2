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
  | { kind: 'home' }
  | { kind: 'space'; spaceId: string }
  | { kind: 'trash'; spaceId: string };

function decodeSegment(value: string): string | null {
  try {
    const decoded = decodeURIComponent(value);
    return decoded.trim() ? decoded : null;
  } catch {
    return null;
  }
}

/**
 * The app's screens. AI-DO's `/w/:workspaceSlug/files` (+ `?folder=`,
 * `?view=search`) becomes `/spaces/<id>` with the same query; the trash
 * (보강) is `/spaces/<id>/trash`. `/` picks a space. Unknown paths show it too.
 */
export function resolveAppRoute(pathname: string): AppRoute {
  const path = pathname.replace(/\/+$/, '') || '/';
  const trash = /^\/spaces\/([^/]+)\/trash$/.exec(path);
  if (trash) {
    const spaceId = decodeSegment(trash[1]);
    if (spaceId) return { kind: 'trash', spaceId };
  }
  const space = /^\/spaces\/([^/]+)$/.exec(path);
  if (space) {
    const spaceId = decodeSegment(space[1]);
    if (spaceId) return { kind: 'space', spaceId };
  }
  return { kind: 'home' };
}

export function buildSpacePath(
  spaceId: string,
  params: { folder?: string | null; view?: 'search' | null } = {},
): string {
  const path = `/spaces/${encodeURIComponent(spaceId)}`;
  const query = new URLSearchParams();
  if (params.view) query.set('view', params.view);
  if (params.folder) query.set('folder', params.folder);
  const text = query.toString();
  return text ? `${path}?${text}` : path;
}

export function buildTrashPath(spaceId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/trash`;
}
