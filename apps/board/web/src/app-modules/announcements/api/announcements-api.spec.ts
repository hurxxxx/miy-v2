import { describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  createAnnouncement,
  listAnnouncements,
  updateAnnouncement,
} from './announcements-api';

// Ported from AI-DO app-modules/announcements/api/announcements-api.spec.ts.
// AI-DO mocked `fetch` and checked the workspace-rewritten path; miy calls
// `runtime.api` with the board space in the path.
const notice = {
  id: 'notice-1',
  workspaceId: 'home-one',
  authorId: 'author',
  authorName: 'Author',
  scope: 'company',
  title: 'Notice',
  body: 'Content',
  isPinned: false,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
};

describe('announcement resource permissions', () => {
  it('maps list permissions fail closed without inferring them from author or workspace metadata', async () => {
    const api = vi.fn().mockResolvedValue({
      items: [
        { ...notice, id: 'allowed', canManage: true },
        { ...notice, id: 'denied', canManage: false },
        { ...notice, id: 'missing' },
      ],
    });
    const response = await listAnnouncements(api as unknown as AppApi, 'home-one', {
      scope: 'company',
    });
    expect(response.items.map((item) => item.canManage)).toEqual([
      true,
      false,
      false,
    ]);
    expect(api.mock.calls[0][0]).toBe(
      '/v1/spaces/home-one/announcements?scope=company',
    );
  });

  it('preserves the server permission returned by create and update', async () => {
    const api = vi
      .fn()
      .mockResolvedValueOnce({ ...notice, canManage: true })
      .mockResolvedValueOnce({ ...notice, canManage: false });
    expect(
      (
        await createAnnouncement(api as unknown as AppApi, 'home-one', {
          title: 'Notice',
          scope: 'company',
        })
      ).canManage,
    ).toBe(true);
    expect(
      (
        await updateAnnouncement(api as unknown as AppApi, 'home-one', 'notice-1', {
          title: 'Updated',
        })
      ).canManage,
    ).toBe(false);
    expect(api.mock.calls[0][0]).toBe('/v1/spaces/home-one/announcements');
    expect(api.mock.calls[1][0]).toBe(
      '/v1/spaces/home-one/announcements/notice-1',
    );
  });

  it('reaches company notices without a space (miy)', async () => {
    const api = vi.fn().mockResolvedValue({ items: [] });
    await listAnnouncements(api as unknown as AppApi, null, {
      scope: 'company',
      limit: 8,
    });
    expect(api.mock.calls[0][0]).toBe('/v1/announcements?scope=company&limit=8');
  });
});
