import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  AppApi,
  AppApiBlob,
} from '../../../platform/runtime/runtime-context';
import { exportTaskListXlsx } from './pms-api';

// miy: the file comes from `runtime.apiBlob` (no response headers) and the
// list name from `runtime.api`; AI-DO fetched both through one response.
function exportClients() {
  const api = vi.fn(async (_path: string) => ({
    id: 'list-1',
    key: 'LS',
    name: '목록',
  }));
  const fetchSpy = vi.fn(async (_path: string) => new Blob(['xlsx']));
  return {
    api: api as unknown as AppApi,
    apiBlob: fetchSpy as unknown as AppApiBlob,
    fetchSpy,
  };
}

describe('PMS task export', () => {
  beforeEach(() => {
    URL.createObjectURL = vi.fn(() => 'blob:export');
    URL.revokeObjectURL = vi.fn();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('asks for the whole list when no filter is given', async () => {
    const { api, apiBlob, fetchSpy } = exportClients();

    await exportTaskListXlsx(api, apiBlob, 'list-1');

    expect(String(fetchSpy.mock.calls[0][0])).toMatch(
      /\/v1\/pms\/lists\/list-1\/export$/,
    );
  });

  it('sends the list filter the same way the task query does', async () => {
    const { api, apiBlob, fetchSpy } = exportClients();

    await exportTaskListXlsx(api, apiBlob, 'list-1', {
      archived_state: 'active',
      label_id: 'leave',
      q: '[휴가]',
      status: ['todo', 'in_progress'],
    });

    const url = new URL(String(fetchSpy.mock.calls[0][0]), 'http://local');
    expect(url.pathname).toMatch(/\/v1\/pms\/lists\/list-1\/export$/);
    expect(url.searchParams.get('q')).toBe('[휴가]');
    expect(url.searchParams.get('label_id')).toBe('leave');
    expect(url.searchParams.get('archived')).toBe('false');
    expect(url.searchParams.getAll('status')).toEqual(['todo', 'in_progress']);
  });
});
