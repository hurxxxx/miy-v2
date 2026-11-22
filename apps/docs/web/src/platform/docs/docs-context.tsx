import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';

import { useAppApi } from '../runtime/runtime-context';

import {
  getDocsMe,
  listSpaces,
  type DocsMe,
  type DocsSpace,
} from './docs-admin-api';

/**
 * Replaces AI-DO's `useAuth().user` and the launcher's space list: whether the
 * caller is a docs manager and which docs spaces they may enter (with their
 * role). Presentation only; the server checks every action again.
 */
interface DocsAppContextValue {
  me: DocsMe | null;
  spaces: DocsSpace[];
  loading: boolean;
  failed: boolean;
  reload: () => void;
}

const DocsAppContext = createContext<DocsAppContextValue>({
  me: null,
  spaces: [],
  loading: false,
  failed: false,
  reload: () => undefined,
});

export function DocsAppProvider({ children }: { children: ReactNode }) {
  const api = useAppApi();
  const [me, setMe] = useState<DocsMe | null>(null);
  const [spaces, setSpaces] = useState<DocsSpace[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    Promise.all([getDocsMe(api), listSpaces(api)])
      .then(([value, items]) => {
        if (cancelled) return;
        setMe(value);
        setSpaces(items);
        setFailed(false);
      })
      .catch(() => {
        if (cancelled) return;
        // Keep the last known spaces: a failed refresh must not empty the
        // space choice the person is working in.
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
    <DocsAppContext.Provider value={{ me, spaces, loading, failed, reload }}>
      {children}
    </DocsAppContext.Provider>
  );
}

export function useDocsApp(): DocsAppContextValue {
  return useContext(DocsAppContext);
}

/** AI-DO's app administrator for this app: the docs manager. */
export function useIsDocsManager(): boolean {
  return Boolean(useDocsApp().me?.isManager);
}
