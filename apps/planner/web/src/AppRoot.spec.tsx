import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AppRuntime, AppSettings } from '@miy/sdk';

import AppRoot from './AppRoot';
import { syncLocale } from './platform/i18n';

// Smoke test of the frame entry with a fake runtime (miy).
function runtime(pathname: string, settings: Partial<AppSettings> = {}) {
  const api = vi.fn(async (path: string) => {
    if (path.startsWith('/v1/calendar/events')) return { items: [] };
    if (path.startsWith('/v1/personal-widgets/todos')) return { items: [] };
    if (path.startsWith('/v1/personal-widgets/memo'))
      return { id: null, body: '', createdAt: null, updatedAt: null };
    throw new Error('unexpected ' + path);
  });
  const value = {
    installationId: 'i',
    appId: 'planner',
    releaseId: 'r',
    basePath: '/apps/planner',
    location: { pathname, search: '', hash: '' },
    navigate: vi.fn(),
    api,
    setNavigationState: vi.fn(),
    theme: 'light',
    settings: {
      theme: 'light',
      themePreference: 'system',
      locale: 'ko-KR',
      timeZone: 'Asia/Seoul',
      dateFormat: 'korean',
      ...settings,
    },
  } as unknown as AppRuntime;
  return { value, api };
}

afterEach(() => {
  cleanup();
  syncLocale('ko-KR');
});

describe('AppRoot', () => {
  it('renders the to-do panel route in English without the planner', async () => {
    const { value, api } = runtime('/widgets/todos', { locale: 'en-US' });
    render(<AppRoot runtime={value} />);
    expect(await screen.findByRole('heading', { name: 'Todo' })).toBeTruthy();
    expect(await screen.findByText('No todos yet.')).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Planner' })).toBeNull();
    expect(api).toHaveBeenCalledWith('/v1/personal-widgets/todos?include_completed=true', {});
    expect(value.setNavigationState).toHaveBeenCalledWith({
      dirty: false,
      message: 'You have unsaved changes. Leave anyway?',
    });
  });

  it('renders the memo panel route in Korean', async () => {
    const { value } = runtime('/widgets/memos');
    render(<AppRoot runtime={value} />);
    expect(await screen.findByRole('heading', { name: '메모' })).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByLabelText('개인 메모')).toBeTruthy(),
    );
  });

  it('renders the planner with the dock in English', async () => {
    const { value, api } = runtime('/', { locale: 'en-US' });
    render(<AppRoot runtime={value} />);
    expect(await screen.findByRole('heading', { name: 'Planner' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Open todo' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Open memo' })).toBeTruthy();
    await waitFor(() =>
      expect(
        api.mock.calls.some(([path]) =>
          String(path).startsWith('/v1/calendar/events?') &&
          String(path).includes('sources=planner_event'),
        ),
      ).toBe(true),
    );
    expect(await screen.findByText('Event filter')).toBeTruthy();
  });
});
