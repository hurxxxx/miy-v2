import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { FeedbackProvider } from '@miy/ui';
import type { AppRuntime, NavigationState } from '@miy/sdk';
import { useMemo, useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { resolveAppRoute } from '../../../platform/runtime/app-location';
import { NavigationStateProvider } from '../../../platform/runtime/navigation-state';
import { RuntimeProvider } from '../../../platform/runtime/runtime-context';
import {
  CommunityApiError,
  createCommunityComment,
  createCommunityPost,
  deleteCommunityComment,
  deleteCommunityPost,
  getCommunityPost,
  listCommunityChannels,
  listCommunityPosts,
  markCommunityPostRead,
} from '../api/community-api';
import { CommunityView } from './CommunityView';

// Ported from AI-DO app-modules/community/views/CommunityView.spec.tsx. AI-DO
// rendered the view in a router with a session credential; miy renders it
// with a frame runtime whose `navigate` moves `location`, and the API
// functions receive `runtime.api` (`api` below).
vi.mock('../../../platform/board/board-context', () => ({
  useIsBoardManager: () => false,
}));

vi.mock('../../../platform/media/use-media-upload', () => ({
  useMediaUpload: () => ({
    resolveFileUrl: async (url: string) => url,
    uploadFile: async () => 'media:test',
  }),
}));

vi.mock('../../../platform/community/CommunityMarkdownEditor', () => ({
  CommunityMarkdownEditor: ({
    ariaLabel,
    onChange,
    value,
  }: {
    ariaLabel: string;
    onChange: (value: string) => void;
    value: string;
  }) => (
    <textarea
      aria-label={ariaLabel}
      onChange={(event) => onChange(event.target.value)}
      value={value}
    />
  ),
  CommunityMarkdownViewer: ({ markdown }: { markdown: string }) => (
    <div>{markdown}</div>
  ),
}));

vi.mock('../api/community-api', async (importOriginal) => ({
  CommunityApiError: (await importOriginal<typeof import('../api/community-api')>())
    .CommunityApiError,
  createCommunityComment: vi.fn(),
  createCommunityPost: vi.fn(),
  deleteCommunityComment: vi.fn(),
  deleteCommunityPost: vi.fn(),
  getCommunityPost: vi.fn(),
  listCommunityChannels: vi.fn(),
  listCommunityPosts: vi.fn(),
  markCommunityPostRead: vi.fn(),
  resolveCommunityPostMediaUrls: vi.fn(),
  unlockCommunityPost: vi.fn(),
  updateCommunityComment: vi.fn(),
  updateCommunityPost: vi.fn(),
}));

const api = vi.fn();
const report = vi.fn<(state: NavigationState) => void>();

function locationOf(path: string) {
  const url = new URL(path, 'https://app.invalid');
  return { pathname: url.pathname, search: url.search, hash: url.hash };
}

function Harness({ initialPath }: { initialPath: string }) {
  const [location, setLocation] = useState(() => locationOf(initialPath));
  const runtime = useMemo(
    () =>
      ({
        api,
        fileApi: vi.fn(),
        location,
        navigate: (path: string) => setLocation(locationOf(path)),
        setNavigationState: vi.fn(),
        settings: {
          dateFormat: 'iso',
          locale: 'ko-KR',
          theme: 'light',
          themePreference: 'light',
          timeZone: 'UTC',
        },
      }) as unknown as AppRuntime,
    [location],
  );
  const route = resolveAppRoute(location.pathname);
  return (
    <RuntimeProvider runtime={runtime}>
      <NavigationStateProvider report={report} message="unsaved">
        <FeedbackProvider labels={{ close: '닫기', item: '알림', region: '알림' }}>
          <CommunityView postId={route.kind === 'community' ? route.postId : null} />
        </FeedbackProvider>
      </NavigationStateProvider>
    </RuntimeProvider>
  );
}

function renderAt(path: string) {
  return render(<Harness initialPath={path} />);
}

describe('CommunityView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCommunityChannels).mockResolvedValue({
      channels: [
        {
          active: true,
          adminOnlyContent: false,
          description: 'General channel',
          forceAnonymous: false,
          id: 'channel-1',
          key: 'general',
          name: 'General',
          position: 1,
          readOnly: false,
          templateBody: '',
          templateTitle: '',
        },
      ],
    });
    vi.mocked(listCommunityPosts).mockResolvedValue({
      page: 1,
      pageSize: 10,
      posts: [],
      total: 0,
    });
    vi.mocked(getCommunityPost).mockResolvedValue({
      authorName: 'Author',
      body: 'Body',
      canModify: true,
      channelKey: 'general',
      commentCount: 0,
      comments: [],
      createdAt: '2026-06-29T00:00:00Z',
      id: 'post-1',
      number: 1,
      isAnonymous: false,
      isMine: true,
      isRead: false,
      isSecret: false,
      locked: false,
      lockedReason: null,
      title: 'Post title',
      updatedAt: '2026-06-29T00:00:00Z',
    });
    vi.mocked(deleteCommunityPost).mockResolvedValue(undefined);
    vi.mocked(markCommunityPostRead).mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('marks an opened post as read without blocking the detail on failure', async () => {
    vi.mocked(markCommunityPostRead).mockRejectedValueOnce(
      new Error('read receipt failed'),
    );

    renderAt('/community/posts/post-1?channel=general');

    expect(
      await screen.findByRole('heading', { name: 'Post title' }),
    ).toBeTruthy();
    await waitFor(() => {
      expect(markCommunityPostRead).toHaveBeenCalledWith(api, 'post-1');
    });
  });

  it('does not mark a locked secret post as read', async () => {
    vi.mocked(getCommunityPost).mockResolvedValue({
      authorName: null,
      body: '',
      canModify: false,
      channelKey: 'general',
      commentCount: 0,
      comments: [],
      createdAt: '2026-06-29T00:00:00Z',
      id: 'post-locked',
      number: 2,
      isAnonymous: true,
      isMine: false,
      isRead: false,
      isSecret: true,
      locked: true,
      lockedReason: 'password',
      title: '',
      updatedAt: '2026-06-29T00:00:00Z',
    });

    renderAt('/community/posts/post-locked?channel=general');

    await waitFor(() => {
      expect(getCommunityPost).toHaveBeenCalledWith(api, 'post-locked');
    });
    expect(markCommunityPostRead).not.toHaveBeenCalled();
  });

  it('uses the app confirm dialog instead of the browser system confirm for post deletion', async () => {
    const browserConfirm = vi
      .spyOn(window, 'confirm')
      .mockImplementation(() => {
        throw new Error('window.confirm should not be called');
      });

    renderAt('/community/posts/post-1?channel=general');

    await screen.findByRole('heading', { name: 'Post title' });

    fireEvent.click(screen.getByRole('button', { name: '삭제' }));

    expect(browserConfirm).not.toHaveBeenCalled();
    const dialog = await screen.findByRole('dialog');

    fireEvent.click(within(dialog).getByRole('button', { name: '삭제' }));

    await waitFor(() => {
      expect(deleteCommunityPost).toHaveBeenCalledWith(api, 'post-1');
    });
  });

  it('uses the app confirm dialog instead of the browser system confirm for comment deletion', async () => {
    vi.mocked(getCommunityPost).mockResolvedValue({
      authorName: 'Author',
      body: 'Body',
      canModify: true,
      channelKey: 'general',
      commentCount: 1,
      comments: [
        {
          anonSeq: null,
          authorName: 'Commenter',
          body: 'Comment body',
          canModify: true,
          createdAt: '2026-06-29T00:00:00Z',
          id: 'comment-1',
          isAnonymous: false,
          isDeleted: false,
          parentCommentId: null,
          updatedAt: '2026-06-29T00:00:00Z',
        },
      ],
      createdAt: '2026-06-29T00:00:00Z',
      id: 'post-1',
      number: 3,
      isAnonymous: false,
      isMine: true,
      isRead: false,
      isSecret: false,
      locked: false,
      lockedReason: null,
      title: 'Post title',
      updatedAt: '2026-06-29T00:00:00Z',
    });
    vi.mocked(deleteCommunityComment).mockResolvedValue(undefined);
    const browserConfirm = vi
      .spyOn(window, 'confirm')
      .mockImplementation(() => {
        throw new Error('window.confirm should not be called');
      });

    renderAt('/community/posts/post-1?channel=general');

    await screen.findByText('Comment body');

    fireEvent.click(screen.getAllByRole('button', { name: '삭제' })[1]);

    expect(browserConfirm).not.toHaveBeenCalled();
    const dialog = await screen.findByRole('dialog');

    fireEvent.click(within(dialog).getByRole('button', { name: '삭제' }));

    await waitFor(() => {
      expect(deleteCommunityComment).toHaveBeenCalledWith(
        api,
        'post-1',
        'comment-1',
      );
    });
  });

  it('shows each post number before its title in the list and on the post', async () => {
    const post = {
      authorName: 'Author',
      body: 'Body',
      canModify: false,
      channelKey: 'general',
      commentCount: 0,
      createdAt: '2026-10-06T00:00:00Z',
      id: 'post-37',
      number: 37,
      isAnonymous: false,
      isMine: false,
      isRead: false,
      isSecret: false,
      locked: false,
      lockedReason: null,
      title: '간트 차트 보기 화면 기능 추가 및 멤버 관리 기능',
      updatedAt: '2026-10-06T00:00:00Z',
    };
    vi.mocked(listCommunityPosts).mockResolvedValue({
      page: 1,
      pageSize: 20,
      posts: [post],
      total: 1,
    });
    vi.mocked(getCommunityPost).mockResolvedValue({ ...post, comments: [] });

    renderAt('/community?channel=general');

    const title = await screen.findByText(post.title);
    expect(screen.getByText('번호')).not.toBeNull();
    const row = title.closest('button');
    expect(row).not.toBeNull();
    expect(
      within(row as HTMLElement).getAllByText('37').length,
    ).toBeGreaterThan(0);

    fireEvent.click(row as HTMLElement);

    expect(
      await screen.findByRole('heading', { name: post.title }),
    ).toBeTruthy();
    expect(screen.getByText('#37')).not.toBeNull();
  });

  it('filters the visible post rows only after submitting the search', async () => {
    const alphaPost = {
      authorName: 'Author',
      body: 'Body',
      canModify: false,
      channelKey: 'general',
      commentCount: 0,
      createdAt: '2026-06-29T00:00:00Z',
      id: 'post-alpha',
      number: 4,
      isAnonymous: false,
      isMine: false,
      isRead: false,
      isSecret: false,
      locked: false,
      lockedReason: null,
      title: 'Alpha release',
      updatedAt: '2026-06-29T00:00:00Z',
    };
    const betaPost = {
      authorName: 'Maintainer',
      body: 'Body',
      canModify: false,
      channelKey: 'general',
      commentCount: 2,
      createdAt: '2026-06-29T00:00:00Z',
      id: 'post-beta',
      number: 5,
      isAnonymous: false,
      isMine: false,
      isRead: true,
      isSecret: false,
      locked: false,
      lockedReason: null,
      title: 'Beta handbook',
      updatedAt: '2026-06-29T00:00:00Z',
    };
    vi.mocked(listCommunityPosts).mockImplementation(
      async (_api, _channelKey, options = {}) => ({
        page: 1,
        pageSize: 20,
        posts: options.search ? [betaPost] : [alphaPost, betaPost],
        total: options.search ? 1 : 2,
      }),
    );

    renderAt('/community?channel=general');

    expect(await screen.findByText('Alpha release')).not.toBeNull();
    expect(listCommunityPosts).toHaveBeenCalledWith(api, 'general', {
      page: 1,
      pageSize: 20,
      search: '',
    });
    const unreadTitle = screen.getByText('Alpha release');
    const readTitle = screen.getByText('Beta handbook');
    expect(unreadTitle.className).toContain('font-semibold');
    expect(readTitle.className).toContain('font-normal');
    const unreadStatusId = unreadTitle
      .closest('button')
      ?.getAttribute('aria-describedby');
    const readStatusId = readTitle
      .closest('button')
      ?.getAttribute('aria-describedby');
    expect(unreadStatusId).toBeTruthy();
    expect(readStatusId).toBeTruthy();
    expect(
      document.getElementById(unreadStatusId ?? '')?.textContent,
    ).toBeTruthy();
    expect(
      document.getElementById(readStatusId ?? '')?.textContent,
    ).toBeTruthy();
    expect(document.getElementById(unreadStatusId ?? '')?.textContent).not.toBe(
      document.getElementById(readStatusId ?? '')?.textContent,
    );

    const searchInput = screen.getByLabelText('글 검색') as HTMLInputElement;
    const initialPostFetchCount =
      vi.mocked(listCommunityPosts).mock.calls.length;
    searchInput.focus();
    fireEvent.change(searchInput, {
      target: { value: 'beta' },
    });

    await new Promise((resolve) => window.setTimeout(resolve, 350));
    expect(listCommunityPosts).toHaveBeenCalledTimes(initialPostFetchCount);
    expect(document.activeElement).toBe(searchInput);

    fireEvent.submit(searchInput.closest('form') as HTMLFormElement);

    await waitFor(() => {
      expect(listCommunityPosts).toHaveBeenLastCalledWith(api, 'general', {
        page: 1,
        pageSize: 20,
        search: 'beta',
      });
    });
    expect(screen.queryByText('Alpha release')).toBeNull();
    expect(screen.getByText('Beta handbook')).not.toBeNull();
  });

  it('disables search for private anonymous channels for regular users', async () => {
    vi.mocked(listCommunityChannels).mockResolvedValue({
      channels: [
        {
          active: true,
          adminOnlyContent: true,
          description: 'Private anonymous channel',
          forceAnonymous: true,
          id: 'channel-private',
          key: 'private',
          name: 'Private',
          position: 1,
          readOnly: false,
          templateBody: '',
          templateTitle: '',
        },
      ],
    });
    vi.mocked(listCommunityPosts).mockResolvedValue({
      page: 1,
      pageSize: 20,
      posts: [
        {
          authorName: null,
          body: '',
          canModify: false,
          channelKey: 'private',
          commentCount: 0,
          createdAt: '2026-06-29T00:00:00Z',
          id: 'post-private',
          number: 6,
          isAnonymous: true,
          isMine: false,
          isRead: false,
          isSecret: false,
          locked: true,
          lockedReason: 'admin_only',
          title: '비공개 글입니다',
          updatedAt: '2026-06-29T00:00:00Z',
        },
      ],
      total: 1,
    });

    renderAt('/community?channel=private');

    const searchInput = await screen.findByLabelText('글 검색');
    expect((searchInput as HTMLInputElement).disabled).toBe(true);
    expect(searchInput.getAttribute('placeholder')).toBe(
      '비공개 익명 게시판은 검색할 수 없습니다.',
    );
    await waitFor(() => {
      expect(listCommunityPosts).toHaveBeenLastCalledWith(api, 'private', {
        page: 1,
        pageSize: 20,
        search: '',
      });
    });
  });

  it('applies channel templates and forced privacy settings when creating a post', async () => {
    vi.mocked(listCommunityChannels).mockResolvedValue({
      channels: [
        {
          active: true,
          adminOnlyContent: true,
          description: 'Private channel',
          forceAnonymous: true,
          id: 'channel-1',
          key: 'private',
          name: 'Private',
          position: 1,
          readOnly: false,
          templateBody: 'Template body',
          templateTitle: 'Template title',
        },
      ],
    });
    vi.mocked(createCommunityPost).mockResolvedValue({
      authorName: null,
      body: '',
      canModify: false,
      channelKey: 'private',
      commentCount: 0,
      createdAt: '2026-06-29T00:00:00Z',
      id: 'post-2',
      number: 7,
      isAnonymous: true,
      isMine: true,
      isRead: false,
      isSecret: false,
      locked: true,
      lockedReason: 'admin_only',
      title: '',
      updatedAt: '2026-06-29T00:00:00Z',
    });

    renderAt('/community?channel=private');

    fireEvent.click(await screen.findByRole('button', { name: '새 글' }));

    expect((screen.getByLabelText('제목') as HTMLInputElement).value).toBe(
      'Template title',
    );
    expect((screen.getByLabelText('본문') as HTMLTextAreaElement).value).toBe(
      'Template body',
    );

    fireEvent.click(screen.getByRole('button', { name: '등록' }));

    await waitFor(() => {
      expect(createCommunityPost).toHaveBeenCalledWith(api, 'private', {
        body: 'Template body',
        isAnonymous: true,
        isSecret: false,
        password: null,
        title: 'Template title',
      });
    });
  });

  it('prevents composing in read-only channels for regular users', async () => {
    vi.mocked(listCommunityChannels).mockResolvedValue({
      channels: [
        {
          active: true,
          adminOnlyContent: false,
          description: 'Announcements',
          forceAnonymous: false,
          id: 'channel-read-only',
          key: 'announcements',
          name: 'Announcements',
          position: 1,
          readOnly: true,
          templateBody: '',
          templateTitle: '',
        },
      ],
    });

    renderAt('/community?channel=announcements');

    const newPostButton = await screen.findByRole('button', { name: '새 글' });

    expect((newPostButton as HTMLButtonElement).disabled).toBe(true);
    expect(
      screen.getByText(
        '읽기 전용 채널입니다. 글과 댓글을 등록하거나 수정할 수 없습니다.',
      ),
    ).toBeTruthy();

    fireEvent.click(newPostButton);

    expect(screen.queryByLabelText('제목')).toBeNull();
    expect(createCommunityPost).not.toHaveBeenCalled();
  });
  it('reports an unsaved comment to the portal (miy contract)', async () => {
    renderAt('/community/posts/post-1?channel=general');
    await screen.findByRole('heading', { name: 'Post title' });
    report.mockClear();
    fireEvent.change(screen.getByLabelText('댓글'), {
      target: { value: '작성 중인 댓글' },
    });
    await waitFor(() => {
      expect(report).toHaveBeenLastCalledWith({ dirty: true, message: 'unsaved' });
    });
  });

  it('names a banned-word rejection instead of the generic failure (보강)', async () => {
    vi.mocked(createCommunityComment).mockRejectedValueOnce(
      new CommunityApiError(422, 'x', 'ugc_content_rejected'),
    );
    renderAt('/community/posts/post-1?channel=general');
    await screen.findByRole('heading', { name: 'Post title' });
    fireEvent.change(screen.getByLabelText('댓글'), {
      target: { value: '금칙어 포함' },
    });
    fireEvent.click(screen.getByRole('button', { name: '댓글 등록' }));
    expect(
      await screen.findByText('운영 정책상 보낼 수 없는 표현이 포함되어 있습니다.'),
    ).toBeTruthy();
  });
});
