import { describe, expect, it, vi } from 'vitest';
import type { ExcalidrawImperativeAPI } from '@excalidraw/excalidraw/types';
import type { AppRuntime } from '@miy/sdk';
import { createWhiteboardFileStore } from '../api/whiteboard-files';
import { BoardRoom, type BoardChange } from './board-room';

vi.mock('@excalidraw/excalidraw', () => ({ CaptureUpdateAction: { NEVER: 'never' } }));
vi.mock('../../../../../sdk/collab/client', async () => {
  const { Doc } = await import('yjs');
  const { Awareness } = await import('y-protocols/awareness');
  return { EditingRoom: class {
    doc = new Doc();
    awareness = new Awareness(this.doc);
    constructor(_runtime: unknown, _path: string, private notify: (status: unknown) => void) {}
    start() {
      this.doc.getMap('elements').set('seed:a', JSON.stringify({pos: 0, element: {id: 'a', x: 0, version: 1, updated: 1, versionNonce: 5}}));
      this.notify({ready: true, phase: 'saved', dirty: false, revision: 1});
    }
    destroy() { this.awareness.destroy(); this.doc.destroy(); }
  }};
});

describe('Excalidraw editing room', () => {
  it('publishes successive in-place movements of a previously loaded element', () => {
    let canvas: Array<{id: string; x: number; version: number}> = [];
    const appState = { selectedElementIds: {} };
    const editor = {
      getAppState: () => appState, getFiles: () => ({}),
      getSceneElementsIncludingDeleted: () => canvas,
      updateScene: (scene: {elements?: typeof canvas}) => { if (scene.elements) canvas = scene.elements; },
    };
    const board = new BoardRoom({} as AppRuntime, 'b', 's', editor as unknown as ExcalidrawImperativeAPI, createWhiteboardFileStore(), vi.fn(), vi.fn());
    try {
      const elements = board.room.doc.getMap<string>('elements');
      for (const version of [2,3]) {
        Object.assign(canvas[0], { x: version * 5, version });
        board.change({elements: canvas, appState, files: {}} as unknown as BoardChange);
        const saved = JSON.parse(elements.get(board.room.doc.clientID + ':a')!);
        expect(saved.element.x).toBe(version * 5);
      }
    } finally { board.destroy(); }
  });
});
