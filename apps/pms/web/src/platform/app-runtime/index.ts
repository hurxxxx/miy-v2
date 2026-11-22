import { useParams } from '../router';
import type { PmsRouteParams } from '../runtime/app-location';

/**
 * AI-DO apps/web/src/platform/app-runtime, reduced to what the PMS screens
 * import.
 *
 * AI-DO ran PMS either in its app-first runtime (`/pms`, `/pms/s/:spaceKey`,
 * one runtime per platform space) or on the workspace routes
 * (`/w/:workspaceSlug/pms/...`, `useOptionalAppRuntime()` null). miy has one
 * PMS per installation and addresses spaces and lists by id
 * (`/spaces/:spaceId`, `/lists/:taskListId`), which is the workspace-route
 * shape, so the ported screens take their no-runtime branches. The global
 * overview, which AI-DO showed only in the app-first runtime, is opened
 * directly at `/` (see PMSView.tsx).
 */
export interface AppRuntime {
  app: {
    app_id: string;
    title: string;
    route_base: string;
    can_create_space: boolean;
  };
  space: {
    id: string;
    key: string;
    slug: string;
    name: string;
    kind: 'app_global' | 'app_space';
    ownerAppId: string;
    role: string;
  };
  capabilityAppIds: ReadonlySet<string>;
  href: (path?: string) => string;
}

export function useOptionalAppRuntime(): AppRuntime | null {
  return null;
}

/** Old app views keep the parameter name while the route owns its value. */
export function useAppRouteParams<
  T extends Record<string, string | undefined> = PmsRouteParams,
>(): Readonly<Partial<T>> & { workspaceSlug?: string; toolId?: string } {
  return useParams<T>() as Readonly<Partial<T>> & {
    workspaceSlug?: string;
    toolId?: string;
  };
}
