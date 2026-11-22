import { render, type RenderOptions } from '@testing-library/react';
import type { ReactElement, ReactNode } from 'react';
import { vi } from 'vitest';
import type { AppRuntime, AppSettings } from '@miy/sdk';

import {
  CurrentUserContextProvider,
  type PmsCurrentUser,
} from '../platform/auth/current-user';
import { RuntimeProvider } from '../platform/runtime/runtime-context';

/**
 * Test helpers for the ported component specs (miy). AI-DO specs rendered in
 * a `MemoryRouter` and mocked `useAuth()`; here a fake frame runtime carries
 * the location, navigation and API, and the current user comes from the
 * context `PmsCurrentUserProvider` fills from `/v1/pms/me`.
 */
export type TestRuntimeOverrides = Partial<Omit<AppRuntime, 'location' | 'settings'>> & {
  location?: Partial<AppRuntime['location']>;
  settings?: Partial<AppSettings>;
  /** Shorthand for `location`: `/lists/1?task=2`. */
  path?: string;
};

export function createTestRuntime(overrides: TestRuntimeOverrides = {}): AppRuntime {
  const { path, location, settings, ...rest } = overrides;
  const [pathname, search = ''] = (path ?? '/').split('?');
  return {
    installationId: 'installation-1',
    appId: 'pms',
    releaseId: 'release-1',
    basePath: '/apps/pms',
    navigate: vi.fn(),
    api: vi.fn(async () => {
      throw new Error('unexpected api call');
    }) as unknown as AppRuntime['api'],
    apiBlob: vi.fn(async () => new Blob()) as unknown as AppRuntime['apiBlob'],
    fileApi: vi.fn(async () => new Response('{}')) as unknown as AppRuntime['fileApi'],
    platformApi: vi.fn() as unknown as AppRuntime['platformApi'],
    directory: {} as AppRuntime['directory'],
    appApi: vi.fn() as unknown as AppRuntime['appApi'],
    setNavigationState: vi.fn(),
    live: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
    theme: 'light',
    ...rest,
    location: {
      pathname: pathname || '/',
      search: search ? `?${search}` : '',
      hash: '',
      ...location,
    },
    settings: {
      theme: 'light',
      themePreference: 'light',
      locale: 'ko-KR',
      timeZone: 'Asia/Seoul',
      dateFormat: 'korean',
      ...settings,
    },
  };
}

export const TEST_USER: PmsCurrentUser = {
  id: 'user-1',
  name: '테스트 사용자',
  primary_org_unit_id: null,
  primary_org_unit_name: null,
};

export function TestRuntimeProviders({
  runtime,
  user = TEST_USER,
  children,
}: {
  runtime: AppRuntime;
  user?: PmsCurrentUser | null;
  children: ReactNode;
}) {
  return (
    <RuntimeProvider runtime={runtime}>
      <CurrentUserContextProvider user={user}>{children}</CurrentUserContextProvider>
    </RuntimeProvider>
  );
}

export function renderWithRuntime(
  ui: ReactElement,
  {
    runtime = createTestRuntime(),
    user = TEST_USER,
    ...options
  }: { runtime?: AppRuntime; user?: PmsCurrentUser | null } & Omit<
    RenderOptions,
    'wrapper'
  > = {},
) {
  const result = render(ui, {
    ...options,
    wrapper: ({ children }) => (
      <TestRuntimeProviders runtime={runtime} user={user}>
        {children}
      </TestRuntimeProviders>
    ),
  });
  return { ...result, runtime };
}
