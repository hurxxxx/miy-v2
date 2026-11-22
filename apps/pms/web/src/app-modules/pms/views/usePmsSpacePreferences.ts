import { useCallback, useEffect, useRef, useState } from 'react';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import {
  listSpacePreferences,
  updateSpacePreference,
  type PmsSpacePreference,
  type PmsSpacePreferenceChange,
} from '../api/pms-api';
import {
  PMS_SPACE_PREFERENCE_CHANGED_EVENT,
  dispatchPmsSpacePreferenceChanged,
} from './pms-events';

export type PmsSpacePreferenceMap = ReadonlyMap<string, PmsSpacePreference>;

const EMPTY_PREFERENCES: PmsSpacePreferenceMap = new Map();

function withPreference(
  current: PmsSpacePreferenceMap,
  preference: PmsSpacePreference,
): PmsSpacePreferenceMap {
  const next = new Map(current);
  if (preference.pinned || preference.hidden) {
    next.set(preference.space_id, preference);
  } else {
    next.delete(preference.space_id);
  }
  return next;
}

/** The current user's pinned and hidden spaces, kept in step across views. */
export function usePmsSpacePreferences(workspaceSlug?: string | null): {
  preferences: PmsSpacePreferenceMap;
  setSpacePreference: (
    spaceId: string,
    change: PmsSpacePreferenceChange,
  ) => Promise<void>;
} {
  const api = useAppApi();
  const [preferences, setPreferences] = useState<PmsSpacePreferenceMap>(
    () => new Map(),
  );
  // Changes saved while the first load is in flight; the load's snapshot may
  // predate them, so they are applied again on top of it.
  const changedDuringLoadRef = useRef<Map<string, PmsSpacePreference> | null>(
    null,
  );

  useEffect(() => {
    if (!api) return undefined;
    let cancelled = false;
    const changedDuringLoad = new Map<string, PmsSpacePreference>();
    changedDuringLoadRef.current = changedDuringLoad;
    const stopCollecting = () => {
      if (changedDuringLoadRef.current === changedDuringLoad) {
        changedDuringLoadRef.current = null;
      }
    };
    void listSpacePreferences(api, workspaceSlug)
      .then(
        (response) => {
          if (cancelled) return;
          let loaded: PmsSpacePreferenceMap = new Map(
            response.items.map((item) => [item.space_id, item]),
          );
          for (const change of changedDuringLoad.values()) {
            loaded = withPreference(loaded, change);
          }
          setPreferences(loaded);
        },
        () => undefined,
      )
      .finally(stopCollecting);
    return () => {
      cancelled = true;
      stopCollecting();
      // Another workspace or account must not show this one's placement,
      // even if its own load then fails.
      setPreferences(EMPTY_PREFERENCES);
    };
  }, [api, workspaceSlug]);

  useEffect(() => {
    const handleChanged = (event: Event) => {
      const detail = (event as CustomEvent<PmsSpacePreference>).detail;
      if (!detail) return;
      changedDuringLoadRef.current?.set(detail.space_id, detail);
      setPreferences((current) => withPreference(current, detail));
    };
    window.addEventListener(PMS_SPACE_PREFERENCE_CHANGED_EVENT, handleChanged);
    return () =>
      window.removeEventListener(
        PMS_SPACE_PREFERENCE_CHANGED_EVENT,
        handleChanged,
      );
  }, []);

  const setSpacePreference = useCallback(
    async (spaceId: string, change: PmsSpacePreferenceChange) => {
      if (!api) return;
      const saved = await updateSpacePreference(
        api,
        spaceId,
        change,
        workspaceSlug,
      );
      // Every mounted copy of this hook, this one included, picks it up.
      dispatchPmsSpacePreferenceChanged(saved);
    },
    [api, workspaceSlug],
  );

  return { preferences, setSpacePreference };
}
