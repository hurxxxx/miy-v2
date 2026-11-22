import { useCallback, useMemo } from 'react';
import { requestId } from '@miy/ui';

import { apiFetchJson } from '../api/client';
import { i18n } from '../i18n';
import { useRuntime } from '../runtime/runtime-context';

/**
 * AI-DO apps/web/src/platform/media/use-media-upload.ts for miy. AI-DO posted
 * the image to `/api/v1/media/upload` and resolved `media:<id>` to a signed
 * proxy URL. Here the app reserves a core file (`POST /v1/media/upload`),
 * streams the bytes through the platform file gateway
 * (`runtime.fileApi('/media/<id>/content', { method: 'PUT' })`), and shows an
 * image by downloading it in the frame into a local Blob URL. The returned
 * value is AI-DO's `media:<id>` reference.
 */
export interface MediaUploadResponse {
  id: string;
  url: string;
}

export function useMediaUpload() {
  const runtime = useRuntime();
  const { api, fileApi } = runtime;

  const uploadFile = useCallback(
    async (file: File): Promise<string> => {
      const reserved = await apiFetchJson<MediaUploadResponse>(
        '/v1/media/upload',
        api,
        {
          method: 'POST',
          body: JSON.stringify({
            requestId: requestId(),
            name: file.name || 'image',
            mime: file.type,
            size: file.size,
          }),
        },
      );
      const response = await fileApi(
        `/media/${encodeURIComponent(reserved.id)}/content`,
        {
          method: 'PUT',
          body: file,
          headers: { 'Content-Type': 'application/octet-stream' },
        },
      );
      if (!response.ok) {
        throw new Error(i18n.t('apps:community.editor.imageUploadFailed'));
      }
      return reserved.url; // "media:{id}"
    },
    [api, fileApi],
  );

  const resolveFileUrl = useCallback(
    async (url: string): Promise<string> => {
      if (!url.startsWith('media:')) return url;
      const id = url.slice('media:'.length);
      try {
        const response = await fileApi(
          `/media/${encodeURIComponent(id)}/content`,
        );
        if (!response.ok) return url;
        return URL.createObjectURL(await response.blob());
      } catch {
        return url;
      }
    },
    [fileApi],
  );

  return useMemo(
    () => ({ uploadFile, resolveFileUrl }),
    [resolveFileUrl, uploadFile],
  );
}
