import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { FeedbackProvider } from '@miy/ui';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocsComment } from '../api/docs-api';
import { DocsPageComments } from './DocsPageComments';
import {
  clearCommentDrafts,
  getCommentDraft,
  setCommentDraft,
} from './docs-comment-drafts';

const api = vi.fn();
vi.mock('../../../platform/runtime/runtime-context', () => ({
  useAppApi: () => api,
  useRuntime: () => ({ api, settings: { timeZone: 'Asia/Seoul' } }),
}));

const listDocPageComments = vi.fn();
const createDocPageComment = vi.fn();
vi.mock('../api/docs-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../api/docs-api')>()),
  listDocPageComments: (...args: unknown[]) => listDocPageComments(...args),
  createDocPageComment: (...args: unknown[]) => createDocPageComment(...args),
  listShareableUsers: () => Promise.resolve([]),
}));

function comment(id: string, pageId: string, body: string): DocsComment {
  return {
    id,
    page_id: pageId,
    parent_id: null,
    author_id: 'u1',
    author_name: 'Alice',
    body,
    mentions: [],
    is_deleted: false,
    resolved_at: null,
    resolved_by_id: null,
    resolved_by_name: null,
    created_at: '2026-11-22T01:00:00Z',
    updated_at: '2026-11-22T01:00:00Z',
    can_edit: true,
    can_delete: true,
    can_resolve: true,
    replies: [],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function view(pageId: string) {
  return (
    <FeedbackProvider labels={{ close: 'close', item: 'item', region: 'region' }}>
      <DocsPageComments docId="d1" pageId={pageId} highlightCommentId={null} />
    </FeedbackProvider>
  );
}

describe('docs page comments', () => {
  beforeEach(() => {
    listDocPageComments.mockReset();
    createDocPageComment.mockReset();
    for (const key of ['d1:p1:new', 'd1:p2:new']) setCommentDraft(key, null);
  });

  it('never lets a late answer for another page replace the comments shown', async () => {
    const first = deferred<{ items: DocsComment[]; can_comment: boolean }>();
    listDocPageComments.mockImplementation((_api: unknown, pageId: string) =>
      pageId === 'p1'
        ? first.promise
        : Promise.resolve({ items: [comment('c2', 'p2', 'second page')], can_comment: true }),
    );
    const { rerender } = render(view('p1'));
    rerender(view('p2'));
    expect(await screen.findByText('second page')).toBeTruthy();
    await act(async () => {
      first.resolve({ items: [comment('c1', 'p1', 'first page')], can_comment: true });
    });
    expect(screen.queryByText('first page')).toBeNull();
    expect(screen.getByText('second page')).toBeTruthy();
  });

  it('keeps the text after a failed send and resends with the same request ID', async () => {
    listDocPageComments.mockResolvedValue({ items: [], can_comment: true });
    createDocPageComment
      .mockRejectedValueOnce(new TypeError('answer lost'))
      .mockResolvedValueOnce({ id: 'c9', created_at: '2026-11-22T01:00:00Z' });
    render(view('p1'));
    const box = await screen.findByRole('textbox');
    fireEvent.change(box, { target: { value: 'Looks good', selectionStart: 10 } });
    const send = screen.getByRole('button', { name: '댓글 남기기' });
    fireEvent.click(send);
    await waitFor(() => expect(createDocPageComment).toHaveBeenCalledTimes(1));
    await waitFor(() => expect((box as HTMLTextAreaElement).disabled).toBe(false));
    expect((box as HTMLTextAreaElement).value).toBe('Looks good');
    expect(getCommentDraft('d1:p1:new')?.text).toBe('Looks good');
    fireEvent.click(send);
    await waitFor(() => expect(createDocPageComment).toHaveBeenCalledTimes(2));
    const [firstCall, secondCall] = createDocPageComment.mock.calls;
    expect(firstCall[2].request_id).toBe(secondCall[2].request_id);
    await waitFor(() => expect((box as HTMLTextAreaElement).value).toBe(''));
    expect(getCommentDraft('d1:p1:new')).toBeUndefined();
  });

  it('keeps an unsent comment when the page is left and opened again', async () => {
    listDocPageComments.mockResolvedValue({ items: [], can_comment: true });
    const { unmount } = render(view('p1'));
    fireEvent.change(await screen.findByRole('textbox'), {
      target: { value: 'Draft text', selectionStart: 10 },
    });
    unmount();
    const again = render(view('p1'));
    expect(((await screen.findByRole('textbox')) as HTMLTextAreaElement).value).toBe(
      'Draft text',
    );
    again.unmount();
    // Trashing the page drops its drafts.
    clearCommentDrafts('d1:p1');
    expect(getCommentDraft('d1:p1:new')).toBeUndefined();
  });
});
