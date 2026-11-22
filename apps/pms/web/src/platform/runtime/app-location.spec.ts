import { describe, expect, it } from 'vitest';

import {
  matchPmsRoute,
  PMS_WORKSPACE_KEY,
  resolveAppRoute,
  WIDGET_ROUTE_PATHS,
} from './app-location';

describe('app routes (miy)', () => {
  it('maps the frame routes and falls back to the PMS view', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'pms' });
    expect(resolveAppRoute('')).toEqual({ kind: 'pms' });
    expect(resolveAppRoute('/spaces/s1')).toEqual({ kind: 'pms' });
    expect(resolveAppRoute('/lists/l1')).toEqual({ kind: 'pms' });
    expect(resolveAppRoute('/assigned')).toEqual({ kind: 'assigned' });
    expect(resolveAppRoute('/today/')).toEqual({ kind: 'today' });
    expect(resolveAppRoute('/tasks/t%201')).toEqual({
      kind: 'task',
      taskId: 't 1',
    });
    expect(resolveAppRoute(WIDGET_ROUTE_PATHS.myTasks)).toEqual({
      kind: 'widget',
      widget: 'my-tasks',
    });
    expect(resolveAppRoute('/widgets/other')).toEqual({ kind: 'pms' });
    expect(resolveAppRoute('/unknown/path/here')).toEqual({ kind: 'pms' });
  });

  it('gives the ported screens their route parameters', () => {
    expect(matchPmsRoute('/lists/l%2F1')).toEqual({
      workspaceSlug: PMS_WORKSPACE_KEY,
      taskListId: 'l/1',
    });
    expect(matchPmsRoute('/spaces/s1')).toEqual({
      workspaceSlug: PMS_WORKSPACE_KEY,
      spaceId: 's1',
    });
    expect(matchPmsRoute('/tasks/t1')).toEqual({
      workspaceSlug: PMS_WORKSPACE_KEY,
      taskId: 't1',
    });
    expect(matchPmsRoute('/')).toEqual({ workspaceSlug: PMS_WORKSPACE_KEY });
  });
});
