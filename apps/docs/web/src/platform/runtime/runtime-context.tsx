import { createContext, useContext, type ReactNode } from 'react';
import type { AppRuntime } from '@miy/sdk';

/**
 * Replaces AI-DO's `useAuth()` (token + user profile). The frame runtime gives
 * the app its own API client and the portal display settings; the server takes
 * the caller identity from the gateway, never from the client.
 */
export type AppApi = AppRuntime['api'];
export type AppFileApi = AppRuntime['fileApi'];

const RuntimeContext = createContext<AppRuntime | null>(null);

export function RuntimeProvider({
  runtime,
  children,
}: {
  runtime: AppRuntime;
  children: ReactNode;
}) {
  return (
    <RuntimeContext.Provider value={runtime}>{children}</RuntimeContext.Provider>
  );
}

export function useRuntime(): AppRuntime {
  const runtime = useContext(RuntimeContext);
  if (!runtime) {
    throw new Error('RuntimeProvider is missing');
  }
  return runtime;
}

/** The app's own API client (`runtime.api`). */
export function useAppApi(): AppApi {
  return useRuntime().api;
}

/** The platform file gateway (`runtime.fileApi`). */
export function useAppFileApi(): AppFileApi {
  return useRuntime().fileApi;
}
