import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import { listSpaceMembers } from './pms-api';

function response(userIds: string[], total: number) {
  return {
    items: userIds.map((userId) => ({ user_id: userId })),
    page: 1,
    page_size: 100,
    total,
  };
}

describe('listSpaceMembers', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('reads every page so a large space is complete', async () => {
    // miy: `runtime.api` with app-relative paths replaces `fetch` + token.
    const fetchSpy = vi
      .fn()
      .mockResolvedValueOnce(response(['a', 'b'], 3))
      .mockResolvedValueOnce(response(['c'], 3));

    const result = await listSpaceMembers(
      fetchSpy as unknown as AppApi,
      'space-1',
      'workspace',
    );

    expect(result.items.map((member) => member.user_id)).toEqual([
      'a',
      'b',
      'c',
    ]);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(String(fetchSpy.mock.calls[0][0])).toContain('page=1&page_size=100');
    expect(String(fetchSpy.mock.calls[1][0])).toContain('page=2&page_size=100');
  });
});
