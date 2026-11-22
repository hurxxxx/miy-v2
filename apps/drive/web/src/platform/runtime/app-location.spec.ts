import { describe, expect, it } from 'vitest';

import { buildSpacePath, buildTrashPath, resolveAppRoute } from './app-location';

describe('drive routes (miy: AI-DO /w/<slug>/files as /spaces/<id>)', () => {
  it('resolves spaces, trash and the space picker', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'home' });
    expect(resolveAppRoute('/spaces/abc')).toEqual({ kind: 'space', spaceId: 'abc' });
    expect(resolveAppRoute('/spaces/abc/')).toEqual({ kind: 'space', spaceId: 'abc' });
    expect(resolveAppRoute('/spaces/a%20b/trash')).toEqual({
      kind: 'trash',
      spaceId: 'a b',
    });
    expect(resolveAppRoute('/spaces/%E0%A4%A')).toEqual({ kind: 'home' });
    expect(resolveAppRoute('/elsewhere')).toEqual({ kind: 'home' });
  });

  it('builds app-relative paths with the AI-DO query parameters', () => {
    expect(buildSpacePath('s 1')).toBe('/spaces/s%201');
    expect(buildSpacePath('s', { folder: 'f' })).toBe('/spaces/s?folder=f');
    expect(buildSpacePath('s', { view: 'search' })).toBe('/spaces/s?view=search');
    expect(buildTrashPath('s')).toBe('/spaces/s/trash');
  });
});
