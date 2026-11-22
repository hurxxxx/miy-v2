import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import {
  listSpaces,
  type DriveSpace,
} from '../../app-modules/files/api/files-api';
import { useRuntime } from '../runtime/runtime-context';

/**
 * The spaces the person may open (AI-DO: the workspace bootstrap context).
 * Reloaded after space changes; a late answer never replaces a newer one.
 */
interface DriveContextValue {
  spaces: DriveSpace[];
  userId: string | null;
  loading: boolean;
  error: unknown;
  reload: () => Promise<void>;
}

const DriveContext = createContext<DriveContextValue | null>(null);

const LAST_SPACE_KEY = 'miy-drive:last-space';

/** Per-viewer convenience: the space opened last (browser storage may fail). */
export function readLastSpace(): string | null {
  try {
    return window.localStorage.getItem(LAST_SPACE_KEY);
  } catch {
    return null;
  }
}

export function rememberLastSpace(spaceId: string) {
  try {
    window.localStorage.setItem(LAST_SPACE_KEY, spaceId);
  } catch {
    // Storage is optional.
  }
}

export function DriveProvider({ children }: { children: ReactNode }) {
  const runtime = useRuntime();
  const [spaces, setSpaces] = useState<DriveSpace[]>([]);
  const [userId, setUserId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const sequence = useRef(0);

  const reload = useCallback(async () => {
    sequence.current += 1;
    const current = sequence.current;
    setLoading(true);
    try {
      const response = await listSpaces(runtime);
      if (sequence.current !== current) return;
      setSpaces(response.spaces);
      setUserId(response.user_id);
      setError(null);
    } catch (caught) {
      if (sequence.current !== current) return;
      setError(caught);
    } finally {
      if (sequence.current === current) setLoading(false);
    }
  }, [runtime]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const value = useMemo(
    () => ({ spaces, userId, loading, error, reload }),
    [spaces, userId, loading, error, reload],
  );
  return <DriveContext.Provider value={value}>{children}</DriveContext.Provider>;
}

export function useDrive(): DriveContextValue {
  const value = useContext(DriveContext);
  if (!value) {
    throw new Error('DriveProvider is missing');
  }
  return value;
}
