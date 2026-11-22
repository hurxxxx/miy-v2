import { useCallback, useMemo } from 'react';

import { useRuntime } from './runtime-context';

/**
 * Replaces react-router's `useSearchParams` in the ported screens. The app
 * runs in a frame without the history API: the portal owns the address and
 * hands the app `runtime.location`; changes go through `runtime.navigate`
 * (app-relative paths under the app's base path).
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

export type AppRoute =
  | { kind: 'planner' }
  | { kind: 'widget'; widget: 'todo' | 'memo' };

/** The app's screens: the planner, and the to-do / memo panel routes. */
export function resolveAppRoute(pathname: string): AppRoute {
  const path = pathname.replace(/\/+$/, '') || '/';
  if (path === '/widgets/todos') return { kind: 'widget', widget: 'todo' };
  if (path === '/widgets/memos') return { kind: 'widget', widget: 'memo' };
  return { kind: 'planner' };
}

export const WIDGET_ROUTE_PATHS = {
  todo: '/widgets/todos',
  memo: '/widgets/memos',
} as const;
