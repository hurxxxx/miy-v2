import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { readJson } from '@miy/sdk';

import { ApiRequestError, bindApiRuntime } from '../../../platform/api/client';
import { DM_MAX_ATTACHMENT_BYTES } from '../../../contracts/dm';

import {
  createDirectDmConversation,
  createGroupDmConversation,
  addDmConversationParticipants,
  listDmConversations,
  searchDmUsers,
  reportDm,
  sendDmMessage,
  setDmBlock,
  uploadDmAttachment,
  editDmMessage,
  deleteDmMessage,
  setDmConversationMuted,
  cancelDmAttachment,
} from './dm-api';

describe('web DM API client', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    globalThis.fetch = vi.fn() as unknown as typeof globalThis.fetch;
    // miy: requests go through the bound frame runtime; this stand-in keeps
    // AI-DO's fetch expectations (no-store reads of the app API).
    bindApiRuntime({
      api: async <T,>(path: string, init: RequestInit = {}) =>
        readJson<T>(await fetch(path, { ...init, cache: init.cache ?? 'no-store' }), init),
      fileApi: (path, init) => fetch(path, init),
    });
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('normalizes DM conversation responses through the shared contract', async () => {
    mockJsonResponse({ items: [conversation()] });

    await expect(listDmConversations('token-1')).resolves.toEqual({
      items: [
        {
          ...conversation(),
          title: null,
          other_user: null,
          last_message: null,
          last_read_message_id: null,
          muted_at: null,
          peer_blocked: false,
        },
      ],
      safety: null,
    });
    expect(fetchMock()).toHaveBeenCalledWith(
      '/v1/dm/conversations',
      expect.objectContaining({
        cache: 'no-store',
      }),
    );
  });

  it('rejects malformed DM responses before callers render them', async () => {
    const payload = [
      {
        id: 'u/1',
        email: 'user@example.test',
        full_name: 'User One',
      },
    ];
    mockJsonResponse(payload);

    let caught: unknown;
    try {
      await searchDmUsers('token-1', 'user');
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(ApiRequestError);
    expect(caught).toMatchObject({
      status: 502,
      message: 'Invalid response format for dm:users.',
      payload,
    });
  });

  it('searches the global user directory with the requested limit', async () => {
    mockJsonResponse([]);

    await expect(
      searchDmUsers('token-1', 'user', {
        limit: 10,
      }),
    ).resolves.toEqual([]);

    expect(fetchMock()).toHaveBeenCalledWith(
      '/v1/dm/users?q=user&limit=10',
      expect.objectContaining({
        cache: 'no-store',
      }),
    );
  });

  it('scopes user search to a workspace when requested', async () => {
    mockJsonResponse([]);

    await expect(
      searchDmUsers('token-1', 'user', {
        includeCurrent: true,
        limit: 10,
        workspaceKey: '기술연구소',
      }),
    ).resolves.toEqual([]);

    expect(fetchMock()).toHaveBeenCalledWith(
      '/v1/dm/users?q=user&limit=10&include_current=true&workspace_key=%EA%B8%B0%EC%88%A0%EC%97%B0%EA%B5%AC%EC%86%8C',
      expect.objectContaining({
        cache: 'no-store',
      }),
    );
  });

  it('normalizes sent messages and keeps the shared route builder in use', async () => {
    mockJsonResponse(message({ id: 'sent-1' }));

    await expect(
      sendDmMessage('token-1', 'c1', '  hello  ', [' a1 '], ' m1 '),
    ).resolves.toEqual({
      ...message({ id: 'sent-1' }),
      attachments: [],
      hidden: false,
    });
    expect(fetchMock()).toHaveBeenCalledWith(
      '/v1/dm/conversations/c1/messages',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          body: 'hello',
          attachment_ids: ['a1'],
          reply_to_message_id: 'm1',
        }),
      }),
    );
  });

  it('normalizes DM request bodies before fetch receives them', async () => {
    mockJsonResponse(conversation({ id: 'direct-1' }));
    await createDirectDmConversation('token-1', ' u1 ');

    mockJsonResponse(conversation({ id: 'group-1' }));
    await createGroupDmConversation('token-1', [' u1 ', 'u2'], '  Team  ');

    mockJsonResponse(conversation({ id: 'members-1' }));
    await addDmConversationParticipants('token-1', 'c1', [' u3 ']);

    expect(fetchMock()).toHaveBeenNthCalledWith(
      1,
      '/v1/dm/conversations',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ recipient_user_id: 'u1' }),
      }),
    );
    expect(fetchMock()).toHaveBeenNthCalledWith(
      2,
      '/v1/dm/conversations',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          participant_user_ids: ['u1', 'u2'],
          title: 'Team',
        }),
      }),
    );
    expect(fetchMock()).toHaveBeenNthCalledWith(
      3,
      '/v1/dm/conversations/c1/participants',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ user_ids: ['u3'] }),
      }),
    );
  });

  it('rejects malformed DM request bodies before fetch', () => {
    expect(() => sendDmMessage('token-1', 'c1', '   ', [])).toThrow(
      'Invalid request format for dm:send-message.',
    );
    expect(() => createDirectDmConversation('token-1', 'u/1')).toThrow(
      'Invalid request format for dm:create-conversation.',
    );

    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('rejects malformed attachment uploads before fetch', () => {
    const oversized = new File(['x'], 'too-large.bin', {
      type: 'application/octet-stream',
    });
    Object.defineProperty(oversized, 'size', {
      configurable: true,
      value: DM_MAX_ATTACHMENT_BYTES + 1,
    });

    expect(() => uploadDmAttachment('token-1', 'c1', oversized)).toThrow(
      `Attachments must be ${DM_MAX_ATTACHMENT_BYTES / (1024 * 1024)}MB or smaller.`,
    );
    expect(() =>
      uploadDmAttachment('token-1', 'conversation/1', new File(['x'], 'x.txt')),
    ).toThrow('DM route id');
    expect(fetchMock()).not.toHaveBeenCalled();
  });
});

describe('miy DM API additions', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    globalThis.fetch = vi.fn() as unknown as typeof globalThis.fetch;
    bindApiRuntime({
      api: async <T,>(path: string, init: RequestInit = {}) =>
        readJson<T>(await fetch(path, init), init),
      fileApi: (path, init) => fetch(path, init),
    });
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it('reserves storage and streams the bytes through the file gateway', async () => {
    const attachment = {
      id: 'a1',
      conversation_id: 'c1',
      message_id: null,
      filename: 'x.png',
      content_type: 'application/octet-stream',
      size_bytes: 3,
      is_image: false,
      download_url: '/dm/attachments/a1/content',
      preview_url: null,
      created_at: '2026-11-22T01:00:00',
    };
    mockJsonResponse(attachment);
    mockJsonResponse({
      ...attachment,
      content_type: 'image/png',
      is_image: true,
      preview_url: '/dm/attachments/a1/content?disposition=inline',
    });
    const file = new File(['abc'], 'x.png', { type: 'image/png' });
    await expect(uploadDmAttachment('token-1', 'c1', file)).resolves.toMatchObject({
      id: 'a1',
      is_image: true,
    });
    const [reserveCall, uploadCall] = fetchMock().mock.calls;
    expect(reserveCall[0]).toBe('/v1/dm/conversations/c1/attachments');
    expect(JSON.parse(reserveCall[1].body)).toMatchObject({
      filename: 'x.png',
      content_type: 'image/png',
      size_bytes: 3,
    });
    expect(uploadCall[0]).toBe('/dm/attachments/a1/content');
    expect(uploadCall[1]).toMatchObject({ method: 'PUT', body: file });
  });

  it('edits, deletes, mutes and cancels through their routes', async () => {
    mockJsonResponse(message({ id: 'm1', body: 'new', edited_at: '2026-11-22T01:00:00' }));
    await expect(editDmMessage('token-1', 'c1', 'm1', 'new')).resolves.toMatchObject({
      edited_at: '2026-11-22T01:00:00',
    });
    mockJsonResponse(message({ id: 'm1', body: '', deleted: true }));
    await expect(deleteDmMessage('token-1', 'c1', 'm1')).resolves.toMatchObject({ deleted: true });
    mockJsonResponse(conversation({ muted_at: '2026-11-22T01:00:00' }));
    await expect(setDmConversationMuted('token-1', 'c1', true)).resolves.toMatchObject({
      muted_at: '2026-11-22T01:00:00',
    });
    fetchMock().mockResolvedValueOnce(new Response(null, { status: 204 }));
    await cancelDmAttachment('token-1', 'a1');
    expect(fetchMock().mock.calls.map(([path, init]) => [path, init?.method])).toEqual([
      ['/v1/dm/conversations/c1/messages/m1', 'PATCH'],
      ['/v1/dm/conversations/c1/messages/m1', 'DELETE'],
      ['/v1/dm/conversations/c1/mute', 'PUT'],
      ['/v1/dm/attachments/a1', 'DELETE'],
    ]);
  });
});

function mockJsonResponse(payload: unknown): void {
  fetchMock().mockResolvedValueOnce(
    new Response(JSON.stringify(payload), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }),
  );
}

function fetchMock(): ReturnType<typeof vi.fn> {
  return globalThis.fetch as unknown as ReturnType<typeof vi.fn>;
}

function conversation(
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    id: 'c1',
    conversation_type: 'direct',
    thread_type: 'direct',
    display_name: 'Direct',
    participants: [],
    participant_count: 2,
    unread_count: 0,
    created_by_id: 'u1',
    created_at: '2026-05-20T00:00:00.000Z',
    updated_at: '2026-05-20T00:00:00.000Z',
    ...overrides,
  };
}

function message(
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    id: 'm1',
    conversation_id: 'c1',
    thread_id: 'c1',
    sequence: 1,
    sender_id: 'u1',
    sender_name: 'User One',
    read_state: {
      unread_count: 0,
      read_by_all: true,
    },
    reply_to: null,
    body: 'hello',
    attachments: [],
    created_at: '2026-05-20T00:00:00.000Z',
    ...overrides,
  };
}


describe('web DM safety API (ADR 0025 개정 2026-11-18)', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    globalThis.fetch = vi.fn() as unknown as typeof globalThis.fetch;
    // miy: requests go through the bound frame runtime; this stand-in keeps
    // AI-DO's fetch expectations (no-store reads of the app API).
    bindApiRuntime({
      api: async <T,>(path: string, init: RequestInit = {}) =>
        readJson<T>(await fetch(path, { ...init, cache: init.cache ?? 'no-store' }), init),
      fileApi: (path, init) => fetch(path, init),
    });
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('reports with a client id and toggles 1:1 blocks with PUT/DELETE', async () => {
    mockJsonResponse({ report_id: 'r1', status: 'received', duplicate: false, blocked: true });
    await expect(
      reportDm('token-1', {
        client_report_id: '6f1c2d3e-4b5a-4c6d-8e7f-0123456789ab',
        conversation_id: 'c1',
        message_id: 'm1',
        category: 'spam',
        note: null,
        block: true,
      }),
    ).resolves.toEqual({ report_id: 'r1', status: 'received', duplicate: false, blocked: true });
    expect(fetchMock()).toHaveBeenLastCalledWith(
      '/v1/dm/reports',
      expect.objectContaining({ method: 'POST' }),
    );

    mockJsonResponse({ user_id: 'u2', blocked: true, changed: true });
    await expect(setDmBlock('token-1', 'u2', true)).resolves.toEqual({
      user_id: 'u2',
      blocked: true,
      changed: true,
    });
    expect(fetchMock()).toHaveBeenLastCalledWith(
      '/v1/dm/blocks/u2',
      expect.objectContaining({ method: 'PUT' }),
    );

    mockJsonResponse({ user_id: 'u2', blocked: false, changed: true });
    await setDmBlock('token-1', 'u2', false);
    expect(fetchMock()).toHaveBeenLastCalledWith(
      '/v1/dm/blocks/u2',
      expect.objectContaining({ method: 'DELETE' }),
    );
  });
});
