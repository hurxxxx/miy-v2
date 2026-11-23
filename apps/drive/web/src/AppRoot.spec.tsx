import { act, fireEvent, render, screen, within } from '@testing-library/react';
import type { AppRuntime } from '@miy/sdk';
import { describe, expect, it, vi } from 'vitest';

import AppRoot from './AppRoot';
import type { FileBrowseResponse, FileFolderItem } from './app-modules/files/api/files-api';

const filesPath = '/v1/spaces/team/files';
const space = { can_manage: true, id: 'team', name: 'Team', role: 'admin' as const };

function folder(id: string, name: string): FileFolderItem {
  return {
    can_manage: true,
    created_at: '2026-11-23T00:00:00',
    updated_at: '2026-11-23T00:00:00',
    id,
    name,
    owner_id: 'user',
    owner_name: 'Test User',
    parent_id: null,
    visibility: 'private',
  };
}

const first = folder('first', 'First Folder');
const second = folder('second', 'Second Folder');

function browse(current: FileFolderItem | null): FileBrowseResponse {
  return {
    all_folders: [first, second],
    breadcrumbs: current ? [current] : [],
    can_ingest: true,
    current_folder: current,
    files: [],
    folders: current ? [] : [first, second],
    ingest_block_reason: null,
    space,
  };
}

function fixture(
  respond: (path: string, init?: RequestInit) => unknown = () => undefined,
) {
  const api = vi.fn(async (path: string, init?: RequestInit) => {
    const response = respond(path, init);
    if (response !== undefined) return response;
    if (path === '/v1/spaces') return { spaces: [space], user_id: 'user' };
    if (path === filesPath) return browse(null);
    if (path === `${filesPath}?folder_id=first`) return browse(first);
    if (path === `${filesPath}?folder_id=second`) return browse(second);
    throw new Error(`Unexpected request ${path}`);
  });
  const runtime: AppRuntime = {
    api: api as AppRuntime['api'],
    apiBlob: vi.fn(),
    appApi: vi.fn(),
    appId: 'drive',
    basePath: '/environments/dev/apps/drive',
    directory: vi.fn(),
    fileApi: vi.fn(),
    installationId: 'installation',
    live: vi.fn(),
    location: { pathname: '/spaces/team', search: '', hash: '' },
    navigate: vi.fn(),
    platformApi: vi.fn(),
    releaseId: 'release',
    setNavigationState: vi.fn(),
    settings: {
      dateFormat: 'iso', locale: 'en-US', theme: 'light',
      themePreference: 'system', timeZone: 'Asia/Seoul',
    },
    theme: 'light',
  };
  const route = (search: string, overrides: Partial<AppRuntime> = {}) => (
    <AppRoot runtime={{ ...runtime, location: { ...runtime.location, search }, ...overrides }} />
  );
  return { api, route, runtime };
}

describe('Drive app frame navigation', () => {
  it('keeps the session and unaffected reads stable through location and display updates', async () => {
    const { api, route, runtime } = fixture();
    const page = render(route('?folder=first'));
    await screen.findByRole('heading', { name: first.name });

    page.rerender(route('?folder=first&sort=name', {
      settings: { ...runtime.settings, timeZone: 'UTC' },
    }));
    expect(api.mock.calls.map(([path]) => path).sort()).toEqual([
      '/v1/spaces', filesPath, `${filesPath}?folder_id=first`,
    ]);

    page.rerender(route('?folder=second'));
    await screen.findByRole('heading', { name: second.name });
    page.rerender(route('?folder=first'));
    await screen.findByRole('heading', { name: first.name });
    expect(api.mock.calls.slice(3).map(([path]) => path)).toEqual([
      `${filesPath}?folder_id=second`, `${filesPath}?folder_id=first`,
    ]);
  });

  it('restores a direct search after opening the empty search view and going Back', async () => {
    const searchPath = `${filesPath}/search`;
    const search = '?view=search&q=report';
    const { api, route } = fixture((path) => path === searchPath ? {
      query: 'report', strategy: 'keyword', page: 1, page_size: 20, hits: [],
      has_more: false, max_ranked_results: 100, latency_ms: 1, trace_id: null,
    } : undefined);
    const page = render(route(search));
    await screen.findByText(/no.*(?:file|result)/i);
    expect(api.mock.calls.filter(([path]) => path === searchPath)).toHaveLength(1);

    page.rerender(route('?view=search'));
    expect(screen.queryByText(/no.*(?:file|result)/i)).toBeNull();
    page.rerender(route(search));
    await screen.findByText(/no.*(?:file|result)/i);
    expect(api.mock.calls.filter(([path]) => path === searchPath)).toHaveLength(2);
  });

  it('does not navigate an already changed folder when deleting the previous folder finishes', async () => {
    let finishDeletion!: (value: null) => void;
    const deletion = new Promise<null>((resolve) => { finishDeletion = resolve; });
    const { route, runtime } = fixture((path, init) =>
      path === `${filesPath}/folders/first` && init?.method === 'DELETE' ? deletion : undefined,
    );
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    try {
      const page = render(route('?folder=first'));
      await screen.findByRole('heading', { name: first.name });
      fireEvent.click(screen.getByRole('button', { name: /^Delete folder$/ }));
      expect(runtime.setNavigationState).toHaveBeenLastCalledWith(expect.objectContaining({ dirty: true }));

      page.rerender(route('?folder=second'));
      await screen.findByRole('heading', { name: second.name });
      await act(async () => finishDeletion(null));
      await screen.findByRole('heading', { name: second.name });
      expect(runtime.navigate).not.toHaveBeenCalled();
    } finally {
      confirm.mockRestore();
    }
  });

  it('discards a space settings draft when moving to another space', async () => {
    const other = { ...space, id: 'other', name: 'Other space' };
    const { route, runtime } = fixture((path) => {
      if (path === '/v1/spaces') return { spaces: [space, other], user_id: 'user' };
      if (path === '/v1/spaces/other/files') return { ...browse(null), space: other };
      return undefined;
    });
    const page = render(route(''));
    fireEvent.click(await screen.findByRole('button', { name: 'Space settings' }));
    const settings = screen.getByRole('dialog');
    fireEvent.change(within(settings).getByDisplayValue(space.name), { target: { value: 'Draft' } });
    page.rerender(route('', { location: { ...runtime.location, pathname: '/spaces/other' } }));

    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Space settings' }));
    expect(within(screen.getByRole('dialog')).getByDisplayValue(other.name)).toBeTruthy();
    expect(screen.queryByDisplayValue('Draft')).toBeNull();
  });
});
