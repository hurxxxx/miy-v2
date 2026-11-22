import { describe, expect, it } from 'vitest';

import { buildNoticesUrl, resolveAppRoute } from './app-location';

describe('board routes', () => {
  it('keeps the AI-DO community routes and treats / as the community', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'community', postId: null });
    expect(resolveAppRoute('/community')).toEqual({ kind: 'community', postId: null });
    expect(resolveAppRoute('/community/posts/post%201')).toEqual({
      kind: 'community',
      postId: 'post 1',
    });
    expect(resolveAppRoute('/community/posts/abc/')).toEqual({
      kind: 'community',
      postId: 'abc',
    });
  });

  it('resolves notices and board settings, other paths show the community', () => {
    expect(resolveAppRoute('/notices')).toEqual({ kind: 'notices', noticeId: null });
    expect(resolveAppRoute('/notices/n-1')).toEqual({ kind: 'notices', noticeId: 'n-1' });
    expect(resolveAppRoute('/admin')).toEqual({ kind: 'admin' });
    expect(resolveAppRoute('/unknown/path')).toEqual({ kind: 'community', postId: null });
    expect(resolveAppRoute('/community/posts/%E0%A4%A')).toEqual({
      kind: 'community',
      postId: null,
    });
  });

  it('builds notice URLs with the space context', () => {
    expect(buildNoticesUrl()).toBe('/notices');
    expect(buildNoticesUrl('space 1')).toBe('/notices?space=space+1');
    expect(buildNoticesUrl('s', 'notice/1')).toBe('/notices/notice%2F1?space=s');
    expect(buildNoticesUrl(null, 'n')).toBe('/notices/n');
  });
});
