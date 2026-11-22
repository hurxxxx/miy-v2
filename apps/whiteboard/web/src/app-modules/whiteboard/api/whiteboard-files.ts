import { requestId as newRequestId } from '@miy/ui';

import type { AppApi } from '../../../platform/runtime/runtime-context';

import {
  WhiteboardApiError,
  downloadWhiteboardFile,
  reserveWhiteboardFile,
  uploadWhiteboardFileContent,
  type FileApi,
  type WhiteboardScene,
} from './whiteboard-api';

/**
 * miy (보강): a board's images in the core storage instead of inline data URLs.
 *
 * In the editor an image is Excalidraw's `BinaryFileData` (`{id, mimeType,
 * dataURL, created}`). In the saved scene it is a stored entry `{id,
 * mimeType, created, storage: "<row id>"}`. Saving stores new images first
 * (reserve → upload, each repeatable with the same request ID), then saves the
 * scene with stored entries; loading downloads each stored image and gives
 * the editor its data URL again.
 */
export const STORABLE_IMAGE_TYPES = [
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
] as const;
export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

type FileEntry = Record<string, unknown>;

export class WhiteboardImageError extends Error {
  constructor(
    public readonly reason: 'unsupported' | 'too-large' | 'upload',
    message: string,
  ) {
    super(message);
  }
}

/** What a board's stored images are while it is open. */
export interface WhiteboardFileStore {
  /** Excalidraw file ID → stored entry (as saved). */
  stored: Map<string, FileEntry>;
  /** Excalidraw file ID → request ID of its reservation, reused on retry. */
  requests: Map<string, string>;
}

export function createWhiteboardFileStore(
  scene?: WhiteboardScene | null,
): WhiteboardFileStore {
  const stored = new Map<string, FileEntry>();
  for (const [key, value] of Object.entries(scene?.files ?? {})) {
    const entry = asEntry(value);
    if (entry && typeof entry.storage === 'string') {
      stored.set(key, storedEntry(key, entry, entry.storage));
    }
  }
  return { stored, requests: new Map() };
}

function asEntry(value: unknown): FileEntry | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as FileEntry)
    : null;
}

function storedEntry(key: string, file: FileEntry, storage: string): FileEntry {
  const entry: FileEntry = {
    id: typeof file.id === 'string' ? file.id : key,
    mimeType: file.mimeType,
    storage,
  };
  if (typeof file.created === 'number') entry.created = file.created;
  return entry;
}

export function dataUrlToBlob(dataURL: string): Blob {
  const match = /^data:([^;,]*)(;base64)?,(.*)$/s.exec(dataURL);
  if (!match) throw new WhiteboardImageError('unsupported', 'not a data URL');
  const mime = match[1] || 'application/octet-stream';
  if (!match[2]) {
    return new Blob([decodeURIComponent(match[3])], { type: mime });
  }
  const binary = atob(match[3]);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return new Blob([bytes], { type: mime });
}

export function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error('read failed'));
    reader.readAsDataURL(blob);
  });
}

/** SVG and other non-raster images are drawn once and stored as PNG. */
async function rasterize(blob: Blob): Promise<Blob> {
  const url = URL.createObjectURL(blob);
  try {
    const image = new Image();
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error('image decode failed'));
      image.src = url;
    });
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, image.naturalWidth || 1024);
    canvas.height = Math.max(1, image.naturalHeight || 768);
    const context = canvas.getContext('2d');
    if (!context) throw new Error('no canvas');
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const png = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, 'image/png'),
    );
    if (!png) throw new Error('no png');
    return png;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function extension(mime: string): string {
  return { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/gif': 'gif', 'image/webp': 'webp' }[mime] ?? 'png';
}

/**
 * The scene to save: every image as a stored entry. New images are stored
 * first; a failure keeps what was stored so far (the request IDs stay in the
 * store) and the next save continues with the same requests. Images still
 * referenced by live elements but missing from the editor (a download that
 * failed) keep their stored entry, so a save never drops them.
 */
export async function storeSceneFiles(
  scene: WhiteboardScene,
  store: WhiteboardFileStore,
  context: {
    api: AppApi | null | undefined;
    fileApi: FileApi;
    boardId: string;
    spaceId: string;
  },
): Promise<WhiteboardScene> {
  const files: Record<string, FileEntry> = {};
  for (const [key, value] of Object.entries(scene.files)) {
    const file = asEntry(value);
    if (!file) continue;
    if (typeof file.storage === 'string') {
      files[key] = storedEntry(key, file, file.storage);
      continue;
    }
    const known = store.stored.get(key);
    if (known) {
      files[key] = known;
      continue;
    }
    if (typeof file.dataURL !== 'string') continue;
    let blob = dataUrlToBlob(file.dataURL);
    if (!STORABLE_IMAGE_TYPES.includes(blob.type as (typeof STORABLE_IMAGE_TYPES)[number])) {
      try {
        blob = await rasterize(blob);
      } catch {
        throw new WhiteboardImageError('unsupported', 'unsupported image');
      }
    }
    if (blob.size > MAX_IMAGE_BYTES) {
      throw new WhiteboardImageError('too-large', 'image too large');
    }
    const request = store.requests.get(key) ?? newRequestId();
    store.requests.set(key, request);
    let reserved;
    try {
      reserved = await reserveWhiteboardFile(
        context.api,
        context.boardId,
        {
          request_id: request,
          file_id: key,
          name: `${key.slice(0, 40)}.${extension(blob.type)}`,
          mime: blob.type,
          size: blob.size,
        },
        context.spaceId,
      );
      if (reserved.status === 'pending') {
        await uploadWhiteboardFileContent(context.fileApi, context.boardId, reserved.id, blob);
      }
    } catch (error) {
      if (error instanceof WhiteboardApiError && error.status >= 400 && error.status < 500 && error.status !== 408) {
        // A definite refusal (type, size, access): a new request next time.
        store.requests.delete(key);
      }
      throw error;
    }
    const entry = storedEntry(key, { ...file, mimeType: blob.type }, reserved.id);
    store.stored.set(key, entry);
    store.requests.delete(key);
    files[key] = entry;
  }
  for (const element of scene.elements) {
    const item = asEntry(element);
    if (!item || item.type !== 'image' || item.isDeleted === true) continue;
    const fileId = typeof item.fileId === 'string' ? item.fileId : null;
    if (fileId && !files[fileId] && store.stored.has(fileId)) {
      files[fileId] = store.stored.get(fileId)!;
    }
  }
  return { ...scene, files };
}

/**
 * The editor's images of a saved scene: each stored entry downloaded and
 * given back its data URL. Images that could not be read are left out (the
 * editor draws a placeholder) and counted.
 */
export async function hydrateSceneFiles(
  scene: WhiteboardScene,
  context: { fileApi: FileApi; boardId: string; signal?: AbortSignal },
): Promise<{ files: Record<string, FileEntry>; failed: number }> {
  const files: Record<string, FileEntry> = {};
  let failed = 0;
  await Promise.all(
    Object.entries(scene.files).map(async ([key, value]) => {
      const file = asEntry(value);
      if (!file) return;
      if (typeof file.dataURL === 'string') {
        files[key] = file;
        return;
      }
      if (typeof file.storage !== 'string') return;
      try {
        const blob = await downloadWhiteboardFile(
          context.fileApi,
          context.boardId,
          file.storage,
          context.signal,
        );
        files[key] = {
          id: typeof file.id === 'string' ? file.id : key,
          mimeType: file.mimeType ?? blob.type,
          dataURL: await blobToDataUrl(blob),
          created: typeof file.created === 'number' ? file.created : Date.now(),
        };
      } catch {
        failed += 1;
      }
    }),
  );
  return { files, failed };
}
