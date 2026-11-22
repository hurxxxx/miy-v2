import type {
  DriveClient,
  FileUploadProgress,
  FileVisibility,
} from './api/files-api';
import type { UploadHandle } from './api/file-transfer';

/**
 * AI-DO apps/web/src/app-modules/files/file-upload-session.ts. AI-DO's
 * window events, records and batch runner are kept; miy changes:
 * - `token`/`workspaceSlug` become the frame client and the drive space;
 * - a batch remembers each file's upload handle (request and upload IDs) so
 *   a failed file is sent again with the same request instead of a new one,
 *   and the batch continues from it (`startIndex`, `resume`);
 * - an aborted batch reports `cancelled` (the panel's cancel button).
 */
export const FILES_CHANGED_EVENT = 'files:changed';
export const FILES_UPLOAD_COMPLETED_EVENT = 'files:upload-completed';

export type FileUploadStatus = 'uploading' | 'succeeded' | 'failed' | 'cancelled';

export interface FileUploadRecord {
  id: string;
  currentFileIndex: number;
  currentFileName: string;
  errorMessage?: string;
  loadedBytes: number;
  percent: number;
  status: FileUploadStatus;
  totalBytes: number;
  totalFiles: number;
  updatedAt: number;
}

export interface StartFileUploadInput {
  files: File[];
  folderId: string | null;
  client: DriveClient;
  uploadVisibility: FileVisibility;
  spaceId: string;
  /** miy: a new version of this file (one file). */
  fileId?: string | null;
}

export type FileUploadProgressSnapshot = {
  fileIndex: number;
  fileName: string;
  loadedBytes: number;
  percent: number;
};

export type FileUploadEventDescriptor = {
  detail: {
    folderId: string | null;
    spaceId: string;
  };
  name: typeof FILES_CHANGED_EVENT | typeof FILES_UPLOAD_COMPLETED_EVENT;
};

export type FileUploadFileAdapter = (input: {
  file: File;
  folderId: string | null;
  onProgress: (progress: FileUploadProgress) => void;
  onHandle: (handle: UploadHandle) => void;
  resume: UploadHandle | null;
  signal: AbortSignal;
  client: DriveClient;
  visibility: FileVisibility;
  spaceId: string;
}) => Promise<unknown>;

export type FileUploadBatchResult =
  | { ok: true }
  | { cancelled: true; failedIndex: number; ok: false }
  | { errorMessage: string; failedIndex: number; ok: false };

export function canStartFileUpload({
  hasActiveUpload,
  files,
}: {
  files: readonly File[];
  hasActiveUpload: boolean;
}): boolean {
  return !hasActiveUpload && files.length > 0;
}

export function createFileUploadId({
  now,
  sequence,
}: {
  now: number;
  sequence: number;
}): string {
  return `file-upload-${now}-${sequence}`;
}

export function createFileUploadRecord({
  files,
  id,
  now,
}: {
  files: readonly File[];
  id: string;
  now: number;
}): FileUploadRecord {
  return {
    id,
    currentFileIndex: 1,
    currentFileName: files[0]?.name ?? '',
    loadedBytes: 0,
    percent: 0,
    status: 'uploading',
    totalBytes: getTotalUploadBytes(files),
    totalFiles: files.length,
    updatedAt: now,
  };
}

export function dismissFileUploadRecord(
  uploads: readonly FileUploadRecord[],
  uploadId: string,
): FileUploadRecord[] {
  return uploads.filter(
    (upload) => upload.id !== uploadId || upload.status === 'uploading',
  );
}

export function applyFileUploadProgress({
  now,
  progress,
  upload,
}: {
  now: number;
  progress: FileUploadProgressSnapshot;
  upload: FileUploadRecord;
}): FileUploadRecord {
  return {
    ...upload,
    currentFileIndex: progress.fileIndex,
    currentFileName: progress.fileName,
    loadedBytes: progress.loadedBytes,
    percent: progress.percent,
    updatedAt: now,
  };
}

export function completeFileUploadRecord({
  now,
  upload,
}: {
  now: number;
  upload: FileUploadRecord;
}): FileUploadRecord {
  return {
    ...upload,
    errorMessage: undefined,
    loadedBytes: upload.totalBytes,
    percent: 100,
    status: 'succeeded',
    updatedAt: now,
  };
}

export function failFileUploadRecord({
  errorMessage,
  now,
  upload,
}: {
  errorMessage: string;
  now: number;
  upload: FileUploadRecord;
}): FileUploadRecord {
  return {
    ...upload,
    errorMessage,
    status: 'failed',
    updatedAt: now,
  };
}

/** miy: the person cancelled the upload. */
export function cancelFileUploadRecord({
  now,
  upload,
}: {
  now: number;
  upload: FileUploadRecord;
}): FileUploadRecord {
  return { ...upload, errorMessage: undefined, status: 'cancelled', updatedAt: now };
}

/** miy: a failed upload starts again from its failed file. */
export function retryFileUploadRecord({
  now,
  upload,
}: {
  now: number;
  upload: FileUploadRecord;
}): FileUploadRecord {
  return { ...upload, errorMessage: undefined, status: 'uploading', updatedAt: now };
}

export function getFileUploadSuccessEvents({
  folderId,
  spaceId,
}: {
  folderId: string | null;
  spaceId: string;
}): FileUploadEventDescriptor[] {
  const detail = { folderId, spaceId };
  return [
    { detail, name: FILES_UPLOAD_COMPLETED_EVENT },
    { detail, name: FILES_CHANGED_EVENT },
  ];
}

export async function runFileUploadBatch({
  client,
  fallbackErrorMessage,
  files,
  folderId,
  isAbort = (error) =>
    error instanceof DOMException && error.name === 'AbortError',
  onHandle = () => undefined,
  onProgress,
  resume = null,
  signal,
  spaceId,
  startIndex = 0,
  uploadFile,
  uploadVisibility,
}: {
  client: DriveClient;
  fallbackErrorMessage: string;
  files: File[];
  folderId: string | null;
  isAbort?: (error: unknown) => boolean;
  onHandle?: (index: number, handle: UploadHandle) => void;
  onProgress: (progress: FileUploadProgressSnapshot) => void;
  resume?: UploadHandle | null;
  signal: AbortSignal;
  spaceId: string;
  startIndex?: number;
  uploadFile: FileUploadFileAdapter;
  uploadVisibility: FileVisibility;
}): Promise<FileUploadBatchResult> {
  const totalBytes = getTotalUploadBytes(files);
  let completedBytes = getTotalUploadBytes(files.slice(0, startIndex));
  let index = startIndex;

  try {
    for (; index < files.length; index += 1) {
      const file = files[index];
      const completedBytesBeforeFile = completedBytes;
      const fileIndex = index;
      await uploadFile({
        client,
        file,
        folderId,
        onHandle: (handle) => onHandle(fileIndex, handle),
        onProgress: (progress) => {
          onProgress(
            buildFileUploadProgressSnapshot({
              completedBytes: completedBytesBeforeFile,
              file,
              fileIndex: fileIndex + 1,
              progress,
              totalBytes,
            }),
          );
        },
        resume: index === startIndex ? resume : null,
        signal,
        spaceId,
        visibility: uploadVisibility,
      });
      completedBytes += file.size;
      onProgress({
        fileIndex: index + 1,
        fileName: file.name,
        loadedBytes: Math.min(totalBytes, completedBytes),
        percent:
          totalBytes > 0
            ? Math.round(
                (Math.min(totalBytes, completedBytes) / totalBytes) * 100,
              )
            : 100,
      });
    }
    return { ok: true };
  } catch (caughtError) {
    if (signal.aborted || isAbort(caughtError)) {
      return { cancelled: true, failedIndex: index, ok: false };
    }
    return {
      errorMessage:
        caughtError instanceof Error
          ? caughtError.message
          : fallbackErrorMessage,
      failedIndex: index,
      ok: false,
    };
  }
}

export function buildFileUploadProgressSnapshot({
  completedBytes,
  file,
  fileIndex,
  progress,
  totalBytes,
}: {
  completedBytes: number;
  file: File;
  fileIndex: number;
  progress: FileUploadProgress;
  totalBytes: number;
}): FileUploadProgressSnapshot {
  const currentLoaded =
    progress.total > 0
      ? Math.min(progress.loaded, progress.total)
      : Math.min(progress.loaded, file.size);
  const loadedBytes = Math.min(totalBytes, completedBytes + currentLoaded);
  return {
    fileIndex,
    fileName: file.name,
    loadedBytes,
    percent: totalBytes > 0 ? Math.round((loadedBytes / totalBytes) * 100) : 0,
  };
}

function getTotalUploadBytes(files: readonly File[]): number {
  return files.reduce((total, file) => total + file.size, 0);
}
