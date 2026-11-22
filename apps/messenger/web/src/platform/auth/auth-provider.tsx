import { createContext, useContext, type ReactNode } from 'react';

/**
 * Replaces AI-DO's `useAuth()` (token + signed-in user profile). In the miy
 * frame the API credential stays inside the runtime (platform/api/client.ts);
 * `token` is only an opaque marker that the ported code checks before
 * calling the API. The user is the verified caller as the app server reports
 * it (`GET /v1/dm/me`); display settings come from `runtime.settings`.
 */
export const RUNTIME_TOKEN = 'miy-frame-runtime';

export interface AuthUser {
  id: string;
  full_name: string;
  time_zone?: string | null;
  date_format?: string | null;
  locale?: string | null;
}

export interface AuthState {
  token: string | null;
  user: AuthUser | null;
}

const AuthContext = createContext<AuthState>({ token: null, user: null });

export function AuthProvider({
  value,
  children,
}: {
  value: AuthState;
  children: ReactNode;
}) {
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthState {
  return useContext(AuthContext);
}
