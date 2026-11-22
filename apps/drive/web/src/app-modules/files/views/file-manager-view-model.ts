import { formatByteSize } from '../../../platform/format/byte-size';
import { formatDateTime } from '../../../platform/time/time-utils';

import type { FileItem } from '../api/files-api';

// AI-DO file-manager-view-model.ts without the RAG status presentation and
// polling (RAG is not ported).

export const IMAGE_PREVIEW_ZOOM_MIN = 0.5;
export const IMAGE_PREVIEW_ZOOM_MAX = 4;
export const IMAGE_PREVIEW_ZOOM_STEP = 0.25;

export function isPreviewableImageFile(file: FileItem): boolean {
  return file.content_type
    .split(';', 1)[0]
    .trim()
    .toLowerCase()
    .startsWith('image/');
}

export function clampImagePreviewZoom(value: number): number {
  return Math.min(
    IMAGE_PREVIEW_ZOOM_MAX,
    Math.max(IMAGE_PREVIEW_ZOOM_MIN, value),
  );
}

export function formatFileSize(bytes: number): string {
  return formatByteSize(bytes, {
    fractionDigits: 'compact',
    units: ['KB', 'MB', 'GB', 'TB'],
  });
}

export function formatFileUpdatedAt(
  value: string,
  locale: string,
  timeZone: string,
): string {
  return formatDateTime(value, {
    dateStyle: 'medium',
    fallback: value,
    locale,
    timeStyle: 'short',
    timeZone,
  });
}
