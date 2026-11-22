import { PMS_WORKSPACE_KEY } from '../runtime/app-location';

/**
 * AI-DO apps/web/src/platform/workspaces/workspace-utils.ts, reduced to the
 * helpers the PMS screens call. AI-DO built `/w/<slug>/<app><suffix>`
 * addresses; inside the miy frame the app's paths are app-relative, so the
 * workspace and app prefix are dropped and `<suffix>` (or `/`) remains.
 */
export function buildWorkspaceAppPath(
  _workspaceSlug: string,
  _appId: string,
  suffix = '',
): string {
  if (!suffix) return '/';
  return suffix.startsWith('/') ? suffix : `/${suffix}`;
}

/** The app root: there is one PMS per installation. */
export function resolveDefaultWorkspaceAppPath(
  _user: unknown,
  _appId: string,
  suffix = '',
): string {
  return buildWorkspaceAppPath(PMS_WORKSPACE_KEY, 'pms', suffix);
}

export function getCurrentWorkspaceSlug(): string {
  return PMS_WORKSPACE_KEY;
}

/** A `?workspace=` query never selects anything in miy. */
export function getWorkspaceBySlug(
  _user: unknown,
  slug: string | null | undefined,
): { slug: string } | null {
  return slug === PMS_WORKSPACE_KEY ? { slug } : null;
}
