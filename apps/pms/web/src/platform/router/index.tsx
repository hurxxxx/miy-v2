import {
  useCallback,
  useEffect,
  useMemo,
  type AnchorHTMLAttributes,
  type MouseEvent,
  type ReactNode,
} from 'react';

import { useRuntime } from '../runtime/runtime-context';
import { matchPmsRoute, type PmsRouteParams } from '../runtime/app-location';

/**
 * miy replacement for the parts of `react-router-dom` the ported AI-DO PMS
 * screens use (`useNavigate`, `useLocation`, `useSearchParams`, `useParams`,
 * `Link`, `Navigate`). The app runs in a frame without the history API: the
 * portal owns the address and hands the app `runtime.location`
 * (app-relative); every change goes through `runtime.navigate`.
 *
 * Paths are app-relative (`/lists/<id>?task=<id>`). AI-DO built
 * `/w/<slug>/pms/...` paths; the ported path builders now return the part
 * after `/pms` (see platform/workspaces/workspace-utils.ts).
 */

export type To = string | { pathname?: string; search?: string; hash?: string };

export type NavigateOptions = { replace?: boolean; state?: unknown };

export type NavigateFunction = (to: To | number, options?: NavigateOptions) => void;

function resolveTo(to: To, current: { pathname: string; search: string }): string {
  if (typeof to !== 'string') {
    const pathname = to.pathname ?? current.pathname;
    const search = to.search ? (to.search.startsWith('?') ? to.search : `?${to.search}`) : '';
    const hash = to.hash ? (to.hash.startsWith('#') ? to.hash : `#${to.hash}`) : '';
    return `${pathname}${search}${hash}`;
  }
  if (to === '') return `${current.pathname}${current.search}`;
  if (to.startsWith('?') || to.startsWith('#')) return `${current.pathname}${to}`;
  if (to.startsWith('/')) return to;
  // Relative segments are not used by the ported screens; resolve them
  // against the current directory like the router would.
  const base = current.pathname.replace(/\/[^/]*$/, '');
  return `${base}/${to}`;
}

export function useLocation(): { pathname: string; search: string; hash: string; state: null; key: string } {
  const { location } = useRuntime();
  const { pathname, search, hash } = location;
  return useMemo(
    () => ({ pathname, search, hash, state: null, key: `${pathname}${search}${hash}` }),
    [hash, pathname, search],
  );
}

export function useNavigate(): NavigateFunction {
  const { location, navigate } = useRuntime();
  const { pathname, search } = location;
  return useCallback(
    (to: To | number, options?: NavigateOptions) => {
      // History steps (`navigate(-1)`) belong to the portal; the app has none.
      if (typeof to === 'number') return;
      navigate(resolveTo(to, { pathname, search }), { replace: options?.replace });
    },
    [navigate, pathname, search],
  );
}

type SearchParamsInit =
  | URLSearchParams
  | string
  | Record<string, string>
  | Array<[string, string]>;

export type SetURLSearchParams = (
  next: SearchParamsInit | ((current: URLSearchParams) => SearchParamsInit),
  options?: NavigateOptions,
) => void;

export function useSearchParams(): [URLSearchParams, SetURLSearchParams] {
  const { location, navigate } = useRuntime();
  const { pathname, search } = location;
  const params = useMemo(() => new URLSearchParams(search), [search]);
  const setParams = useCallback<SetURLSearchParams>(
    (next, options) => {
      const resolved =
        typeof next === 'function' ? next(new URLSearchParams(search)) : next;
      const query = new URLSearchParams(resolved).toString();
      navigate(`${pathname}${query ? `?${query}` : ''}`, {
        replace: options?.replace,
      });
    },
    [navigate, pathname, search],
  );
  return [params, setParams];
}

/** Route parameters of the current app path (see app-location.ts). */
export function useParams<T = PmsRouteParams>(): Readonly<Partial<T>> {
  const { location } = useRuntime();
  return useMemo(
    () => matchPmsRoute(location.pathname) as unknown as Partial<T>,
    [location.pathname],
  );
}

type LinkProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'href'> & {
  to: To;
  replace?: boolean;
  children?: ReactNode;
};

/**
 * An anchor that navigates through the runtime. The `href` carries the
 * app-relative path for display and accessibility; clicks are handled here
 * (opening a new browser tab is not offered inside the frame).
 */
export function Link({ to, replace, onClick, children, ...rest }: LinkProps) {
  const { location, navigate } = useRuntime();
  const target = resolveTo(to, location);
  const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event);
    if (event.defaultPrevented) return;
    event.preventDefault();
    navigate(target, { replace });
  };
  return (
    <a
      {...rest}
      href={target}
      onClick={handleClick}
      // The frame's own origin has no page at this path; a middle click
      // would open a broken tab, so it is ignored like a plain click.
      onAuxClick={(event) => event.preventDefault()}
    >
      {children}
    </a>
  );
}

/** Navigates once when rendered, like react-router's `<Navigate>`. */
export function Navigate({ to, replace }: { to: To; replace?: boolean }) {
  const { location, navigate } = useRuntime();
  const target = resolveTo(to, location);
  useEffect(() => {
    navigate(target, { replace });
  }, [navigate, replace, target]);
  return null;
}
