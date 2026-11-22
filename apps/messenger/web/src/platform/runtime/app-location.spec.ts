import { describe, expect, it } from 'vitest';

import { conversationPath, resolveAppRoute } from './app-location';

describe('messenger routes (miy)', () => {
  it('resolves conversations, the side panel and the report review screen', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'messenger', threadId: '' });
    expect(resolveAppRoute('/conversations/c-1')).toEqual({ kind: 'messenger', threadId: 'c-1' });
    expect(resolveAppRoute('/widgets/conversations')).toEqual({ kind: 'panel', threadId: '' });
    expect(resolveAppRoute('/widgets/conversations/c-2/')).toEqual({ kind: 'panel', threadId: 'c-2' });
    expect(resolveAppRoute('/safety')).toEqual({ kind: 'safety', reportId: null });
    expect(resolveAppRoute('/safety/reports/r-1')).toEqual({ kind: 'safety', reportId: 'r-1' });
    expect(resolveAppRoute('/unknown/path')).toEqual({ kind: 'messenger', threadId: '' });
    expect(resolveAppRoute('/conversations/%E0%A4%A')).toEqual({ kind: 'messenger', threadId: '' });
  });

  it('builds the matching paths', () => {
    expect(conversationPath('messenger', '')).toBe('/');
    expect(conversationPath('messenger', 'c 1')).toBe('/conversations/c%201');
    expect(conversationPath('panel', '')).toBe('/widgets/conversations');
    expect(conversationPath('panel', 'c1')).toBe('/widgets/conversations/c1');
  });
});
