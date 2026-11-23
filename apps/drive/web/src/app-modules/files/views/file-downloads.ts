import { isApiError, type AppRuntime } from '@miy/sdk';

import { i18n } from '../../../platform/i18n';
import { FilesApiError, type FileDownloadResponse } from '../api/files-api';

export async function startDownload(
  runtime: Pick<AppRuntime, 'download'>,
  fileId: string,
  getLink: (fileId: string) => Promise<FileDownloadResponse>,
): Promise<void> {
  const download = runtime.download;
  if (!download) {
    throw new FilesApiError(0, i18n.t('apps:drive.download.unavailable'));
  }
  const link = await getLink(fileId);
  download(link.url);
}

export type DownloadEntry = { id: string; path: string; downloadUrl?: string };
export type DownloadBatch = { remaining: DownloadEntry[]; sent: number; total: number };

/** Save confirmed progress so callers can resume at the first unfinished file. */
export async function sendDownloadBatch(
  download: NonNullable<AppRuntime['downloadAndWait']>,
  batch: DownloadBatch,
  getLink: (fileId: string) => Promise<FileDownloadResponse>,
  signal: AbortSignal,
  saveProgress: (batch: DownloadBatch) => void,
  retryFirst = false,
): Promise<void> {
  let current = batch;
  for (const [index, entry] of batch.remaining.entries()) {
    signal.throwIfAborted();
    const checkOnly = entry.downloadUrl !== undefined && !(retryFirst && index === 0);
    let url = entry.downloadUrl;
    if (url === undefined) {
      const link = await getLink(entry.id);
      signal.throwIfAborted();
      url = link.url;
      current = {
        ...current,
        remaining: [{ ...entry, downloadUrl: url }, ...current.remaining.slice(1)],
      };
      saveProgress(current);
    }
    try {
      await download(url, signal, { checkOnly });
    } catch (error) {
      signal.throwIfAborted();
      if (isApiError(error) && (error.code === 'file_download_failed' || error.status === 404)) {
        current = {
          ...current,
          remaining: [{ id: entry.id, path: entry.path }, ...current.remaining.slice(1)],
        };
        saveProgress(current);
      }
      throw error;
    }
    signal.throwIfAborted();
    current = { ...current, remaining: current.remaining.slice(1), sent: current.sent + 1 };
    saveProgress(current);
  }
}
