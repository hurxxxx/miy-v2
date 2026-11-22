import { describe, expect, it } from 'vitest';

import type { FileItem } from '../api/files-api';
import {
  clampImagePreviewZoom,
  formatFileSize,
  isPreviewableImageFile,
} from './file-manager-view-model';

function file(contentType: string): FileItem {
  return {
    id: 'file-1',
    folder_id: null,
    filename: 'file',
    content_type: contentType,
    size_bytes: 1024,
    visibility: 'workspace',
    owner_id: 'owner',
    owner_name: 'Owner',
    can_delete: true,
    version_number: 1,
    version_count: 1,
    created_at: '2026-05-30T00:00:00Z',
    updated_at: '2026-05-30T00:00:00Z',
  };
}

describe('file manager view model', () => {
  it('detects previewable image content types with parameters', () => {
    expect(isPreviewableImageFile(file('image/png; charset=binary'))).toBe(
      true,
    );
    expect(isPreviewableImageFile(file('IMAGE/JPEG'))).toBe(true);
    expect(isPreviewableImageFile(file('application/pdf'))).toBe(false);
  });

  it('clamps image preview zoom to the supported range', () => {
    expect(clampImagePreviewZoom(0.1)).toBe(0.5);
    expect(clampImagePreviewZoom(1.75)).toBe(1.75);
    expect(clampImagePreviewZoom(9)).toBe(4);
  });

  it('formats file sizes across byte units', () => {
    expect(formatFileSize(512)).toBe('512 B');
    expect(formatFileSize(1023)).toBe('1023 B');
    expect(formatFileSize(1024)).toBe('1.0 KB');
    expect(formatFileSize(1536)).toBe('1.5 KB');
    expect(formatFileSize(12 * 1024)).toBe('12 KB');
    expect(formatFileSize(1024 * 1024)).toBe('1.0 MB');
    expect(formatFileSize(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatFileSize(1024 ** 4)).toBe('1.0 TB');
  });
});
