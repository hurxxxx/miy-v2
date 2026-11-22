import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import { listAllPmsTaskLists, listPmsTaskLists } from './pms-api';

function response(items: Array<{ id: string }>, total = items.length) {
  return { items, total, page: 1, page_size: 100 };
}

// miy: `runtime.api` (app-relative paths) replaces `fetch` with a token.
const asApi = (mock: ReturnType<typeof vi.fn>) => mock as unknown as AppApi;

describe('PMS task-list queries', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('requests active lists by default and supports archived-only queries', async () => {
    const fetchSpy = vi.fn().mockResolvedValue(response([]));

    await listPmsTaskLists(asApi(fetchSpy), 'space-1', 'workspace');
    await listPmsTaskLists(asApi(fetchSpy), 'space-1', 'workspace', {
      archived: true,
    });

    expect(String(fetchSpy.mock.calls[0][0])).toContain('archived=false');
    expect(String(fetchSpy.mock.calls[1][0])).toContain('archived=true');
  });

  it('preserves the archive filter while loading every page', async () => {
    const fetchSpy = vi
      .fn()
      .mockResolvedValueOnce(response([{ id: 'archived-1' }], 2))
      .mockResolvedValueOnce(response([{ id: 'archived-2' }], 2));

    const result = await listAllPmsTaskLists(
      asApi(fetchSpy),
      undefined,
      'workspace',
      {
        archived: true,
      },
    );

    expect(result.items.map((item) => item.id)).toEqual([
      'archived-1',
      'archived-2',
    ]);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(String(fetchSpy.mock.calls[0][0])).toContain('archived=true');
    expect(String(fetchSpy.mock.calls[1][0])).toContain('archived=true');
  });
});
