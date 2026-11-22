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
  | { kind: 'community'; postId: string | null }
  | { kind: 'notices'; noticeId: string | null }
  | { kind: 'admin' };

function decodeSegment(value: string): string | null {
  try {
    const decoded = decodeURIComponent(value);
    return decoded.trim() ? decoded : null;
  } catch {
    return null;
  }
}

/**
 * The app's screens. AI-DO's community routes (`/community`,
 * `/community/posts/:postId`) are kept; `/` is the community too. Notices
 * (AI-DO's home announcement boards) and board settings (AI-DO's admin
 * console sections) are routes of this app. Unknown paths show the community.
 */
export function resolveAppRoute(pathname: string): AppRoute {
  const path = pathname.replace(/\/+$/, '') || '/';
  const post = /^\/community\/posts\/([^/]+)$/.exec(path);
  if (post) {
    const postId = decodeSegment(post[1]);
    if (postId) return { kind: 'community', postId };
  }
  if (path === '/notices') return { kind: 'notices', noticeId: null };
  const notice = /^\/notices\/([^/]+)$/.exec(path);
  if (notice) {
    return { kind: 'notices', noticeId: decodeSegment(notice[1]) };
  }
  if (path === '/admin') return { kind: 'admin' };
  return { kind: 'community', postId: null };
}

export const NOTICES_PATH = '/notices';
export const ADMIN_PATH = '/admin';

export function buildNoticesUrl(
  spaceId?: string | null,
  noticeId?: string | null,
): string {
  const path = noticeId
    ? `${NOTICES_PATH}/${encodeURIComponent(noticeId)}`
    : NOTICES_PATH;
  if (!spaceId) return path;
  const params = new URLSearchParams();
  params.set('space', spaceId);
  return `${path}?${params.toString()}`;
}
