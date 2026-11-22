import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { AppRuntime } from '@miy/sdk';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../../platform/i18n/i18n';
import { RuntimeProvider } from '../../../platform/runtime/runtime-context';
import { AnnouncementsBoard } from './AnnouncementsBoard';

// Ported from AI-DO app-modules/home/views/WorkspaceHomeView/AnnouncementsBoard.spec.tsx.
// AI-DO derived "may create" from the Home space role or the platform admin
// role; miy takes the space's canManage (space admin, from the server) and
// the board manager flag. The notice's own canManage still decides edit/delete.
const board = vi.hoisted(() => ({ manager: false }));
vi.mock('../../../platform/board/board-context', () => ({
  useIsBoardManager: () => board.manager,
}));

function runtimeWith(api: ReturnType<typeof vi.fn>): AppRuntime {
  return {
    api,
    settings: {
      dateFormat: 'iso',
      locale: 'en-US',
      theme: 'light',
      themePreference: 'light',
      timeZone: 'UTC',
    },
  } as unknown as AppRuntime;
}

function renderBoard(canManage: boolean | undefined) {
  const api = vi.fn().mockResolvedValue({
    items: [
      {
        id: 'notice-1',
        workspaceId: null,
        authorId: 'author',
        authorName: 'Author',
        scope: 'company',
        title: 'Notice title',
        body: 'Notice content',
        isPinned: false,
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
        canManage,
      },
    ],
  });
  render(
    <RuntimeProvider runtime={runtimeWith(api)}>
      <AnnouncementsBoard scope="company" title="Company notices" spaceId="home-one" />
    </RuntimeProvider>,
  );
  return api;
}

beforeEach(async () => {
  board.manager = false;
  await i18n.changeLanguage('en-US');
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('announcement resource management actions', () => {
  it.each([
    [true, false, true],
    [true, true, true],
    [false, false, false],
    [false, true, false],
  ] as const)(
    'uses the space admin flag %s (board manager: %s) for new space announcements',
    async (spaceCanManage, manager, expected) => {
      board.manager = manager;
      const api = vi.fn().mockResolvedValue({ items: [] });
      render(
        <RuntimeProvider runtime={runtimeWith(api)}>
          <AnnouncementsBoard
            scope="workspace"
            title="Team notices"
            spaceId="space-1"
            spaceCanManage={spaceCanManage}
          />
        </RuntimeProvider>,
      );
      await screen.findByRole('heading', { name: 'Team notices' });
      expect(
        Boolean(screen.queryByRole('button', { name: 'New announcement' })),
      ).toBe(expected);
      expect(api).toHaveBeenCalledWith(
        '/v1/spaces/space-1/announcements?scope=workspace&limit=8',
        {},
      );
    },
  );

  it('lets a notice manager edit and delete without board management', async () => {
    renderBoard(true);
    fireEvent.click(await screen.findByRole('button', { name: /Notice title/ }));
    expect(screen.getByRole('button', { name: 'Edit' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'New announcement' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(
      (screen.getByPlaceholderText('Announcement title') as HTMLInputElement).value,
    ).toBe('Notice title');
  });

  it.each([false, undefined])(
    'does not give a board manager edit/delete when canManage is %s',
    async (canManage) => {
      board.manager = true;
      renderBoard(canManage);
      fireEvent.click(await screen.findByRole('button', { name: /Notice title/ }));
      expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull();
      expect(screen.queryByRole('button', { name: 'Delete' })).toBeNull();
      expect(screen.getByRole('button', { name: 'New announcement' })).toBeTruthy();
    },
  );

  it('opens the notice of a notification link (miy)', async () => {
    const api = vi.fn(async (path: string) =>
      path.includes('/announcements/notice-9')
        ? {
            id: 'notice-9',
            workspaceId: null,
            authorId: 'a',
            authorName: 'A',
            scope: 'company',
            title: 'Linked notice',
            body: 'Linked body',
            isPinned: false,
            createdAt: '2026-01-01T00:00:00Z',
            updatedAt: '2026-01-01T00:00:00Z',
          }
        : { items: [] },
    );
    const onChange = vi.fn();
    render(
      <RuntimeProvider runtime={runtimeWith(api as unknown as ReturnType<typeof vi.fn>)}>
        <AnnouncementsBoard
          scope="company"
          title="Company notices"
          openNoticeId="notice-9"
          onOpenNoticeChange={onChange}
        />
      </RuntimeProvider>,
    );
    expect(await screen.findByText('Linked body')).toBeTruthy();
    expect(api).toHaveBeenCalledWith('/v1/announcements/notice-9', {});
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(onChange).toHaveBeenCalledWith(null);
  });
});
