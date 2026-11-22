import { describe, expect, it } from 'vitest';

import { resolveAppRoute, WIDGET_ROUTE_PATHS } from './app-location';

describe('app routes (miy)', () => {
  it('maps the panel routes and falls back to the planner', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'planner' });
    expect(resolveAppRoute('')).toEqual({ kind: 'planner' });
    expect(resolveAppRoute('/planner')).toEqual({ kind: 'planner' });
    expect(resolveAppRoute(WIDGET_ROUTE_PATHS.todo)).toEqual({
      kind: 'widget',
      widget: 'todo',
    });
    expect(resolveAppRoute('/widgets/memos/')).toEqual({
      kind: 'widget',
      widget: 'memo',
    });
    expect(resolveAppRoute('/widgets/other')).toEqual({ kind: 'planner' });
  });
});
