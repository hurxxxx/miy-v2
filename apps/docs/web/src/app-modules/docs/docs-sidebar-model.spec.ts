import { describe, expect, it } from 'vitest';

import {
  buildDocsDocPath,
  docsSidebarRowSpaceLabel,
  getInitialDocsSidebarExpandedSections,
  resolveDocsSidebarRowPath,
  toggleDocsSidebarSection,
} from './docs-sidebar-model';

describe('docs sidebar model', () => {
  it('creates and toggles section expansion state', () => {
    const initial = getInitialDocsSidebarExpandedSections();

    expect(initial).toEqual(['favorites', 'recentPages']);
    expect(toggleDocsSidebarSection(initial, 'favorites')).toEqual([
      'recentPages',
    ]);
    expect(toggleDocsSidebarSection(['recentPages'], 'favorites')).toEqual([
      'recentPages',
      'favorites',
    ]);
    expect(initial).toEqual(['favorites', 'recentPages']);
  });

  it('links rows to the doc route of the space that holds the doc (miy)', () => {
    expect(buildDocsDocPath({ docId: 'doc-1' })).toBe('/doc-1');
    expect(buildDocsDocPath({ docId: 'doc-1', spaceId: 'global' })).toBe('/doc-1');
    expect(buildDocsDocPath({ docId: 'doc 1', spaceId: 's-1', pageId: 'p 1' })).toBe(
      '/doc%201?space=s-1&page=p%201',
    );
    const other = { key: 'team', name: 'Team', kind: 'app_space' as const };
    expect(
      resolveDocsSidebarRowPath({ docId: 'd', pageId: 'p', space: other, currentSpaceId: 'global' }),
    ).toBe('/d?space=team&page=p');
    expect(resolveDocsSidebarRowPath({ docId: 'd', currentSpaceId: 'team' })).toBe(
      '/d?space=team',
    );
  });

  it('labels only rows held by a different space', () => {
    const space = {
      key: 'docs-company',
      name: 'Company',
      kind: 'app_space' as const,
    };
    expect(docsSidebarRowSpaceLabel(space, 'docs-ai-tft')).toBe('Company');
    expect(docsSidebarRowSpaceLabel(space, 'docs-company')).toBeNull();
    expect(docsSidebarRowSpaceLabel(null, 'docs-ai-tft')).toBeNull();
  });
});
