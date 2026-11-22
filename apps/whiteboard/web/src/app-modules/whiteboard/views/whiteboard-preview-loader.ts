import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  getWhiteboard,
  normalizeWhiteboardScene,
  type FileApi,
  type WhiteboardDetail,
  type WhiteboardHubItem,
} from '../api/whiteboard-api';
import { hydrateSceneFiles } from '../api/whiteboard-files';
import { WHITEBOARD_PREVIEW_BACKGROUND_COLOR } from './whiteboard-colors';

/**
 * AI-DO views/whiteboard-preview-loader.ts. The loader takes the app's API
 * clients instead of the bearer token, and the default renderer downloads the
 * board's stored images first (miy keeps them out of the scene).
 */
export interface WhiteboardPreviewClient {
  api: AppApi | null | undefined;
  fileApi: FileApi;
}

export type WhiteboardPreviewStatus =
  | 'idle'
  | 'loading'
  | 'ready'
  | 'empty'
  | 'error';

export interface WhiteboardPreviewState {
  sourceKey: string;
  status: WhiteboardPreviewStatus;
  previewUrl: string | null;
}

export type WhiteboardPreviewAction =
  | { type: 'previewVisible'; sourceKey: string }
  | { type: 'previewLoaded'; sourceKey: string; previewUrl: string | null }
  | { type: 'previewFailed'; sourceKey: string };

export interface WhiteboardPreviewLoader {
  load(
    client: WhiteboardPreviewClient,
    item: WhiteboardHubItem,
    spaceId?: string | null,
  ): Promise<string | null>;
}

export interface CreateWhiteboardPreviewLoaderOptions {
  fetchWhiteboard: typeof getWhiteboard;
  renderScenePreview: (
    scene: WhiteboardDetail['scene'],
    context: { fileApi: FileApi; boardId: string },
  ) => Promise<string | null>;
  cacheKey?: (item: Pick<WhiteboardHubItem, 'id' | 'updated_at'>) => string;
}

export function getWhiteboardPreviewSourceKey(
  item: Pick<WhiteboardHubItem, 'id' | 'updated_at'>,
): string {
  return `${item.id}:${item.updated_at}`;
}

export function createWhiteboardPreviewState(
  sourceKey: string,
): WhiteboardPreviewState {
  return {
    sourceKey,
    status: 'idle',
    previewUrl: null,
  };
}

export function whiteboardPreviewReducer(
  state: WhiteboardPreviewState,
  action: WhiteboardPreviewAction,
): WhiteboardPreviewState {
  const currentState =
    state.sourceKey === action.sourceKey
      ? state
      : createWhiteboardPreviewState(action.sourceKey);

  switch (action.type) {
    case 'previewVisible':
      return currentState.status === 'idle'
        ? { ...currentState, status: 'loading' }
        : currentState;
    case 'previewLoaded':
      return {
        ...currentState,
        status: action.previewUrl ? 'ready' : 'empty',
        previewUrl: action.previewUrl,
      };
    case 'previewFailed':
      return {
        ...currentState,
        status: 'error',
        previewUrl: null,
      };
    default:
      return currentState;
  }
}

function isDeletedElement(element: unknown): boolean {
  return Boolean(
    element &&
      typeof element === 'object' &&
      (element as { isDeleted?: unknown }).isDeleted,
  );
}

export async function renderWhiteboardScenePreview(
  scene: WhiteboardDetail['scene'],
  context?: { fileApi: FileApi; boardId: string },
): Promise<string | null> {
  const normalized = normalizeWhiteboardScene(scene);
  const elements = normalized.elements.filter(
    (element) => !isDeletedElement(element),
  );
  if (elements.length === 0) return null;
  const files =
    context && Object.keys(normalized.files).length > 0
      ? (await hydrateSceneFiles(normalized, context)).files
      : normalized.files;

  const { exportToBlob } = await import('@excalidraw/excalidraw');
  const blob = await exportToBlob({
    elements: elements as never,
    appState: {
      ...normalized.appState,
      exportBackground: true,
      viewBackgroundColor: WHITEBOARD_PREVIEW_BACKGROUND_COLOR,
    } as never,
    files: files as never,
    mimeType: 'image/png',
    exportPadding: 32,
    maxWidthOrHeight: 720,
  });
  return URL.createObjectURL(blob);
}

export function createWhiteboardPreviewLoader(
  options: CreateWhiteboardPreviewLoaderOptions,
): WhiteboardPreviewLoader {
  const previewCache = new Map<string, string | null>();
  const previewPromiseCache = new Map<string, Promise<string | null>>();
  const cacheKey = options.cacheKey ?? getWhiteboardPreviewSourceKey;

  return {
    load(client, item, spaceId) {
      const key = cacheKey(item);
      if (previewCache.has(key)) {
        return Promise.resolve(previewCache.get(key) ?? null);
      }

      const existing = previewPromiseCache.get(key);
      if (existing) return existing;

      const promise = options
        .fetchWhiteboard(client.api, item.id, spaceId)
        .then((detail) =>
          options.renderScenePreview(detail.scene, {
            fileApi: client.fileApi,
            boardId: item.id,
          }),
        )
        .then((url) => {
          previewCache.set(key, url);
          return url;
        })
        .finally(() => {
          previewPromiseCache.delete(key);
        });

      previewPromiseCache.set(key, promise);
      return promise;
    },
  };
}

export const whiteboardPreviewLoader = createWhiteboardPreviewLoader({
  fetchWhiteboard: getWhiteboard,
  renderScenePreview: renderWhiteboardScenePreview,
});
