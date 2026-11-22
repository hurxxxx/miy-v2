import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { COMMUNITY_CHANNELS_CHANGED_EVENT } from '../../../platform/community/community-channel-events';
import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  invalidateCommunityChannelsCache,
  listCommunityChannels,
  listCommunityPosts,
  markCommunityPostRead,
  resolveCommunityPostMediaUrls,
  type FileApi,
} from './community-api';

// Ported from AI-DO app-modules/community/api/community-api.spec.ts. AI-DO
// mocked `fetch` with a token; miy calls `runtime.api` (mocked here) with
// app-relative paths, and images come through `runtime.fileApi`.
describe('community API channel cache', () => {
  let api: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    invalidateCommunityChannelsCache();
    api = vi.fn();
  });

  afterEach(() => {
    invalidateCommunityChannelsCache();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const runtimeApi = () => api as unknown as AppApi;

  it('shares in-flight channel loads and reuses the resolved response', async () => {
    api.mockResolvedValueOnce({
      channels: [channel({ key: 'suggestions', name: 'Suggestions' })],
    });

    const [first, second] = await Promise.all([
      listCommunityChannels(runtimeApi()),
      listCommunityChannels(runtimeApi()),
    ]);
    const cached = await listCommunityChannels(runtimeApi());

    expect(api).toHaveBeenCalledTimes(1);
    expect(api).toHaveBeenCalledWith('/v1/community/channels', {});
    expect(first.channels[0].key).toBe('suggestions');
    expect(second.channels[0].key).toBe('suggestions');
    expect(cached.channels[0].key).toBe('suggestions');
    expect(cached).not.toBe(first);
    expect(cached.channels[0]).not.toBe(first.channels[0]);
  });

  it('reloads channel cache after explicit refresh or change event', async () => {
    api.mockResolvedValueOnce({
      channels: [channel({ key: 'suggestions', name: 'Suggestions' })],
    });
    await listCommunityChannels(runtimeApi());

    api.mockResolvedValueOnce({
      channels: [channel({ key: 'notice', name: 'Notice' })],
    });
    const refreshed = await listCommunityChannels(runtimeApi(), {
      forceRefresh: true,
    });
    expect(refreshed.channels[0].key).toBe('notice');

    window.dispatchEvent(new Event(COMMUNITY_CHANNELS_CHANGED_EVENT));

    api.mockResolvedValueOnce({
      channels: [channel({ key: 'qna', name: 'Q&A' })],
    });
    const afterEvent = await listCommunityChannels(runtimeApi());

    expect(api).toHaveBeenCalledTimes(3);
    expect(afterEvent.channels[0].key).toBe('qna');
  });

  it('resolves community post media with the post password context', async () => {
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: vi.fn(() => 'blob:local-1') }));
    const fileApi = vi.fn().mockResolvedValue(
      new Response(new Blob(['png'], { type: 'image/png' }), { status: 200 }),
    );

    const response = await resolveCommunityPostMediaUrls(
      fileApi as unknown as FileApi,
      'post 1',
      ['media:00000000-0000-0000-0000-000000000001'],
      'secret-pw',
    );

    expect(response.resolved).toEqual({
      'media:00000000-0000-0000-0000-000000000001': 'blob:local-1',
    });
    expect(fileApi).toHaveBeenCalledWith(
      '/community/posts/post%201/media/00000000-0000-0000-0000-000000000001',
      expect.objectContaining({
        body: JSON.stringify({ password: 'secret-pw' }),
        method: 'POST',
      }),
    );
  });

  it('rejects a denied secret post image', async () => {
    const fileApi = vi.fn().mockResolvedValue(new Response(null, { status: 403 }));
    await expect(
      resolveCommunityPostMediaUrls(fileApi as unknown as FileApi, 'p', ['media:x'], null),
    ).rejects.toMatchObject({ status: 403 });
  });

  it('sends post list pagination and search parameters', async () => {
    api.mockResolvedValueOnce({ page: 1, pageSize: 20, posts: [], total: 0 });

    await listCommunityPosts(runtimeApi(), 'team news', {
      page: 2,
      pageSize: 50,
      search: ' release ',
    });

    expect(api).toHaveBeenCalledWith(
      '/v1/community/channels/team%20news/posts?page=2&page_size=50&q=release',
      expect.any(Object),
    );
  });

  it('marks a community post as read with an idempotent request', async () => {
    api.mockResolvedValueOnce(undefined);

    await markCommunityPostRead(runtimeApi(), 'post 1');

    expect(api).toHaveBeenCalledWith(
      '/v1/community/posts/post%201/read',
      expect.objectContaining({ method: 'POST' }),
    );
  });
});

function channel(
  overrides: Partial<
    Awaited<ReturnType<typeof listCommunityChannels>>['channels'][number]
  > = {},
) {
  return {
    active: true,
    adminOnlyContent: false,
    description: '',
    forceAnonymous: false,
    id: `channel-${overrides.key ?? 'suggestions'}`,
    key: 'suggestions',
    name: 'Suggestions',
    position: 0,
    readOnly: false,
    templateBody: '',
    templateTitle: '',
    ...overrides,
  };
}
