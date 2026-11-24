import { CaptureUpdateAction } from '@excalidraw/excalidraw';
import type {
  AppState,
  BinaryFileData,
  BinaryFiles,
  ExcalidrawImperativeAPI,
  ExcalidrawProps,
} from '@excalidraw/excalidraw/types';
import type { AppRuntime } from '@miy/sdk';
import { EditingRoom, type RoomStatus } from '../../../../../sdk/collab/client';
import {
  hydrateSceneFiles,
  storeSceneFiles,
  type WhiteboardFileStore,
} from '../api/whiteboard-files';

type Element = Parameters<NonNullable<ExcalidrawProps['onChange']>>[0][number];
export type BoardChange = {
  elements: readonly Element[];
  appState: AppState;
  files: BinaryFiles;
};
type Candidate = { pos: number; element: Element };
const settings = [
  'viewBackgroundColor',
  'gridModeEnabled',
  'gridSize',
] as const;
const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
type ElementClock = Pick<Element, 'version' | 'updated' | 'versionNonce'>;
function clockOf({version, updated, versionNonce}: ElementClock): ElementClock {
  // Excalidraw mutates elements in place while dragging and nudging.
  return {version, updated, versionNonce};
}
const rank = (a: ElementClock, b: ElementClock) =>
  (a.version || 0) - (b.version || 0) ||
  (a.updated || 0) - (b.updated || 0) ||
  (a.versionNonce || 0) - (b.versionNonce || 0);

export class BoardRoom {
  readonly room: EditingRoom;
  private seen = new Map<string, ElementClock>();
  private localSettings = new Map<string, unknown>();
  private uploads = new Map<string, Promise<void>>();
  private downloads = new Set<string>();
  private active = true;
  private ready = false;
  private applying = false;
  private uploadFailed = false;
  private selection = '';
  private latest?: RoomStatus;

  constructor(
    private runtime: AppRuntime,
    private boardId: string,
    private spaceId: string,
    private editor: ExcalidrawImperativeAPI,
    private store: WhiteboardFileStore,
    private notify: (status: RoomStatus) => void,
    private error: (message: string) => void,
  ) {
    this.room = new EditingRoom(
      runtime,
      '/v1/collaboration/' + encodeURIComponent(boardId) + '/session',
      (status) => {
        this.latest = status;
        if (status.ready && !this.ready) {
          this.ready = true;
          this.renderScene();
          this.downloadFiles();
        }
        this.report();
      },
    );
    const refresh = () => {
      if (this.ready && !this.applying) this.renderScene();
    };
    this.room.doc.getMap('elements').observe(refresh);
    this.room.doc.getMap('settings').observe(refresh);
    this.room.doc.getMap('files').observe(() => {
      if (this.ready) this.downloadFiles();
    });
    this.room.awareness.on('change', () => {
      if (!this.active) return;
      const collaborators = new Map();
      for (const [id, state] of this.room.awareness.getStates()) {
        if (id === this.room.doc.clientID) continue;
        collaborators.set(String(id), {
          username: state.user?.name,
          color: { background: state.user?.color, stroke: state.user?.color },
          pointer: state.pointer,
          button: state.button,
          selectedElementIds: state.selectedElementIds,
        });
      }
      editor.updateScene({ collaborators });
    });
    this.room.start();
  }

  private report() {
    if (!this.active || !this.latest) return;
    const pending = this.uploads.size > 0 || this.uploadFailed;
    const terminal = ['replaced', 'revoked', 'rejected'].includes(
      this.latest.phase,
    );
    this.notify(
      pending && !terminal
        ? {
            ...this.latest,
            dirty: true,
            phase: this.uploadFailed ? 'failed' : 'saving',
          }
        : this.latest,
    );
  }

  private renderScene() {
    const winners = new Map<string, Candidate & { key: string }>();
    for (const [key, value] of this.room.doc.getMap<string>('elements')) {
      const candidate: Candidate = JSON.parse(value);
      const old = winners.get(candidate.element.id);
      if (
        !old ||
        (rank(candidate.element, old.element) || compare(key, old.key)) > 0
      )
        winners.set(candidate.element.id, { ...candidate, key });
    }
    const ordered = [...winners.values()].sort(
      (a, b) =>
        compare(a.element.index ?? '', b.element.index ?? '') ||
        a.pos - b.pos ||
        compare(a.element.id, b.element.id),
    );
    // Keep a local image on the canvas until its upload can publish the element and storage reference together.
    const pendingImages = this.editor
      .getSceneElementsIncludingDeleted()
      .filter(
        (e) =>
          e.type === 'image' &&
          e.fileId &&
          this.uploads.has(e.fileId) &&
          !winners.has(e.id),
      );
    const elements = [...ordered.map((c) => c.element), ...pendingImages];
    const appState: Record<string, unknown> = {};
    for (const key of settings) {
      const value = this.room.doc.getMap<string>('settings').get(key);
      if (value !== undefined) appState[key] = JSON.parse(value);
      this.localSettings.set(
        key,
        value === undefined ? this.editor.getAppState()[key] : appState[key],
      );
    }
    this.seen = new Map(ordered.map(({ element }) => [element.id, clockOf(element)]));
    this.applying = true;
    try {
      this.editor.updateScene({
        elements,
        appState: { ...this.editor.getAppState(), ...appState },
        captureUpdate: CaptureUpdateAction.NEVER,
      });
    } finally {
      this.applying = false;
    }
  }

  private downloadFiles() {
    const files = this.room.doc.getMap<string>('files');
    for (const [key, value] of files) {
      const ref = JSON.parse(value);
      this.store.stored.set(key, ref);
      if (this.editor.getFiles()[key] || this.downloads.has(key)) continue;
      this.downloads.add(key);
      void hydrateSceneFiles(
        { elements: [], appState: {}, files: { [key]: ref } },
        { fileApi: this.runtime.fileApi, boardId: this.boardId },
      )
        .then((result) => {
          if (this.active && files.get(key) === value)
            this.editor.addFiles(
              Object.values(result.files) as unknown as BinaryFileData[],
            );
        })
        .finally(() => this.downloads.delete(key));
    }
  }
  private upload(key: string, files: BinaryFiles) {
    if (this.uploads.has(key)) return;
    const job = storeSceneFiles(
      { elements: [], appState: {}, files: { [key]: files[key] } },
      this.store,
      {
        api: this.runtime.api,
        fileApi: this.runtime.fileApi,
        boardId: this.boardId,
        spaceId: this.spaceId,
      },
    )
      .then(() => {
        this.uploads.delete(key);
        if (this.active)
          this.change({
            elements: this.editor.getSceneElementsIncludingDeleted(),
            appState: this.editor.getAppState(),
            files: this.editor.getFiles(),
          });
      })
      .catch((e) => {
        this.uploadFailed = true;
        if (this.active) this.error(String(e));
      })
      .finally(() => {
        this.uploads.delete(key);
        this.report();
      });
    this.uploads.set(key, job);
    this.report();
  }
  change({ elements, appState, files }: BoardChange) {
    if (!this.ready || !this.active || this.applying) return;
    const selection = JSON.stringify(appState.selectedElementIds);
    if (this.selection !== selection) {
      this.selection = selection;
      const ids = Object.keys(appState.selectedElementIds).slice(0, 96);
      this.room.awareness.setLocalStateField(
        'selectedElementIds',
        Object.fromEntries(ids.map((id) => [id, true])),
      );
    }
    const doc = this.room.doc;
    this.applying = true;
    try {
      doc.transact(() => {
        for (const [pos, element] of elements.entries()) {
          const old = this.seen.get(element.id);
          if (old && rank(element, old) <= 0) continue;
          if (
            element.type === 'image' &&
            element.fileId &&
            !element.isDeleted
          ) {
            const id = element.fileId;
            const ref = this.store.stored.get(id);
            if (!ref) {
              if (files[id] && !this.uploadFailed) this.upload(id, files);
              continue;
            }
            if (!doc.getMap('files').has(id))
              doc.getMap('files').set(id, JSON.stringify(ref));
          }
          doc
            .getMap('elements')
            .set(
              doc.clientID + ':' + element.id,
              JSON.stringify({ pos, element }),
            );
          this.seen.set(element.id, clockOf(element));
        }
        for (const key of settings) {
          if (
            appState[key] === undefined ||
            this.localSettings.get(key) === appState[key]
          )
            continue;
          doc.getMap('settings').set(key, JSON.stringify(appState[key]));
          this.localSettings.set(key, appState[key]);
        }
      });
    } finally {
      this.applying = false;
    }
  }
  pointer: NonNullable<ExcalidrawProps['onPointerUpdate']> = ({
    pointer,
    button,
  }) => {
    this.room.awareness.setLocalState({
      ...this.room.awareness.getLocalState(),
      pointer,
      button,
    });
  };
  async end() {
    if (
      this.latest?.phase === 'revoked' ||
      this.latest?.phase === 'replaced' ||
      this.latest?.phase === 'rejected'
    )
      return;
    if (!this.ready && !this.latest?.dirty) return;
    if (this.uploadFailed) {
      this.uploadFailed = false;
      this.change({
        elements: this.editor.getSceneElementsIncludingDeleted(),
        appState: this.editor.getAppState(),
        files: this.editor.getFiles(),
      });
    }
    await Promise.all([...this.uploads.values()]);
    if (this.uploadFailed) throw new Error('image_upload_failed');
    await this.room.end();
  }
  destroy() {
    this.active = false;
    this.room.destroy();
    this.editor.updateScene({ collaborators: new Map() });
  }
}
