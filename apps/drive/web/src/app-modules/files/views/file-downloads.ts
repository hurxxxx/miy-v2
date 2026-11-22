import type { AppRuntime } from '@miy/sdk';

import { i18n } from '../../../platform/i18n';
import { FilesApiError, type FileDownloadResponse } from '../api/files-api';

/** Pause between downloads so browsers keep them as separate files. */
export const DOWNLOAD_SPACING_MS = 400;

/**
 * AI-DO opened a signed URL (`openDownloadUrl`) or saved a ZIP Blob. In the
 * frame a download is a five-minute platform link the backend issues after
 * its ACL check, started with `runtime.download(url)`: the browser streams
 * the bytes, nothing is held in memory and the platform asks the app again
 * before sending them. Each file gets a fresh link just before it starts.
 */
export async function startDownloads(
  runtime: Pick<AppRuntime, 'download'>,
  fileIds: readonly string[],
  getLink: (fileId: string) => Promise<FileDownloadResponse>,
  wait: (ms: number) => Promise<void> = (ms) =>
    new Promise((resolve) => setTimeout(resolve, ms)),
): Promise<number> {
  const download = runtime.download;
  if (!download) {
    throw new FilesApiError(0, i18n.t('apps:drive.download.unavailable'));
  }
  let started = 0;
  for (const [index, fileId] of fileIds.entries()) {
    if (index > 0) {
      await wait(DOWNLOAD_SPACING_MS);
    }
    const link = await getLink(fileId);
    download(link.url);
    started += 1;
  }
  return started;
}
