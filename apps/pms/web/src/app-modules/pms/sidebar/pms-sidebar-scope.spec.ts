import { describe, expect, it } from 'vitest';
import type { PmsSpace } from '../api/pms-api';
import {
  pmsSidebarListPath,
  pmsSidebarSpaceKey,
  pmsSidebarSpacePath,
} from './pms-sidebar-scope';

const spaces = ['one', 'two'].map((name) => ({
  id: `id-${name}`,
  workspace_id: `id-${name}`,
  workspace_key: `pms-${name}`,
  key: `pms-${name}`,
  name,
  description: '',
  member_count: 1,
  current_user_role: 'member',
  created_at: '',
  updated_at: '',
})) satisfies PmsSpace[];

describe('PMS sidebar explicit resource scope', () => {
  it('uses each authorized row owner in the aggregate instead of its global key', () => {
    expect(
      spaces.map((space) =>
        pmsSidebarSpaceKey({
          canonical: true,
          currentWorkspaceSlug: 'app-pms',
          spaces,
          spaceId: space.id,
        }),
      ),
    ).toEqual(['pms-one', 'pms-two']);
  });

  it('does not invent owners for unknown or unpromoted rows', () => {
    const scope = { canonical: true, currentWorkspaceSlug: 'app-pms', spaces };
    expect(pmsSidebarSpaceKey({ ...scope, spaceId: 'missing' })).toBeNull();
    expect(
      pmsSidebarSpaceKey({
        ...scope,
        spaceId: 'id-one',
        spaces: [{ ...spaces[0], workspace_id: 'legacy-parent' }],
      }),
    ).toBeNull();
  });

  // miy: the app runs on the workspace-route shape without an app runtime;
  // paths are app-relative (AI-DO: `/w/<slug>/pms/...`).
  it('preserves explicit legacy routing when no app runtime exists', () => {
    expect(
      pmsSidebarSpaceKey({
        canonical: false,
        currentWorkspaceSlug: 'legacy',
        spaces: [],
        spaceId: 'team',
      }),
    ).toBe('legacy');
    expect(pmsSidebarSpacePath(null, 'legacy', 'team')).toBe('/spaces/team');
    expect(pmsSidebarListPath(null, 'legacy', 'list', true)).toBe(
      '/lists/list?settings=1',
    );
  });
});
