import { describe, expect, it } from 'vitest';

import { resolveAppRoute } from './app-location';

describe('docs routes', () => {
  it('maps the list, a doc and settings like AI-DO docs routes', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'list' });
    expect(resolveAppRoute('/docs')).toEqual({ kind: 'list' });
    expect(resolveAppRoute('/doc-1')).toEqual({ kind: 'doc', docId: 'doc-1' });
    expect(resolveAppRoute('/docs/doc-1/')).toEqual({ kind: 'doc', docId: 'doc-1' });
    expect(resolveAppRoute('/a%20b')).toEqual({ kind: 'doc', docId: 'a b' });
    expect(resolveAppRoute('/settings')).toEqual({ kind: 'settings' });
    expect(resolveAppRoute('/trash')).toEqual({ kind: 'trash' });
  });

  it('shows the list for unknown or malformed paths', () => {
    expect(resolveAppRoute('/doc-1/html/page-1')).toEqual({ kind: 'list' });
    expect(resolveAppRoute('/%E0%A4%A')).toEqual({ kind: 'list' });
  });
});
