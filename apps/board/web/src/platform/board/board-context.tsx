import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';

import { useAppApi } from '../runtime/runtime-context';

import { getBoardMe, type BoardMe } from './board-api';

/**
 * Replaces AI-DO's `useAuth().user` role checks (`hasAdminConsoleAccess`,
 * `hasAnySystemRole(user, ['platform_admin'])`): whether the caller is a board
 * manager. Presentation only; the server checks every action again.
 */
interface BoardContextValue {
  me: BoardMe | null;
  loading: boolean;
  failed: boolean;
  reload: () => void;
}

const BoardContext = createContext<BoardContextValue>({
  me: null,
  loading: false,
  failed: false,
  reload: () => undefined,
});

export function BoardProvider({ children }: { children: ReactNode }) {
  const api = useAppApi();
  const [me, setMe] = useState<BoardMe | null>(null);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getBoardMe(api)
      .then((value) => {
        if (cancelled) return;
        setMe(value);
        setFailed(false);
      })
      .catch(() => {
        if (cancelled) return;
        setMe(null);
        setFailed(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, reloadKey]);

  const reload = useCallback(() => setReloadKey((key) => key + 1), []);

  return (
    <BoardContext.Provider value={{ me, loading, failed, reload }}>
      {children}
    </BoardContext.Provider>
  );
}

export function useBoard(): BoardContextValue {
  return useContext(BoardContext);
}

/** AI-DO `hasAdminConsoleAccess(user)` for this app: the board manager. */
export function useIsBoardManager(): boolean {
  return Boolean(useBoard().me?.isManager);
}
