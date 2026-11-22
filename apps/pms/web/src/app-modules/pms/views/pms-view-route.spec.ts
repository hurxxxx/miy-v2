import { describe, expect, it } from 'vitest';

import {
  buildPmsSpaceTaskToolPath,
  buildPmsSpaceToolId,
  buildPmsSpaceToolPath,
  buildPmsCreateTaskSearch,
  buildPmsTaskListToolId,
  buildPmsTaskListToolPath,
  clearPmsCreateTaskSearchParams,
  readPmsCreateTaskRequest,
  resolvePmsViewRoute,
} from './pms-view-route';

describe('PMS view route resolver', () => {
  // miy: paths are app-relative (`/lists/<id>`, `/spaces/<id>`); AI-DO built
  // `/w/<workspaceSlug>/pms/...`. The workspace argument is ignored.
  it('builds PMS navigation ids and workspace paths from route inputs', () => {
    expect(buildPmsTaskListToolId('list-1')).toBe('pms-list-list-1');
    expect(
      buildPmsTaskListToolPath({
        taskListId: 'list-1',
        workspaceSlug: 'hq',
      }),
    ).toBe('/lists/list-1');
    expect(
      buildPmsTaskListToolPath({
        taskListId: 'list-1',
        tab: 'board',
        workspaceSlug: 'hq',
      }),
    ).toBe('/lists/list-1?tab=board');
    expect(
      buildPmsTaskListToolPath({
        settings: true,
        taskListId: 'list-1',
        workspaceSlug: 'hq',
      }),
    ).toBe('/lists/list-1?settings=1');
    expect(
      buildPmsTaskListToolPath({
        taskId: 'task-1',
        taskListId: 'list-1',
        workspaceSlug: 'hq',
      }),
    ).toBe('/lists/list-1?task=task-1');
    expect(buildPmsSpaceToolId('space-1')).toBe('pms-space-space-1');
    expect(buildPmsSpaceToolPath('space-1', { workspaceSlug: 'hq' })).toBe(
      '/spaces/space-1',
    );
    expect(buildPmsSpaceToolPath('space-1')).toBe('/spaces/space-1');
  });

  it('encodes PMS path segments', () => {
    expect(
      buildPmsTaskListToolPath({
        taskListId: 'list/1',
        workspaceSlug: '기술연구소',
      }),
    ).toBe('/lists/list%2F1');
    expect(buildPmsSpaceToolPath('space/1', { workspaceSlug: 'hq' })).toBe(
      '/spaces/space%2F1',
    );
  });

  it('serializes list view query state on workspace paths', () => {
    expect(
      buildPmsTaskListToolPath({
        taskListId: 'list-1',
        tab: 'board',
        settings: true,
        workspaceSlug: 'hq',
      }),
    ).toBe('/lists/list-1?tab=board&settings=1');
  });

  it.each(['list', 'board', 'calendar', 'gantt', 'table'] as const)(
    'builds and resolves the %s space task view without selecting a list',
    (tab) => {
      expect(
        buildPmsSpaceTaskToolPath({
          spaceId: 'space-1',
          tab,
          workspaceSlug: 'hq',
        }),
      ).toBe(`/spaces/space-1?tab=${tab}`);

      expect(
        resolvePmsViewRoute({
          routePathname: '/spaces/space-1',
          requestedTab: tab,
          createTaskRequested: false,
          isNewTaskModalOpen: false,
        }),
      ).toEqual({ kind: 'spaceTasks', spaceId: 'space-1', tab });
    },
  );

  it('fails closed to the space overview for an unknown space tab', () => {
    expect(
      resolvePmsViewRoute({
        routePathname: '/spaces/space-1',
        requestedTab: 'unknown',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'spaceOverview', spaceId: 'space-1' });
  });

  it('resolves assigned and today routes', () => {
    expect(
      resolvePmsViewRoute({
        toolId: 'pms-tasks',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'assigned' });
    expect(
      resolvePmsViewRoute({
        toolId: 'pms-tasks-assigned',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'assigned' });
    expect(
      resolvePmsViewRoute({
        toolId: 'pms-tasks-today',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'today' });
  });

  it('resolves task list routes', () => {
    expect(
      resolvePmsViewRoute({
        routePathname: '/lists/list-1',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'list', taskListId: 'list-1' });
    expect(
      resolvePmsViewRoute({
        routePathname: '/lists/list%2F1',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'list', taskListId: 'list/1' });
  });

  it('resolves space overview before falling through to root overview', () => {
    expect(
      resolvePmsViewRoute({
        routePathname: '/spaces/space-1',
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'spaceOverview', spaceId: 'space-1' });
    expect(
      resolvePmsViewRoute({
        createTaskRequested: false,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'overview' });
  });

  it('keeps empty-root create task state out of the overview route', () => {
    expect(
      resolvePmsViewRoute({
        createTaskRequested: true,
        isNewTaskModalOpen: false,
      }),
    ).toEqual({ kind: 'taskCreateFallback' });
    expect(
      resolvePmsViewRoute({
        createTaskRequested: false,
        isNewTaskModalOpen: true,
      }),
    ).toEqual({ kind: 'taskCreateFallback' });
  });

  it('serializes and clears PMS create-task query state', () => {
    const search = buildPmsCreateTaskSearch({
      sourceTodoId: 'todo-1',
      title: '  Follow up  ',
    });

    expect(search).toBe(
      '?create=1&title=Follow+up&source=personal-todo&sourceTodoId=todo-1',
    );
    expect(readPmsCreateTaskRequest(new URLSearchParams(search))).toEqual({
      sourceTodoId: 'todo-1',
      title: 'Follow up',
    });

    const cleared = clearPmsCreateTaskSearchParams(
      new URLSearchParams(`${search}&tab=board`),
    );

    expect(cleared.toString()).toBe('tab=board');
  });
});
