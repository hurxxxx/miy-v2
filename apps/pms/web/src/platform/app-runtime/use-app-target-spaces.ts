import { useEffect, useMemo, useState } from 'react';

import { listSpaces, type PmsSpace } from '../../app-modules/pms/api/pms-api';
import { PMS_WORKSPACE_KEY } from '../runtime/app-location';
import { useAppApi } from '../runtime/runtime-context';

/**
 * AI-DO apps/web/src/platform/app-runtime/use-app-target-spaces.ts for the
 * PMS app only. AI-DO read the platform's app-space catalog
 * (`/api/v1/apps/pms/spaces`); in miy the PMS spaces are the app's own, so the
 * choices come from `GET /v1/pms/spaces` (each space's `workspace_key` and
 * `current_user_role`). The first choice is the PMS-wide catalog
 * (`app_global`, every list the caller can read), as AI-DO's global space.
 * Viewers are left out unless `includeViewers`.
 */
export interface AppTargetSpaceChoice {
  key: string;
  name: string;
  kind: 'app_global' | 'app_space';
  role: string;
  ownerAppId: string;
}

export function useAppTargetSpaces(
  appId: string,
  { includeViewers = false }: { includeViewers?: boolean; context?: 'owner' | 'capability' } = {},
) {
  const api = useAppApi();
  const [result, setResult] = useState<{
    spaces: PmsSpace[];
    error: boolean;
  } | null>(null);
  useEffect(() => {
    if (appId !== 'pms') return undefined;
    let cancelled = false;
    listSpaces(api)
      .then((spaces) => {
        if (!cancelled) setResult({ spaces, error: false });
      })
      .catch(() => {
        if (!cancelled) setResult({ spaces: [], error: true });
      });
    return () => {
      cancelled = true;
    };
  }, [api, appId]);
  const choices = useMemo((): AppTargetSpaceChoice[] => {
    if (appId !== 'pms') return [];
    return [
      {
        key: PMS_WORKSPACE_KEY,
        name: 'PMS',
        kind: 'app_global',
        role: 'member',
        ownerAppId: 'pms',
      },
      ...(result?.spaces ?? [])
        .map((space) => ({
          key: space.workspace_key,
          name: space.name,
          kind: 'app_space' as const,
          role: space.current_user_role ?? '',
          ownerAppId: 'pms',
        }))
        .filter((space) => includeViewers || space.role !== 'viewer'),
    ];
  }, [appId, includeViewers, result]);
  return {
    legacy: false,
    choices,
    defaultKey: null as string | null,
    error: Boolean(result?.error),
    loading: appId === 'pms' && result === null,
  };
}
