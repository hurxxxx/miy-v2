import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { DM_REALTIME_EVENT_TYPES } from '../../../contracts/dm';

import { ApiRequestError } from '../../../platform/api/client';
import { i18n } from '../../../platform/i18n';

import {
  acceptDmTerms,
  createGroupDmThread,
  editDmMessage,
  getDmAttachmentPreviewUrl,
  getDmTerms,
  listDmBlocks,
  listDmMessages,
  listDmThreads,
  markDmThreadRead,
  searchDmUsers,
  sendDmMessage,
  setDmBlock,
  setDmReaction,
  type DmMessage,
  type DmSafetyCapability,
  type DmTermsStatus,
  type DmThread,
  type DmUser,
} from '../api/dm-api';
import { DMEmbeddedView } from './DMView';

const auth = vi.hoisted(() => ({
  token: 'token',
  user: { id: 'me-0', time_zone: 'Asia/Seoul' },
}));

const realtime = vi.hoisted(() => ({
  listeners: new Map<string, Set<(event: unknown) => void>>(),
}));

const feedbackMock = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
}));

vi.mock('@miy/components/ui/lib/feedback/feedback-provider', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@miy/components/ui/lib/feedback/feedback-provider')>()),
  useFeedback: () => feedbackMock,
}));

vi.mock('../../../platform/auth/auth-provider', () => ({
  useAuth: () => auth,
}));

vi.mock('../../../platform/realtime/realtime-provider', async () => {
  const React = await import('react');
  return {
    useRealtime: () => ({ reconnectSeq: 0, status: 'live' }),
    useRealtimeEvent: (type: string, handler: (event: unknown) => void) => {
      const handlerRef = React.useRef(handler);
      handlerRef.current = handler;
      React.useEffect(() => {
        const listener = (event: unknown) => handlerRef.current(event);
        const set = realtime.listeners.get(type) ?? new Set();
        set.add(listener);
        realtime.listeners.set(type, set);
        return () => {
          set.delete(listener);
        };
      }, [type]);
    },
  };
});

vi.mock('../api/dm-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../api/dm-api')>()),
  acceptDmTerms: vi.fn(),
  createGroupDmThread: vi.fn(),
  editDmMessage: vi.fn(),
  getDmAttachmentPreviewUrl: vi.fn(),
  // miy: images are read through runtime.fileApi into object URLs.
  loadDmAttachmentObjectUrl: vi.fn(async (_token: string, path: string) => `blob:${path}`),
  getDmTerms: vi.fn(),
  listDmBlocks: vi.fn(),
  listDmMessages: vi.fn(),
  listDmThreads: vi.fn(),
  markDmThreadRead: vi.fn(),
  searchDmUsers: vi.fn(async () => []),
  sendDmMessage: vi.fn(),
  setDmBlock: vi.fn(),
  setDmReaction: vi.fn(),
}));

const SAFETY: DmSafetyCapability = {
  version: 1,
  report: true,
  block: true,
  report_categories: ['harassment', 'spam', 'other'],
  report_note_max_length: 1000,
  terms: null,
};

const peer: DmUser = {
  id: 'peer',
  email: 'peer@example.com',
  full_name: '상대',
  display_name: null,
  job_title: null,
  primary_org_unit_name: null,
};

let userSeq = 0;

function thread(
  id: string,
  overrides: Partial<DmThread> = {},
): DmThread {
  return {
    id,
    conversation_type: 'direct',
    thread_type: 'direct',
    title: null,
    display_name: `대화 ${id}`,
    other_user: peer,
    participants: [],
    participant_count: 2,
    last_message: null,
    unread_count: 0,
    last_read_message_id: null,
    muted_at: null,
    created_by_id: auth.user.id,
    created_at: '2026-11-18T00:00:00Z',
    updated_at: '2026-11-18T00:00:00Z',
    peer_blocked: false,
    ...overrides,
  };
}

function message(
  id: string,
  conversationId: string,
  overrides: Partial<DmMessage> = {},
): DmMessage {
  return {
    id,
    conversation_id: conversationId,
    thread_id: conversationId,
    sequence: 1,
    sender_id: 'peer',
    sender_name: '상대',
    read_state: { unread_count: 0, read_by_all: true },
    reply_to: null,
    body: `${id} 본문`,
    attachments: [],
    created_at: '2026-11-18T00:00:00Z',
    hidden: false,
    ...overrides,
  };
}

function emit(type: string, data: Record<string, unknown>) {
  act(() => {
    realtime.listeners.get(type)?.forEach((listener) => listener({ type, data }));
  });
}

function view(threadId: string) {
  return <DMEmbeddedView onThreadIdChange={vi.fn()} threadId={threadId} />;
}

describe('DMEmbeddedView composer', () => {
  const own = () => message('original', 'c1', { sender_id: auth.user.id, body: '기존 메시지' });
  const composer = () =>
    screen.getByRole('textbox', { name: i18n.t('apps:dm.messagePlaceholder') }) as HTMLTextAreaElement;

  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
    userSeq += 1;
    auth.user = { id: `me-${userSeq}`, time_zone: 'Asia/Seoul' };
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [thread('c1'), thread('c2')],
      safety: SAFETY,
    });
    vi.mocked(listDmMessages).mockImplementation(async (_token, id) => ({
      items: id === 'c1' ? [own()] : [],
    }));
    vi.mocked(listDmBlocks).mockResolvedValue({ items: [] });
    vi.mocked(markDmThreadRead).mockImplementation(async (_token, id) => thread(id));
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    realtime.listeners.clear();
  });

  it('preserves the next draft when a send fails and ignores Enter during IME composition', async () => {
    let rejectSend!: (error: Error) => void;
    vi.mocked(sendDmMessage).mockImplementation(
      () => new Promise((_resolve, reject) => { rejectSend = reject; }),
    );
    render(view('c1'));
    await screen.findByText(own().body);
    fireEvent.change(composer(), { target: { value: '첫 메시지' } });
    fireEvent.keyDown(composer(), { key: 'Enter', isComposing: true });
    expect(sendDmMessage).not.toHaveBeenCalled();
    fireEvent.keyDown(composer(), { key: 'Enter' });
    expect(sendDmMessage).toHaveBeenCalledTimes(1);
    fireEvent.change(composer(), { target: { value: '다음 초안' } });
    await act(async () => rejectSend(new Error('temporarily unavailable')));
    expect(composer().value).toBe('다음 초안');
    const restore = screen.getByRole('button', { name: i18n.t('apps:dm.restoreFailedDraft') });
    expect((restore as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(composer(), { target: { value: '' } });
    fireEvent.click(restore);
    expect(composer().value).toBe('첫 메시지');
  });

  // miy (§2.2 보강): "@" picker, mention drafts and reactions.
  it('picks a participant with "@" and sends the mention with the body', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [
        thread('c1', {
          participants: [{ user: peer, role: 'member', joined_at: '2026-11-18T00:00:00Z' }],
        }),
      ],
      safety: SAFETY,
    });
    vi.mocked(sendDmMessage).mockImplementation(async (_t, id, body) =>
      message('sent', id, { sender_id: auth.user.id, body }),
    );
    render(view('c1'));
    await screen.findByText(own().body);
    fireEvent.change(composer(), { target: { value: '안녕 @상', selectionStart: 5 } });
    const option = await screen.findByRole('option', { name: '@상대' });
    expect(option).toBeTruthy();
    fireEvent.keyDown(composer(), { key: 'Enter' });
    expect(composer().value).toBe('안녕 @상대 ');
    expect(sendDmMessage).not.toHaveBeenCalled();
    fireEvent.change(composer(), { target: { value: '안녕 @상대 확인해 주세요' } });
    fireEvent.keyDown(composer(), { key: 'Enter' });
    await waitFor(() => expect(sendDmMessage).toHaveBeenCalledTimes(1));
    expect(vi.mocked(sendDmMessage).mock.calls[0][5]).toEqual([
      { userId: 'peer', label: '@상대' },
    ]);
  });

  it('shows reactions and toggles them through the API', async () => {
    const liked = message('m-like', 'c1', {
      reactions: [{ reaction: 'heart', count: 1, reacted: false, users: [peer] }],
    });
    vi.mocked(listDmMessages).mockImplementation(async (_token, id) => ({
      items: id === 'c1' ? [liked] : [],
    }));
    vi.mocked(setDmReaction).mockImplementation(async () => ({
      ...liked,
      reactions: [{ reaction: 'heart', count: 2, reacted: true, users: [peer, peer] }],
    }));
    render(view('c1'));
    const chip = await screen.findByRole('button', { name: `${i18n.t('apps:dm.reactions.heart')} 1` });
    expect(chip.getAttribute('title')).toBe(peer.full_name);
    fireEvent.click(chip);
    await waitFor(() =>
      expect(setDmReaction).toHaveBeenCalledWith('token', 'c1', 'm-like', 'heart', true),
    );
    expect(
      await screen.findByRole('button', { name: `${i18n.t('apps:dm.reactions.heart')} 2` }),
    ).toBeTruthy();
  });

  it('loads older pages until a focused message is there', async () => {
    const newest = message('m-new', 'c1', { sequence: 200, body: '최근 메시지' });
    const target = message('m-old', 'c1', { sequence: 3, body: '찾던 메시지' });
    vi.mocked(listDmMessages).mockImplementation(async (_token, id, _limit, before) => ({
      items: id !== 'c1' ? [] : before ? [target] : [newest],
    }));
    render(
      <DMEmbeddedView focusMessageId="m-old" onThreadIdChange={vi.fn()} threadId="c1" />,
    );
    expect(await screen.findByText('찾던 메시지')).toBeTruthy();
    expect(
      vi.mocked(listDmMessages).mock.calls.some(
        ([, id, limit, before]) => id === 'c1' && limit === 100 && before === 200,
      ),
    ).toBe(true);
  });

  // miy (M4): live hints only trigger refetches through the API.
  it('refetches the list and the shown conversation on a live hint', async () => {
    render(view('c1'));
    await screen.findByText(own().body);
    const messageCalls = () =>
      vi.mocked(listDmMessages).mock.calls.filter(([, id]) => id === 'c1').length;
    const listCalls = () => vi.mocked(listDmThreads).mock.calls.length;
    const beforeMessages = messageCalls();
    const beforeList = listCalls();
    emit('conversation.changed', { type: 'conversation.changed', conversationId: 'c1' });
    emit('conversation.changed', { type: 'conversation.changed', conversationId: 'c1' });
    await waitFor(() => expect(messageCalls()).toBe(beforeMessages + 1));
    expect(listCalls()).toBe(beforeList + 1);
    emit('conversation.changed', { type: 'conversation.changed', conversationId: 'c2' });
    await waitFor(() => expect(listCalls()).toBe(beforeList + 2));
    expect(vi.mocked(listDmMessages).mock.calls.some(([, id]) => id === 'c2')).toBe(false);
  });

  it('shows who is typing in the shown conversation until it ends', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [
        thread('c1', {
          participants: [
            { user: peer, role: 'member', joined_at: '2026-11-18T00:00:00Z' },
          ],
        }),
      ],
      safety: SAFETY,
    });
    render(view('c1'));
    await screen.findByText(own().body);
    const label = i18n.t('apps:dm.typingOne', { name: peer.full_name });
    emit('typing', { type: 'typing', conversationId: 'c2', userId: peer.id, active: true });
    expect(screen.queryByText(label)).toBeNull();
    emit('typing', { type: 'typing', conversationId: 'c1', userId: peer.id, active: true });
    expect(await screen.findByText(label)).toBeTruthy();
    emit('typing', { type: 'typing', conversationId: 'c1', userId: peer.id, active: false });
    await waitFor(() => expect(screen.queryByText(label)).toBeNull());
    emit('typing', { type: 'typing', conversationId: 'c1', userId: auth.user.id, active: true });
    expect(screen.queryByText(/입력 중/)).toBeNull();
  });

  it('keeps an ordinary draft when the user opens and leaves an edit', async () => {
    const { rerender } = render(view('c1'));
    await screen.findByText(own().body);
    fireEvent.change(composer(), { target: { value: '보낼 초안' } });
    fireEvent.click(screen.getByRole('button', { name: i18n.t('apps:dm.edit') }));
    expect(composer().value).toBe(own().body);
    fireEvent.change(composer(), { target: { value: '수정 중' } });
    rerender(view('c2'));
    await waitFor(() => expect(composer().value).toBe(''));
    rerender(view('c1'));
    await waitFor(() => expect(composer().value).toBe('보낼 초안'));
  });

  it('does not overwrite the current composer when an edit in another thread succeeds', async () => {
    let finishEdit!: (value: DmMessage) => void;
    vi.mocked(editDmMessage).mockImplementation(
      () => new Promise((resolve) => { finishEdit = resolve; }),
    );
    const { rerender } = render(view('c1'));
    await screen.findByText(own().body);
    fireEvent.change(composer(), { target: { value: '첫 대화 초안' } });
    fireEvent.click(screen.getByRole('button', { name: i18n.t('apps:dm.edit') }));
    fireEvent.change(composer(), { target: { value: '고친 메시지' } });
    fireEvent.keyDown(composer(), { key: 'Enter' });
    expect(editDmMessage).toHaveBeenCalledWith('token', 'c1', 'original', '고친 메시지', []);
    rerender(view('c2'));
    await waitFor(() => expect(composer().value).toBe(''));
    fireEvent.change(composer(), { target: { value: '다른 대화 초안' } });
    await act(async () => finishEdit({ ...own(), body: '고친 메시지' }));
    expect(composer().value).toBe('다른 대화 초안');
    rerender(view('c1'));
    await waitFor(() => expect(composer().value).toBe('첫 대화 초안'));
  });

  it('preserves back history until the host accepts the requested navigation', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [
        thread('c1', { other_user: { ...peer, id: 'one', full_name: '첫 사람' } }),
        thread('c2', { other_user: { ...peer, id: 'two', full_name: '둘째 사람' } }),
      ],
      safety: SAFETY,
    });
    const requestNavigation = vi.fn();
    const at = (threadId: string) => (
      <DMEmbeddedView embeddedPersistentList onThreadIdChange={requestNavigation} threadId={threadId} />
    );
    const { rerender } = render(at('c1'));
    const nextThread = await screen.findByText('둘째 사람');
    fireEvent.click(nextThread);
    expect(requestNavigation).toHaveBeenLastCalledWith('c2');
    rerender(at('c2'));
    const back = await screen.findByRole('button', { name: i18n.t('apps:dm.previousThread') });
    fireEvent.click(back);
    expect(requestNavigation).toHaveBeenLastCalledWith('c1');
    expect((back as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(back);
    expect(requestNavigation).toHaveBeenCalledTimes(3);
    rerender(at('c1'));
    await waitFor(() => expect((screen.getByRole('button', {
      name: i18n.t('apps:dm.nextThread'),
    }) as HTMLButtonElement).disabled).toBe(false));
  });

  it('refreshes already loaded messages after they leave the newest page', async () => {
    const older = message('oldest', 'c1', { sequence: 1, body: '이전 메시지 본문' });
    const newest = Array.from({ length: 50 }, (_, index) =>
      message(`newest-${index}`, 'c1', { sequence: index + 2 }),
    );
    vi.mocked(listDmMessages).mockImplementation(async (_token, _id, _limit, before, ids) => ({
      items: ids ? [{ ...older, hidden: true, body: '관리자가 가린 메시지입니다.' }]
        : before ? [older] : newest,
    }));
    const at = (reloadSeq: number) => (
      <DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={reloadSeq} threadId="c1" />
    );
    const { rerender } = render(at(0));
    fireEvent.click(await screen.findByRole('button', { name: i18n.t('apps:dm.loadOlder') }));
    await screen.findByText(older.body);
    rerender(at(1));
    await waitFor(() => expect(screen.queryByText(older.body)).toBeNull());
    expect(listDmMessages).toHaveBeenCalledWith('token', 'c1', 50, null, ['oldest']);
  });
});

describe('DMEmbeddedView safety', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
    // 모듈 캐시는 사용자 범위로 나뉜다 — 시험마다 다른 사용자로 서로 섞이지 않게 한다.
    userSeq += 1;
    auth.user = { id: `me-${userSeq}`, time_zone: 'Asia/Seoul' };
    vi.mocked(listDmBlocks).mockResolvedValue({ items: [] });
    vi.mocked(markDmThreadRead).mockImplementation(async (_token, id) => thread(id));
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    realtime.listeners.clear();
  });

  it('keeps report and block actions when another view reuses the cached thread list', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({ items: [thread('c1')], safety: SAFETY });
    vi.mocked(listDmMessages).mockResolvedValue({ items: [message('m1', 'c1')] });

    const first = render(view('c1'));
    await within(first.container).findByRole('button', { name: '1:1 차단' });

    const second = render(view('c1'));
    expect(
      await within(second.container).findByRole('button', { name: '1:1 차단' }),
    ).toBeTruthy();
    expect(within(second.container).getAllByRole('button', { name: '신고' })).toHaveLength(1);
    expect(listDmThreads).toHaveBeenCalledTimes(1);

    first.unmount();
    second.unmount();
    const reopened = render(view('c1'));
    expect(
      await within(reopened.container).findByRole('button', { name: '1:1 차단' }),
    ).toBeTruthy();
  });

  it('applies a hide to the cached list of a conversation that is not open, quotes included', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [thread('c1'), thread('c2')],
      safety: SAFETY,
    });
    const original = message('m1', 'c1', { body: '원문 비밀' });
    const reply = message('m2', 'c1', {
      sequence: 2,
      sender_id: auth.user.id,
      sender_name: '나',
      body: '답장',
      created_at: '2026-11-18T00:01:00Z',
      reply_to: {
        id: 'm1',
        sender_id: 'peer',
        sender_name: '상대',
        body_preview: '원문 비밀',
        attachment_count: 0,
        created_at: '2026-11-18T00:00:00Z',
        hidden: false,
      },
    });
    vi.mocked(listDmMessages).mockImplementation(async (_token, id) => ({
      items: id === 'c1' ? [original, reply] : [message('x1', 'c2', { body: '다른 대화' })],
    }));

    const rendered = render(view('c1'));
    expect(await screen.findAllByText('원문 비밀')).toHaveLength(2);
    rendered.rerender(view('c2'));
    await screen.findByText('다른 대화');

    emit(DM_REALTIME_EVENT_TYPES.messageUpdated, {
      conversation: thread('c1'),
      conversation_id: 'c1',
      message: { ...original, body: '관리자가 가린 메시지입니다.', hidden: true },
    });
    rendered.rerender(view('c1'));

    expect(await screen.findAllByText('관리자가 가린 메시지입니다.')).toHaveLength(2);
    expect(screen.queryByText('원문 비밀')).toBeNull();
    expect(vi.mocked(listDmMessages).mock.calls.map(([, id]) => id)).toEqual(['c1', 'c2']);
  });

  it('replaces the reply preview when the message being answered is hidden', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({ items: [thread('c1')], safety: SAFETY });
    const original = message('m1', 'c1', { body: '답장할 원문' });
    vi.mocked(listDmMessages).mockResolvedValue({ items: [original] });

    render(view('c1'));
    await screen.findByText('답장할 원문');
    fireEvent.click(screen.getByRole('button', { name: '답변' }));
    expect(await screen.findAllByText('답장할 원문')).toHaveLength(2);

    emit(DM_REALTIME_EVENT_TYPES.messageUpdated, {
      conversation: thread('c1'),
      conversation_id: 'c1',
      message: { ...original, body: '관리자가 가린 메시지입니다.', hidden: true },
    });

    expect(await screen.findAllByText('관리자가 가린 메시지입니다.')).toHaveLength(2);
    expect(screen.queryByText('답장할 원문')).toBeNull();
  });

  it('lets a person unblocked on the web even without a 1:1 conversation', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [
        thread('g1', {
          conversation_type: 'group',
          thread_type: 'group',
          title: '단체',
          display_name: '단체',
          other_user: null,
          participant_count: 3,
        }),
      ],
      safety: SAFETY,
    });
    vi.mocked(listDmBlocks).mockResolvedValue({
      items: [{ user: peer, blocked_at: '2026-11-18T00:00:00Z' }],
    });
    vi.mocked(setDmBlock).mockResolvedValue({
      user_id: 'peer',
      blocked: false,
      changed: true,
    });

    render(view(''));
    fireEvent.click(await screen.findByRole('button', { name: '1:1 차단 목록' }));
    fireEvent.click(
      await screen.findByRole('button', { name: '상대 님 1:1 차단 해제' }),
    );

    await waitFor(() =>
      expect(setDmBlock).toHaveBeenCalledWith('token', 'peer', false),
    );
    expect(await screen.findByText('1:1 차단한 사람이 없습니다.')).toBeTruthy();
    expect(feedbackMock.success).toHaveBeenCalledWith('1:1 차단을 풀었습니다.');
  });
});

const tr = (key: string, options: Record<string, unknown> = {}): string =>
  String((i18n.t as (k: string, o: Record<string, unknown>) => unknown)(key, { ns: 'apps', ...options }));

function terms(overrides: Partial<DmTermsStatus> = {}): DmTermsStatus {
  return {
    version: '2026-11-18',
    required: true,
    accepted: false,
    accepted_at: null,
    document_url: 'https://example.test/ugc-terms.html',
    contact: null,
    ...overrides,
  };
}

const memberA: DmUser = { ...peer, id: 'member-a', email: 'a@example.com', full_name: '가람' };
const memberB: DmUser = { ...peer, id: 'member-b', email: 'b@example.com', full_name: '나래' };

/** 대화가 하나도 없는 사람이 단체 대화 만들기 화면에서 두 사람을 고르고 이름을 적는다. */
async function fillGroupCreate(title: string) {
  vi.mocked(searchDmUsers).mockResolvedValue([memberA, memberB]);
  fireEvent.change(screen.getByRole('textbox', { name: tr('dm.searchPlaceholder') }), {
    target: { value: '가' },
  });
  const [rowA] = await screen.findAllByRole('button', { name: /가람/ });
  const rowB = screen.getByRole('button', { name: /나래/ });
  fireEvent.click(rowA);
  fireEvent.click(rowB);
  const name = await screen.findByRole('textbox', { name: tr('dm.groupNamePlaceholder') });
  fireEvent.change(name, { target: { value: title } });
  return name as HTMLInputElement;
}

const createGroupButton = () => screen.getByRole('button', { name: tr('dm.createGroup') });
const termsNotice = () => screen.queryByText(tr('dm.safety.termsRequired'));

describe('DMEmbeddedView group create with content rules', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
    userSeq += 1;
    auth.user = { id: 'me-' + userSeq, time_zone: 'Asia/Seoul' };
    vi.mocked(listDmBlocks).mockResolvedValue({ items: [] });
    vi.mocked(listDmMessages).mockResolvedValue({ items: [] });
    vi.mocked(markDmThreadRead).mockImplementation(async (_token, id) => thread(id));
    vi.mocked(createGroupDmThread).mockImplementation(async (_token, ids, title) =>
      thread('g1', { conversation_type: 'group', thread_type: 'group', title: title || null, participant_count: ids.length + 1 }),
    );
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    realtime.listeners.clear();
  });

  it('lets a person without any conversation accept the rules while creating a named group, keeping the name and members through a refused create and a failed accept', async () => {
    let serverAccepted = false;
    vi.mocked(listDmThreads).mockResolvedValue({ items: [], safety: { ...SAFETY, terms: terms() } });
    vi.mocked(getDmTerms).mockImplementation(async () =>
      serverAccepted ? terms({ accepted: true, accepted_at: '2026-11-19T00:00:00Z' }) : terms(),
    );
    vi.mocked(createGroupDmThread).mockImplementation(async (_token, ids, title) => {
      if (!serverAccepted && title.trim()) {
        throw new ApiRequestError(403, '이용규칙 동의 필요', { code: 'ugc_terms_acceptance_required' });
      }
      return thread('g1', { conversation_type: 'group', thread_type: 'group', title, participant_count: ids.length + 1 });
    });
    render(view(''));
    const name = await fillGroupCreate('설비 점검방');

    expect(termsNotice()).toBeTruthy();
    expect(screen.getByRole('link', { name: tr('dm.safety.termsDocument') }).getAttribute('href')).toBe(
      'https://example.test/ugc-terms.html',
    );

    // 동의 전에 만들면 서버가 거절한다 — 입력은 그대로, 안내도 그대로.
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(feedbackMock.error).toHaveBeenCalledWith('이용규칙 동의 필요'));
    expect(termsNotice()).toBeTruthy();
    expect(name.value).toBe('설비 점검방');

    vi.mocked(acceptDmTerms).mockRejectedValueOnce(new Error('동의 저장 실패'));
    fireEvent.click(screen.getByRole('button', { name: tr('dm.safety.termsAccept') }));
    await waitFor(() => expect(feedbackMock.error).toHaveBeenCalledWith('동의 저장 실패'));
    expect(termsNotice()).toBeTruthy();
    expect(name.value).toBe('설비 점검방');

    vi.mocked(acceptDmTerms).mockImplementationOnce(async () => {
      serverAccepted = true;
      return { version: '2026-11-18', accepted_at: '2026-11-19T00:00:00Z', changed: true };
    });
    fireEvent.click(screen.getByRole('button', { name: tr('dm.safety.termsAccept') }));
    await waitFor(() => expect(termsNotice()).toBeNull());
    expect(acceptDmTerms).toHaveBeenLastCalledWith('token', '2026-11-18');
    expect(feedbackMock.success).toHaveBeenCalledWith(tr('dm.safety.termsAccepted'));
    expect(name.value).toBe('설비 점검방');

    fireEvent.click(createGroupButton());
    await waitFor(() =>
      expect(screen.queryByRole('textbox', { name: tr('dm.groupNamePlaceholder') })).toBeNull(),
    );
    expect(createGroupDmThread).toHaveBeenCalledTimes(2);
    expect(vi.mocked(createGroupDmThread).mock.calls.at(-1)).toEqual([
      'token',
      ['member-a', 'member-b'],
      '설비 점검방',
    ]);
  });

  it('keeps an accept made while an older pending rules answer was in flight, yet still takes a newer rules version', async () => {
    // 리뷰 8029: 거절된 만들기가 시작한 이용규칙 받기가 동의 뒤에 늦게 도착해도 동의를 되돌려 보내기를 다시 막지 않는다.
    let resolveStale: (value: DmTermsStatus) => void = () => undefined;
    let serverVersion = '2026-11-18';
    vi.mocked(listDmThreads).mockResolvedValue({ items: [thread('c1')], safety: { ...SAFETY, terms: terms() } });
    vi.mocked(getDmTerms)
      .mockImplementationOnce(
        () =>
          new Promise<DmTermsStatus>((resolve) => {
            resolveStale = resolve;
          }),
      )
      .mockImplementation(async () => terms({ version: serverVersion }));
    vi.mocked(createGroupDmThread).mockImplementation(async () => {
      throw new ApiRequestError(403, '이용규칙 동의 필요', { code: 'ugc_terms_acceptance_required' });
    });
    vi.mocked(acceptDmTerms).mockImplementation(async (_token, version) => ({
      version,
      accepted_at: '2026-11-19T00:00:00Z',
      changed: true,
    }));
    render(view('c1'));
    const composer = await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    fireEvent.change(composer, { target: { value: '점검 시작합니다' } });
    const sendButton = () => screen.getByRole('button', { name: tr('dm.send') }) as HTMLButtonElement;
    const notices = () => screen.queryAllByText(tr('dm.safety.termsRequired')).length;
    expect(sendButton().disabled).toBe(true);
    const name = await fillGroupCreate('설비 점검방');

    // 거절된 만들기 → 이용규칙 받기가 시작됐지만 답(동의 전 값)은 아직 오지 않았다.
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(feedbackMock.error).toHaveBeenCalledWith('이용규칙 동의 필요'));
    expect(getDmTerms).toHaveBeenCalledTimes(1);

    // 그사이 동의한다 — 안내가 사라지고 보내기가 풀린다.
    fireEvent.click(screen.getAllByRole('button', { name: tr('dm.safety.termsAccept') })[0]);
    await waitFor(() => expect(notices()).toBe(0));
    expect(sendButton().disabled).toBe(false);

    // 늦게 온 동의 전 답은 버린다 — 보내기는 풀린 그대로, 입력도 그대로.
    await act(async () => {
      resolveStale(terms());
    });
    expect(notices()).toBe(0);
    expect(sendButton().disabled).toBe(false);
    expect(name.value).toBe('설비 점검방');
    expect(composer).toHaveProperty('value', '점검 시작합니다');

    // 동의 뒤에 시작한 받기는 반영한다 — 서버가 새 판 이용규칙을 요구하면 다시 안내하고 보내기를 막는다.
    serverVersion = '2026-12-01';
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(notices()).toBeGreaterThan(0));
    expect(sendButton().disabled).toBe(true);
    expect(getDmTerms).toHaveBeenCalledTimes(2);
  });

  it('still shows a newer rules version when a list reload that started meanwhile fails', async () => {
    // 리뷰 8040: 이용규칙 받기가 진행 중일 때 시작한 목록 다시 받기가 실패해도, 그 받기 결과(새 판 미동의)는 반영한다.
    let resolveTerms: (value: DmTermsStatus) => void = () => undefined;
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [thread('c1')],
      safety: { ...SAFETY, terms: terms({ accepted: true, accepted_at: '2026-11-18T00:00:00Z' }) },
    });
    vi.mocked(getDmTerms).mockImplementationOnce(
      () =>
        new Promise<DmTermsStatus>((resolve) => {
          resolveTerms = resolve;
        }),
    );
    vi.mocked(createGroupDmThread).mockImplementation(async () => {
      throw new ApiRequestError(403, '이용규칙 동의 필요', { code: 'ugc_terms_acceptance_required' });
    });
    const { rerender } = render(view('c1'));
    const composer = await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    fireEvent.change(composer, { target: { value: '점검 시작합니다' } });
    const sendButton = () => screen.getByRole('button', { name: tr('dm.send') }) as HTMLButtonElement;
    const notices = () => screen.queryAllByText(tr('dm.safety.termsRequired')).length;
    expect(sendButton().disabled).toBe(false);
    await fillGroupCreate('설비 점검방');

    // 서버가 새 판 이용규칙 동의를 요구한다 — 이용규칙 받기가 시작됐지만 답은 아직이다.
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(getDmTerms).toHaveBeenCalledTimes(1));

    // 그사이 목록 다시 받기가 시작돼 실패한다.
    const listCalls = vi.mocked(listDmThreads).mock.calls.length;
    vi.mocked(listDmThreads).mockRejectedValueOnce(new Error('목록 실패'));
    rerender(<DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={1} threadId="c1" />);
    await waitFor(() => expect(vi.mocked(listDmThreads).mock.calls.length).toBeGreaterThan(listCalls));

    // 이용규칙 답(새 판, 미동의)은 반영한다 — 안내가 보이고 보내기는 막힌다.
    await act(async () => {
      resolveTerms(terms({ version: '2026-12-01' }));
    });
    await waitFor(() => expect(notices()).toBeGreaterThan(0));
    expect(sendButton().disabled).toBe(true);
  });

  it('takes a confirmed accept from an earlier list reload even after a later pending rules answer', async () => {
    // 리뷰 8058: 먼저 시작한 목록 다시 받기가, 더 나중에 시작해 먼저 반영된 미동의 답 뒤에 '동의함'을 가져오면 그 동의를 받아들인다.
    let resolveList: (value: Awaited<ReturnType<typeof listDmThreads>>) => void = () => undefined;
    vi.mocked(listDmThreads).mockResolvedValueOnce({ items: [thread('c1')], safety: { ...SAFETY, terms: terms() } });
    vi.mocked(getDmTerms).mockResolvedValue(terms());
    const { rerender } = render(view('c1'));
    const composer = await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    fireEvent.change(composer, { target: { value: '점검 시작합니다' } });
    const sendButton = () => screen.getByRole('button', { name: tr('dm.send') }) as HTMLButtonElement;
    const notices = () => screen.queryAllByText(tr('dm.safety.termsRequired')).length;
    expect(sendButton().disabled).toBe(true);
    await fillGroupCreate('');

    // 목록 다시 받기가 시작됐지만 답은 아직이다.
    vi.mocked(listDmThreads).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveList = resolve;
        }),
    );
    rerender(<DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={1} threadId="c1" />);
    await waitFor(() => expect(listDmThreads).toHaveBeenCalledTimes(2));

    // 이름 없는 단체 대화는 만들어지고, 그 뒤 시작한 이용규칙 받기가 먼저 '미동의'로 반영된다.
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(getDmTerms).toHaveBeenCalledTimes(1));
    await act(async () => undefined);
    expect(notices()).toBeGreaterThan(0);

    // 그사이 다른 기기에서 동의했다 — 먼저 시작한 목록 답이 같은 판의 동의를 가져온다.
    await act(async () => {
      resolveList({
        items: [thread('c1')],
        safety: { ...SAFETY, terms: terms({ accepted: true, accepted_at: '2026-11-19T01:00:00Z' }) },
      });
    });
    await waitFor(() => expect(notices()).toBe(0));
    expect(sendButton().disabled).toBe(false);
  });

  it('does not let an earlier accept of an older rules version hide a newer version that still needs consent', async () => {
    // 리뷰 8058 보완의 경계: 늦게 온 동의 확인은 같은 판일 때만 — 다른(이전) 판의 동의는 새 판 안내를 가리지 않는다.
    let resolveList: (value: Awaited<ReturnType<typeof listDmThreads>>) => void = () => undefined;
    vi.mocked(listDmThreads).mockResolvedValueOnce({
      items: [thread('c1')],
      safety: { ...SAFETY, terms: terms({ version: '2026-12-01' }) },
    });
    vi.mocked(getDmTerms).mockResolvedValue(terms({ version: '2026-12-01' }));
    const { rerender } = render(view('c1'));
    const composer = await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    fireEvent.change(composer, { target: { value: '점검 시작합니다' } });
    const sendButton = () => screen.getByRole('button', { name: tr('dm.send') }) as HTMLButtonElement;
    const notices = () => screen.queryAllByText(tr('dm.safety.termsRequired')).length;
    await fillGroupCreate('');
    vi.mocked(listDmThreads).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveList = resolve;
        }),
    );
    rerender(<DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={1} threadId="c1" />);
    await waitFor(() => expect(listDmThreads).toHaveBeenCalledTimes(2));
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(getDmTerms).toHaveBeenCalledTimes(1));
    await act(async () => undefined);
    await act(async () => {
      resolveList({
        items: [thread('c1')],
        safety: { ...SAFETY, terms: terms({ accepted: true, accepted_at: '2026-11-19T01:00:00Z' }) },
      });
    });
    expect(notices()).toBeGreaterThan(0);
    expect(sendButton().disabled).toBe(true);
  });

  it('never applies a rules answer requested before switching to another account', async () => {
    // 리뷰 8067: 계정 A 의 이용규칙 받기가 진행 중일 때 계정 B 로 바뀌면, 늦게 온 A 의 '동의함' 은 B 의 미동의를 가리지 않는다.
    let resolveTerms: (value: DmTermsStatus) => void = () => undefined;
    vi.mocked(listDmThreads).mockResolvedValue({ items: [thread('c1')], safety: { ...SAFETY, terms: terms() } });
    vi.mocked(getDmTerms).mockImplementationOnce(
      () =>
        new Promise<DmTermsStatus>((resolve) => {
          resolveTerms = resolve;
        }),
    );
    vi.mocked(createGroupDmThread).mockImplementation(async () => {
      throw new ApiRequestError(403, '이용규칙 동의 필요', { code: 'ugc_terms_acceptance_required' });
    });
    const { rerender } = render(view('c1'));
    await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    await fillGroupCreate('설비 점검방');
    fireEvent.click(createGroupButton());
    await waitFor(() => expect(getDmTerms).toHaveBeenCalledTimes(1));

    // 다른 계정으로 바뀐다 — 그 계정의 목록은 같은 판 미동의다.
    const listCalls = vi.mocked(listDmThreads).mock.calls.length;
    auth.user = { id: 'switched-' + userSeq, time_zone: 'Asia/Seoul' };
    rerender(view('c1'));
    await waitFor(() => expect(vi.mocked(listDmThreads).mock.calls.length).toBeGreaterThan(listCalls));
    const composer = await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    fireEvent.change(composer, { target: { value: '점검 시작합니다' } });
    const notices = () => screen.queryAllByText(tr('dm.safety.termsRequired')).length;
    await waitFor(() => expect(notices()).toBeGreaterThan(0));

    // 이전 계정의 늦은 '동의함' 답은 반영하지 않는다.
    await act(async () => {
      resolveTerms(terms({ accepted: true, accepted_at: '2026-11-19T01:00:00Z' }));
    });
    expect(notices()).toBeGreaterThan(0);
    expect((screen.getByRole('button', { name: tr('dm.send') }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('keeps a list answer requested by one account out of another account and its cache', async () => {
    // 리뷰 8070: 동의한 A 의 목록 요청 중 미동의 B 로 바뀌고 B 목록 뒤에 A 목록이 오면, 그 답은 반영·캐시하지 않는다 —
    // 곧 A 로 돌아와도 A 는 동의한 그대로다.
    const accountA = { id: 'account-a-' + userSeq, time_zone: 'Asia/Seoul' };
    const accountB = { id: 'account-b-' + userSeq, time_zone: 'Asia/Seoul' };
    const accepted = { ...SAFETY, terms: terms({ accepted: true, accepted_at: '2026-11-19T00:00:00Z' }) };
    let resolveListA: (value: Awaited<ReturnType<typeof listDmThreads>>) => void = () => undefined;
    auth.user = accountA;
    vi.mocked(listDmThreads)
      .mockResolvedValueOnce({ items: [thread('c1')], safety: accepted })
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveListA = resolve;
          }),
      )
      .mockResolvedValueOnce({ items: [thread('c1')], safety: { ...SAFETY, terms: terms() } })
      .mockResolvedValue({ items: [thread('c1')], safety: accepted });
    const notices = () => screen.queryAllByText(tr('dm.safety.termsRequired')).length;
    const { rerender } = render(view('c1'));
    await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    expect(notices()).toBe(0);

    // A 의 목록 다시 받기가 진행 중일 때 B 로 바뀐다 — B 의 목록(미동의)이 먼저 온다.
    rerender(<DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={1} threadId="c1" />);
    await waitFor(() => expect(listDmThreads).toHaveBeenCalledTimes(2));
    auth.user = accountB;
    rerender(<DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={1} threadId="c1" />);
    await waitFor(() => expect(notices()).toBeGreaterThan(0));

    // 늦게 온 A 의 목록은 B 화면에도, A 의 캐시에도 반영되지 않는다.
    await act(async () => {
      resolveListA({ items: [thread('c1')], safety: accepted });
    });
    expect(notices()).toBeGreaterThan(0);

    // 곧바로 A 로 돌아오면 A 는 동의한 그대로 — 안내 없음, 보내기 가능.
    auth.user = accountA;
    rerender(<DMEmbeddedView onThreadIdChange={vi.fn()} reloadSeq={1} threadId="c1" />);
    const composer = await screen.findByPlaceholderText(tr('dm.messagePlaceholder'));
    fireEvent.change(composer, { target: { value: '점검 시작합니다' } });
    await waitFor(() => expect(notices()).toBe(0));
    expect((screen.getByRole('button', { name: tr('dm.send') }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('creates the named group when the rules were accepted on another device while this screen stayed open', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({ items: [], safety: { ...SAFETY, terms: terms() } });
    vi.mocked(getDmTerms).mockResolvedValue(terms({ accepted: true, accepted_at: '2026-11-19T00:00:00Z' }));
    render(view(''));
    await fillGroupCreate('설비 점검방');
    expect(termsNotice()).toBeTruthy();
    expect((createGroupButton() as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(createGroupButton());
    await waitFor(() =>
      expect(createGroupDmThread).toHaveBeenCalledWith('token', ['member-a', 'member-b'], '설비 점검방'),
    );
    // 서버가 받아들였으니 이 화면의 동의 상태도 새로 받는다(다른 기기에서 동의).
    await waitFor(() => expect(getDmTerms).toHaveBeenCalledTimes(1));
    expect(acceptDmTerms).not.toHaveBeenCalled();
    expect(feedbackMock.error).not.toHaveBeenCalled();
  });

  it('shows the rules but still allows an unnamed group, which the server does not gate', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({ items: [], safety: { ...SAFETY, terms: terms() } });
    render(view(''));
    await fillGroupCreate('   ');

    expect(termsNotice()).toBeTruthy();
    fireEvent.click(createGroupButton());
    await waitFor(() =>
      expect(createGroupDmThread).toHaveBeenCalledWith('token', ['member-a', 'member-b'], '   '),
    );
    expect(acceptDmTerms).not.toHaveBeenCalled();
  });

  it.each([
    ['already accepted', terms({ accepted: true, accepted_at: '2026-11-18T00:00:00Z' })],
    ['not required', terms({ required: false })],
    ['no rules from the server', null],
  ])('creates a named group directly when the rules are %s', async (_label, current) => {
    vi.mocked(listDmThreads).mockResolvedValue({ items: [], safety: { ...SAFETY, terms: current } });
    render(view(''));
    await fillGroupCreate('설비 점검방');

    expect(termsNotice()).toBeNull();
    fireEvent.click(createGroupButton());
    await waitFor(() =>
      expect(createGroupDmThread).toHaveBeenCalledWith('token', ['member-a', 'member-b'], '설비 점검방'),
    );
    expect(acceptDmTerms).not.toHaveBeenCalled();
  });

  it('brings the rules back into the create screen when the server refuses the group name', async () => {
    vi.mocked(listDmThreads).mockResolvedValue({
      items: [],
      safety: { ...SAFETY, terms: terms({ accepted: true, accepted_at: '2026-11-18T00:00:00Z' }) },
    });
    vi.mocked(getDmTerms).mockResolvedValue(terms({ version: '2026-11-19' }));
    vi.mocked(createGroupDmThread).mockRejectedValueOnce(
      new ApiRequestError(403, '이용규칙 동의 필요', { code: 'ugc_terms_acceptance_required' }),
    );
    render(view(''));
    const name = await fillGroupCreate('설비 점검방');

    fireEvent.click(createGroupButton());
    await waitFor(() => expect(feedbackMock.error).toHaveBeenCalledWith('이용규칙 동의 필요'));
    await waitFor(() => expect(termsNotice()).toBeTruthy());
    expect(getDmTerms).toHaveBeenCalledTimes(1);
    expect(name.value).toBe('설비 점검방');
    expect((createGroupButton() as HTMLButtonElement).disabled).toBe(false);
  });
});

describe('DMEmbeddedView image viewer and moderation hide', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('ko-KR');
    userSeq += 1;
    auth.user = { id: 'me-' + userSeq, time_zone: 'Asia/Seoul' };
    vi.mocked(listDmBlocks).mockResolvedValue({ items: [] });
    vi.mocked(markDmThreadRead).mockImplementation(async (_token, id) => thread(id));
    vi.mocked(getDmAttachmentPreviewUrl).mockImplementation(async (_token, id) => ({
      url: '/dm/attachments/' + id + '/content?disposition=inline',
    }));
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    realtime.listeners.clear();
  });

  function imageMessage(id: string, attachmentId: string, filename: string, sequence: number) {
    return message(id, 'c1', {
      sequence,
      body: '',
      attachments: [
        {
          id: attachmentId,
          conversation_id: 'c1',
          message_id: id,
          filename,
          content_type: 'image/png',
          size_bytes: 10,
          is_image: true,
          download_url: '/dm/attachments/' + attachmentId + '/content',
          preview_url: '/dm/attachments/' + attachmentId + '/content?disposition=inline',
          created_at: '2026-11-18T00:00:00Z',
        },
      ],
    });
  }

  const hide = (original: DmMessage) =>
    emit(DM_REALTIME_EVENT_TYPES.messageUpdated, {
      conversation: thread('c1'),
      conversation_id: 'c1',
      message: { ...original, body: '관리자가 가린 메시지입니다.', attachments: [], hidden: true },
    });

  it('closes the open viewer only when its own message is hidden', async () => {
    const first = imageMessage('m1', 'att-1', '현장사진.png', 1);
    const second = imageMessage('m2', 'att-2', '다른사진.png', 2);
    vi.mocked(listDmThreads).mockResolvedValue({ items: [thread('c1')], safety: SAFETY });
    vi.mocked(listDmMessages).mockResolvedValue({ items: [first, second] });
    render(view('c1'));

    fireEvent.click(
      await screen.findByRole('button', { name: tr('dm.openImageAttachment', { filename: '현장사진.png' }) }),
    );
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('img', { name: '현장사진.png' })).toBeTruthy();
    expect(getDmAttachmentPreviewUrl).toHaveBeenCalledWith('token', 'att-1');

    hide(second);
    await waitFor(() => expect(screen.queryAllByAltText('다른사진.png')).toHaveLength(0));
    expect(screen.getByRole('dialog')).toBeTruthy();

    hide(first);
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.queryAllByAltText('현장사진.png')).toHaveLength(0);
  });

  it('does not open the viewer when the message is hidden while its preview link is loading', async () => {
    const first = imageMessage('m1', 'att-1', '현장사진.png', 1);
    vi.mocked(listDmThreads).mockResolvedValue({ items: [thread('c1')], safety: SAFETY });
    vi.mocked(listDmMessages).mockResolvedValue({ items: [first] });
    let release: (value: { url: string }) => void = () => undefined;
    vi.mocked(getDmAttachmentPreviewUrl).mockImplementationOnce(
      () => new Promise((resolve) => { release = resolve; }),
    );
    render(view('c1'));

    fireEvent.click(
      await screen.findByRole('button', { name: tr('dm.openImageAttachment', { filename: '현장사진.png' }) }),
    );
    await waitFor(() => expect(getDmAttachmentPreviewUrl).toHaveBeenCalledTimes(1));
    hide(first);
    await act(async () => {
      release({ url: '/dm/attachments/att-1/content?disposition=inline' });
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
