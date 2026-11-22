import { describe, expect, it } from 'vitest';

import { GLOBAL_SPACE_ID, readSpaceId, resolveAppRoute } from './app-location';

describe('app location', () => {
  it('maps AI-DO whiteboard routes into the frame', () => {
    expect(resolveAppRoute('/')).toEqual({ kind: 'hub' });
    expect(resolveAppRoute('')).toEqual({ kind: 'hub' });
    expect(resolveAppRoute('/b-1')).toEqual({ kind: 'board', whiteboardId: 'b-1' });
    expect(resolveAppRoute('/b-1/')).toEqual({ kind: 'board', whiteboardId: 'b-1' });
    expect(resolveAppRoute('/a/b')).toEqual({ kind: 'hub' });
    expect(resolveAppRoute('/%E0%A4%A')).toEqual({ kind: 'hub' });
  });

  it('reads the space from the query', () => {
    expect(readSpaceId(new URLSearchParams(''))).toBe(GLOBAL_SPACE_ID);
    expect(readSpaceId(new URLSearchParams('space=s1&view=mine'))).toBe('s1');
  });
});
