import type { ExcalidrawProps } from '@excalidraw/excalidraw/types';

import {
  normalizeWhiteboardScene,
  type WhiteboardScene,
} from '../api/whiteboard-api';

/**
 * The scene helpers of AI-DO views/whiteboard-collab-scene.ts without the Yjs
 * maps (collaboration is not ported). One miy change: a scene's signature
 * names each image by its Excalidraw file ID and type only. The ID is a hash
 * of the content, and the bytes are stored apart (files.rs on the server),
 * so the editor's data URL and the stored entry's `storage` row give the same
 * signature.
 */
type ExcalidrawOnChange = NonNullable<ExcalidrawProps['onChange']>;
type ExcalidrawSceneSerializer = (
  elements: Parameters<ExcalidrawOnChange>[0],
  appState: Parameters<ExcalidrawOnChange>[1],
  files: Parameters<ExcalidrawOnChange>[2],
) => string;

const PERSISTED_APP_STATE_KEYS = new Set([
  'gridModeEnabled',
  'gridSize',
  'viewBackgroundColor',
]);

export type WhiteboardElement = Record<string, unknown> & {
  id?: string;
  version?: number;
  versionNonce?: number;
  updated?: number;
  isDeleted?: boolean;
};

export function sceneFromExcalidraw(
  elements: Parameters<ExcalidrawOnChange>[0],
  appState: Parameters<ExcalidrawOnChange>[1],
  files: Parameters<ExcalidrawOnChange>[2],
  serializeScene: ExcalidrawSceneSerializer,
): WhiteboardScene {
  const serialized = normalizeWhiteboardScene(
    JSON.parse(serializeScene(elements, appState, files)),
  );
  return sceneForPersistence({
    elements: cloneValue([...elements]),
    appState: serialized.appState,
    files: serialized.files,
  });
}

export function sceneSignature(scene: WhiteboardScene): string {
  const persisted = sceneForPersistence(scene);
  return JSON.stringify(
    stableJsonValue({ ...persisted, files: fileIdentities(persisted.files) }),
  );
}

export function storedSceneSignature(scene: unknown): string {
  return JSON.stringify(stableJsonValue(sceneForPersistence(scene)));
}

export function sceneForPersistence(scene: unknown): WhiteboardScene {
  const normalized = normalizeWhiteboardScene(scene);
  return {
    elements: normalized.elements,
    appState: appStateForPersistence(normalized.appState),
    files: normalized.files,
  };
}

export function selectedElementCount(
  elements: readonly unknown[],
  selectedElementIds: unknown,
): number {
  if (!selectedElementIds || typeof selectedElementIds !== 'object') return 0;
  let count = 0;
  for (const element of elements) {
    const id = elementId(element);
    if (!id) continue;
    if (
      (selectedElementIds as Record<string, unknown>)[id] === true &&
      !(element as WhiteboardElement).isDeleted
    ) {
      count += 1;
    }
  }
  return count;
}

export function elementId(element: unknown): string | null {
  if (!element || typeof element !== 'object') return null;
  const id = (element as WhiteboardElement).id;
  return typeof id === 'string' && id ? id : null;
}

function fileIdentities(
  files: Record<string, unknown>,
): Record<string, unknown> {
  const identities: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(files)) {
    const file =
      value && typeof value === 'object'
        ? (value as Record<string, unknown>)
        : {};
    identities[key] = { id: file.id ?? key, mimeType: file.mimeType ?? null };
  }
  return identities;
}

function cloneValue<T>(value: T): T {
  if (value === undefined || value === null) {
    return value;
  }
  return JSON.parse(JSON.stringify(value)) as T;
}

function stableJsonValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(stableJsonValue);
  }
  if (!value || typeof value !== 'object') {
    return value;
  }
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .filter(([, entry]) => entry !== undefined)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, entry]) => [key, stableJsonValue(entry)]),
  );
}

function appStateForPersistence(
  appState: Record<string, unknown>,
): Record<string, unknown> {
  const persisted: Record<string, unknown> = {};
  for (const key of PERSISTED_APP_STATE_KEYS) {
    if (appState[key] !== undefined) {
      persisted[key] = appState[key];
    }
  }
  return persisted;
}
