import { useCallback, useEffect, useMemo, useRef } from 'react';
import { requestId } from '@miy/ui';

import { ApiRequestError, apiFetchJson } from '../api/client';
import { i18n } from '../i18n';
import { useRuntime } from '../runtime/runtime-context';

/**
 * AI-DO apps/web/src/platform/media/use-media-upload.ts for miy. AI-DO posted
 * the image to `/api/v1/media/upload` and resolved `media:<id>` to a signed
 * proxy URL. Here the app reserves a core file (`POST /v1/media/upload`),
 * streams the bytes through the platform file gateway
 * (`runtime.fileApi('/media/<id>/content', { method: 'PUT' })`), and shows an
 * image by downloading it in the frame into a local Blob URL. The returned
 * value is AI-DO's `media:<id>` reference. `linkUploadsToPage` is AI-DO's
 * `createLinkedUploadFile` link step (`POST /v1/media/link`, resource
 * `docs_native_page`); the page save links the same images again.
 */
export interface MediaUploadResponse {
  id: string;
  url: string;
  status: string;
}

export function useMediaUpload() {
  const runtime = useRuntime();
  const { api, fileApi } = runtime;
  const requestIds = useRef(new WeakMap<File, string>());

  const uploadFile = useCallback(
    async (file: File): Promise<string> => {
      const id = requestIds.current.get(file) ?? requestId();
      requestIds.current.set(file, id);
      let reserved: MediaUploadResponse | undefined;
      const upload = async () => {
        reserved ??= await apiFetchJson<MediaUploadResponse>(
          '/v1/media/upload',
          api,
          {
            method: 'POST',
            body: JSON.stringify({
              requestId: id,
              name: file.name || 'image',
              mime: file.type,
              size: file.size,
            }),
          },
        );
        if (reserved.status === 'ready' || reserved.status === 'attached') {
          return reserved.url;
        }
        const response = await fileApi(
          `/media/${encodeURIComponent(reserved.id)}/content`,
          {
            method: 'PUT',
            body: file,
            headers: { 'Content-Type': 'application/octet-stream' },
          },
        );
        if (!response.ok) {
          throw new ApiRequestError(
            response.status,
            i18n.t('apps:docsApp.imageUploadFailed'),
          );
        }
        return reserved.url;
      };

      try {
        return await upload();
      } catch (error) {
        if ((error instanceof Error && error.name === 'AbortError') ||
            (error instanceof ApiRequestError && error.status > 0 && error.status < 500)) {
          throw error;
        }
        // Replay an uncertain reservation or transfer against the original
        // request. The server returns the state even after storing the bytes.
        return upload();
      }
    },
    [api, fileApi],
  );

  // miy: one Blob URL per image while this screen lives (the editor asks
  // again on every render); all are revoked when it goes away.
  const blobUrls = useRef(new Map<string, Promise<string>>());
  useEffect(() => {
    const urls = blobUrls.current;
    return () => {
      for (const pending of urls.values()) {
        void pending.then((value) => {
          if (value.startsWith('blob:')) URL.revokeObjectURL(value);
        });
      }
      urls.clear();
    };
  }, [fileApi]);

  const resolveFileUrl = useCallback(
    (url: string): Promise<string> => {
      if (!url.startsWith('media:')) return Promise.resolve(url);
      const cached = blobUrls.current.get(url);
      if (cached) return cached;
      const id = url.slice('media:'.length);
      const pending = (async () => {
        try {
          const response = await fileApi(
            `/media/${encodeURIComponent(id)}/content`,
          );
          if (!response.ok) {
            blobUrls.current.delete(url);
            return url;
          }
          return URL.createObjectURL(await response.blob());
        } catch {
          blobUrls.current.delete(url);
          return url;
        }
      })();
      blobUrls.current.set(url, pending);
      return pending;
    },
    [fileApi],
  );

  const linkUploadsToPage = useCallback(
    async (urls: string[], pageId: string): Promise<void> => {
      const mediaIds = urls
        .filter((url) => url.startsWith('media:'))
        .map((url) => url.slice('media:'.length));
      if (!mediaIds.length) return;
      await apiFetchJson<void>('/v1/media/link', api, {
        method: 'POST',
        body: JSON.stringify({
          media_ids: mediaIds,
          resource_type: 'docs_native_page',
          resource_id: pageId,
        }),
      }).catch(() => undefined);
    },
    [api],
  );

  return useMemo(
    () => ({ uploadFile, resolveFileUrl, linkUploadsToPage }),
    [linkUploadsToPage, resolveFileUrl, uploadFile],
  );
}
