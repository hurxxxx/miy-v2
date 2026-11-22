import { describe, expect, it } from 'vitest';

import type { WhiteboardScene } from '../api/whiteboard-api';
import {
  sceneForPersistence,
  sceneSignature,
  selectedElementCount,
  type WhiteboardElement,
} from './whiteboard-scene';

// AI-DO views/whiteboard-collab-scene.spec.ts, without the Yjs map cases.
function element(
  id: string,
  overrides: Partial<WhiteboardElement> = {},
): WhiteboardElement {
  return {
    id,
    type: 'rectangle',
    version: 1,
    versionNonce: 1,
    updated: 1,
    ...overrides,
  };
}

describe('whiteboard scene model', () => {
  it('builds stable signatures and strips transient app state', () => {
    const left: WhiteboardScene = {
      elements: [element('a', { customData: { z: 2, a: 1 } })],
      appState: {
        currentItemStrokeColor: '#111',
        gridModeEnabled: true,
      },
      files: {
        file: { dataURL: 'data:image/png;base64,a', mimeType: 'image/png' },
      },
    };
    const right: WhiteboardScene = {
      files: {
        file: { mimeType: 'image/png', dataURL: 'data:image/png;base64,a' },
      },
      appState: {
        gridModeEnabled: true,
        currentItemStrokeColor: '#222',
      },
      elements: [element('a', { customData: { a: 1, z: 2 } })],
    };

    expect(sceneSignature(left)).toBe(sceneSignature(right));
    expect(sceneForPersistence(left).appState).toEqual({
      gridModeEnabled: true,
    });
  });

  it('gives an image the same signature inline and stored (miy)', () => {
    const inline: WhiteboardScene = {
      elements: [element('img', { type: 'image', fileId: 'f1' })],
      appState: {},
      files: {
        f1: { id: 'f1', mimeType: 'image/png', dataURL: 'data:image/png;base64,AAAA', created: 1 },
      },
    };
    const stored: WhiteboardScene = {
      ...inline,
      files: { f1: { id: 'f1', mimeType: 'image/png', storage: 'row-1', created: 2 } },
    };
    expect(sceneSignature(inline)).toBe(sceneSignature(stored));
    expect(
      sceneSignature({ ...inline, files: { f2: { id: 'f2', mimeType: 'image/png' } } }),
    ).not.toBe(sceneSignature(inline));
  });

  it('counts only selected live elements', () => {
    expect(
      selectedElementCount(
        [
          element('a'),
          element('b', { isDeleted: true }),
          { type: 'anonymous' },
        ],
        { a: true, b: true, c: true },
      ),
    ).toBe(1);
  });
});
