import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@miy/sdk';

import { i18n } from '../../../platform/i18n';
import type {
  AppApi,
  AppApiBlob,
  AppFileApi,
} from '../../../platform/runtime/runtime-context';
import {
  PMS_ATTACHMENT_MAX_BYTES,
  bulkUpdateTasks,
  createChecklistItem,
  exportTaskListXlsx,
  getPmsTaskList,
  isFilteredTaskExport,
  listPersonalPmsAssignedTasks,
  taskExportFilename,
  updateSpaceMemberRole,
  uploadAttachment,
} from './pms-api';

// miy transport (replaces AI-DO's pms-runtime-scope.spec.ts, which checked the
// workspace API path rewriting): app-relative paths through `runtime.api`,
// attachments through the platform file gateway, the export through
// `runtime.apiBlob` with the file name computed on the screen.

type ApiCall = [string, RequestInit | undefined];

function apiFake(handler: (path: string, init?: RequestInit) => unknown) {
  const mock = vi.fn(async (path: string, init?: RequestInit) =>
    handler(path, init),
  );
  return { api: mock as unknown as AppApi, mock };
}

const ATTACHMENT = {
  id: 'att-1',
  task_id: 'task/1',
  filename: 'spec.pdf',
  content_type: 'application/pdf',
  size_bytes: 4,
  uploaded_by: 'user-1',
  uploaded_by_name: 'User',
  download_url: '/pms/attachments/att-1/content',
  created_at: '2026-11-22T00:00:00Z',
};

describe('PMS API paths (miy)', () => {
  it('sends app-relative paths with encoded ids and ignores the workspace', async () => {
    const { api, mock } = apiFake(() => ({}));
    await getPmsTaskList(api, 'list a/1', 'pms-a');
    await bulkUpdateTasks(
      api,
      'list-a',
      { task_ids: ['task-a'], archived: true },
      'pms-a',
    );
    await createChecklistItem(api, 'task?a', { text: 'Inspect result' }, 'pms-a');
    await updateSpaceMemberRole(api, 'space#1', 'user/1', 'admin');
    expect(mock.mock.calls.map(([path]) => path)).toEqual([
      '/v1/pms/lists/list%20a%2F1',
      '/v1/pms/lists/list-a/tasks/bulk',
      '/v1/pms/tasks/task%3Fa/checklist',
      '/v1/pms/spaces/space%231/members/user%2F1',
    ]);
    expect((mock.mock.calls[1] as ApiCall)[1]).toMatchObject({
      method: 'PATCH',
      body: JSON.stringify({ task_ids: ['task-a'], archived: true }),
    });
  });
});

describe('uploadAttachment (miy file gateway)', () => {
  it('records the attachment, then puts the bytes through fileApi', async () => {
    const { api, mock } = apiFake((path) =>
      path.endsWith('/attachments')
        ? {
            id: 'att-1',
            task_id: 'task/1',
            filename: 'spec.pdf',
            content_type: 'application/pdf',
            size_bytes: 4,
            status: 'pending',
          }
        : undefined,
    );
    const fileApi = vi.fn(
      async () =>
        new Response(JSON.stringify(ATTACHMENT), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
    );
    const file = new File(['data'], 'spec.pdf', { type: 'application/pdf' });

    const result = await uploadAttachment(
      api,
      fileApi as unknown as AppFileApi,
      'task/1',
      file,
    );

    expect(result).toEqual(ATTACHMENT);
    expect(mock).toHaveBeenCalledTimes(1);
    const [path, init] = mock.mock.calls[0] as ApiCall;
    expect(path).toBe('/v1/pms/tasks/task%2F1/attachments');
    expect(init?.method).toBe('POST');
    const body = JSON.parse(String(init?.body));
    expect(body).toMatchObject({
      filename: 'spec.pdf',
      content_type: 'application/pdf',
      size_bytes: 4,
    });
    expect(body.request_id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    const [filePath, fileInit] = fileApi.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(filePath).toBe('/pms/attachments/att-1/content');
    expect(fileInit.method).toBe('PUT');
    expect(fileInit.body).toBe(file);
    expect(new Headers(fileInit.headers).get('Content-Type')).toBe(
      'application/octet-stream',
    );
  });

  it('deletes the pending record when the upload fails', async () => {
    const { api, mock } = apiFake((path) =>
      path.endsWith('/attachments')
        ? { id: 'att-9', status: 'pending' }
        : undefined,
    );
    const fileApi = vi.fn(
      async () =>
        new Response(
          JSON.stringify({ error: { code: 'pms_attachment_not_found' } }),
          { status: 404, headers: { 'Content-Type': 'application/json' } },
        ),
    );

    await expect(
      uploadAttachment(
        api,
        fileApi as unknown as AppFileApi,
        'task-1',
        new File(['data'], 'a.txt'),
      ),
    ).rejects.toMatchObject({
      status: 404,
      message: '첨부파일을 찾을 수 없습니다.',
    });
    expect(mock.mock.calls.map(([path, init]) => [path, init?.method])).toEqual(
      [
        ['/v1/pms/tasks/task-1/attachments', 'POST'],
        ['/v1/pms/attachments/att-9', 'DELETE'],
      ],
    );
  });

  it('refuses files over 10 MiB before any request', async () => {
    const { api, mock } = apiFake(() => ({}));
    const fileApi = vi.fn();
    const file = new File(['x'], 'big.bin');
    Object.defineProperty(file, 'size', { value: PMS_ATTACHMENT_MAX_BYTES + 1 });

    await expect(
      uploadAttachment(api, fileApi as unknown as AppFileApi, 'task-1', file),
    ).rejects.toMatchObject({
      status: 413,
      message: i18n.t('errors:pms_file_size_limit_exceeded', { limit_mb: 10 }),
    });
    expect(i18n.t('errors:pms_file_size_limit_exceeded', { limit_mb: 10 })).toBe(
      '파일 크기가 10 MB 제한을 초과했습니다.',
    );
    expect(mock).not.toHaveBeenCalled();
    expect(fileApi).not.toHaveBeenCalled();
  });
});

describe('exportTaskListXlsx (miy apiBlob)', () => {
  let clicked: HTMLAnchorElement[];

  beforeEach(() => {
    clicked = [];
    URL.createObjectURL = vi.fn(() => 'blob:export');
    URL.revokeObjectURL = vi.fn();
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(
      function (this: HTMLAnchorElement) {
        clicked.push(this);
      },
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('names the file like the AI-DO server did', () => {
    expect(taskExportFilename({ key: 'LS', name: '주간: 보고/계획' }, false)).toBe(
      '주간  보고 계획.xlsx',
    );
    expect(taskExportFilename({ key: 'LS1', name: ' ?* ' }, false)).toBe(
      'LS1.xlsx',
    );
    expect(taskExportFilename({ key: 'LS', name: '목록' }, true)).toBe(
      '목록 (필터 적용).xlsx',
    );
    expect(isFilteredTaskExport(new URLSearchParams())).toBe(false);
    expect(isFilteredTaskExport(new URLSearchParams('q=%20%20'))).toBe(false);
    expect(isFilteredTaskExport(new URLSearchParams('q=motor'))).toBe(true);
    expect(isFilteredTaskExport(new URLSearchParams('archived=false'))).toBe(
      true,
    );
  });

  it('downloads the blob under the list name with the filter suffix', async () => {
    const { api, mock } = apiFake(() => ({
      id: 'list-1',
      key: 'LS',
      name: '휴가 계획',
    }));
    const apiBlob = vi.fn(async (_path: string) => new Blob(['xlsx']));

    await exportTaskListXlsx(
      api,
      apiBlob as unknown as AppApiBlob,
      'list-1',
      { archived_state: 'active', q: '휴가' },
    );

    expect(mock.mock.calls[0]?.[0]).toBe('/v1/pms/lists/list-1');
    expect(apiBlob.mock.calls[0]?.[0]).toBe(
      '/v1/pms/lists/list-1/export?q=%ED%9C%B4%EA%B0%80&archived=false',
    );
    expect(clicked).toHaveLength(1);
    expect(clicked[0]?.download).toBe('휴가 계획 (필터 적용).xlsx');
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:export');
  });

  it('reports a failed export with the AI-DO message', async () => {
    const { api } = apiFake(() => ({ id: 'list-1', key: 'LS', name: 'L' }));
    const apiBlob = vi.fn(async () => {
      throw new ApiError('http', 'http_500', 500);
    });

    await expect(
      exportTaskListXlsx(api, apiBlob as unknown as AppApiBlob, 'list-1'),
    ).rejects.toMatchObject({
      status: 500,
      message: i18n.t('apps:pms.errors.exportFailed'),
    });
    expect(clicked).toHaveLength(0);
  });
});

describe('listPersonalPmsAssignedTasks (miy composition)', () => {
  it('annotates assigned tasks with their space and drops unknown lists', async () => {
    const { api, mock } = apiFake((path) => {
      if (path === '/v1/pms/spaces') {
        return [
          { id: 'space-b', name: 'Beta', current_user_role: 'member' },
          { id: 'space-a', name: 'Alpha', current_user_role: 'viewer' },
          { id: 'space-c', name: 'Gamma', current_user_role: null },
        ];
      }
      if (path.startsWith('/v1/pms/lists?')) {
        return {
          items: [
            { id: 'list-a', team_id: 'space-a' },
            { id: 'list-b', team_id: 'space-b' },
          ],
          total: 2,
          page: 1,
          page_size: 100,
        };
      }
      if (path.startsWith('/v1/pms/tasks/assigned?')) {
        return {
          items: [
            { id: 'task-1', list_id: 'list-b' },
            { id: 'task-2', list_id: 'list-gone' },
            { id: 'task-3', list_id: 'list-a' },
          ],
          total: 3,
          page: 1,
          page_size: 50,
        };
      }
      throw new Error(`unexpected ${path}`);
    });

    const result = await listPersonalPmsAssignedTasks(api);

    expect(result.workspaces).toEqual([
      { id: 'space-a', slug: 'space-a', name: 'Alpha', role: 'viewer' },
      { id: 'space-b', slug: 'space-b', name: 'Beta', role: 'member' },
    ]);
    expect(
      result.items.map((task) => [task.id, task.workspace.slug]),
    ).toEqual([
      ['task-1', 'space-b'],
      ['task-3', 'space-a'],
    ]);
    expect(result.items[0]?.workspace).toEqual({
      id: 'space-b',
      slug: 'space-b',
      name: 'Beta',
    });
    expect(result.total).toBe(2);
    expect(mock.mock.calls.map(([path]) => path)).toEqual(
      expect.arrayContaining([
        '/v1/pms/spaces',
        '/v1/pms/lists?archived=false&page=1&page_size=100',
        '/v1/pms/tasks/assigned?page=1&page_size=50',
      ]),
    );
  });
});
