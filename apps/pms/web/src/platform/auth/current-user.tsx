import type { paths } from '../../api.generated';
import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import { ContentState } from '@miy/ui';

import { useRuntime } from '../runtime/runtime-context';

/**
 * Replaces AI-DO's `useAuth()` user. AI-DO's auth provider held the signed-in
 * user's profile (id, primary department, time zone, system roles) and a
 * bearer token. In miy the server takes the caller from the gateway's signed
 * identity, so there is no token in app code; the screens read who they are
 * from the app's own `GET /v1/pms/me`. The time zone is the portal display
 * setting (`runtime.settings.timeZone`). miy gives apps no admin flag, and a
 * PMS admin bypass never existed for space membership, so there is no
 * `system_roles` here.
 */
export type PmsCurrentUser = paths['/v1/pms/me']['get']['responses'][200]['content']['application/json'];

type CurrentUserState =
  | { status: 'loading' }
  | { status: 'ready'; user: PmsCurrentUser }
  | { status: 'error' };

const CurrentUserContext = createContext<PmsCurrentUser | null>(null);

export function CurrentUserContextProvider({
  user,
  children,
}: {
  user: PmsCurrentUser | null;
  children: ReactNode;
}) {
  return (
    <CurrentUserContext.Provider value={user}>{children}</CurrentUserContext.Provider>
  );
}

/** Resolves the PMS user once for this app frame, independently of its route. */
export function PmsCurrentUserProvider({ children }: { children: ReactNode }) {
  const { api } = useRuntime();
  const { t } = useTranslation('shell');
  const [state, setState] = useState<CurrentUserState>({ status: 'loading' });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setState({ status: 'loading' });
    api<PmsCurrentUser>('/v1/pms/me', { signal: controller.signal })
      .then((user) => {
        if (!controller.signal.aborted) setState({ status: 'ready', user });
      })
      .catch(() => {
        if (!controller.signal.aborted) setState({ status: 'error' });
      });
    return () => controller.abort();
  }, [api, attempt]);
  if (state.status === 'loading') {
    return (
      <ContentState className="m-4" kind="loading" title={t('appRuntime.loading')} />
    );
  }
  if (state.status === 'error') {
    return (
      <ContentState
        className="m-4"
        kind="error"
        title={t('appRuntime.loadFailed')}
        action={{
          label: t('appRuntime.retry'),
          onClick: () => setAttempt((value) => value + 1),
        }}
      />
    );
  }
  return (
    <CurrentUserContextProvider user={state.user}>{children}</CurrentUserContextProvider>
  );
}

export function usePmsCurrentUser(): PmsCurrentUser | null {
  return useContext(CurrentUserContext);
}
