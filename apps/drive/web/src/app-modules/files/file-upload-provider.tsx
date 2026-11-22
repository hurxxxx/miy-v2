import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlertCircle,
  Ban,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Loader2,
  RotateCcw,
  UploadCloud,
  X,
} from 'lucide-react';
import { useFeedback } from '@miy/ui';

import { cn } from '../../lib/utils';
import { formatByteSize } from '../../platform/format/byte-size';
import { useNavigationDirty } from '../../platform/runtime/navigation-state';
import { isAbortError } from './api/files-api';
import {
  cancelUpload,
  transferFile,
  type UploadHandle,
} from './api/file-transfer';
import {
  applyFileUploadProgress,
  canStartFileUpload,
  cancelFileUploadRecord,
  completeFileUploadRecord,
  createFileUploadId,
  createFileUploadRecord,
  dismissFileUploadRecord,
  failFileUploadRecord,
  getFileUploadSuccessEvents,
  retryFileUploadRecord,
  runFileUploadBatch,
  type FileUploadRecord,
  type StartFileUploadInput,
} from './file-upload-session';

/**
 * AI-DO apps/web/src/app-modules/files/file-upload-provider.tsx: one batch at
 * a time, a floating panel with per-batch progress, success/failure toasts
 * and window events for the views. miy additions (the platform file
 * transfer candidate's acceptance): cancel a running batch, and retry a
 * failed one from its failed file with the same upload request; dismissing
 * a failed batch releases that file's unfinished reservation. A running
 * upload is reported as unsaved work (runtime.setNavigationState).
 */
type BatchState = {
  input: StartFileUploadInput;
  files: File[];
  handles: Map<number, UploadHandle>;
  failedIndex: number;
  abort: AbortController | null;
};

interface FileUploadManagerContextValue {
  hasActiveUploads: boolean;
  startUpload: (input: StartFileUploadInput) => boolean;
}

function releaseHandle(batch: BatchState, index: number) {
  const handle = batch.handles.get(index);
  if (handle?.uploadId) {
    void cancelUpload(batch.input.client, handle.spaceId, handle.uploadId).catch(
      () => undefined,
    );
  }
}

const FileUploadManagerContext =
  createContext<FileUploadManagerContextValue | null>(null);
const FILE_UPLOAD_PROVIDER_REQUIRED_ERROR =
  'useFileUploadManager must be used within FileUploadProvider';

export function FileUploadProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation('apps');
  const feedback = useFeedback();
  const activeUploadRef = useRef(false);
  const batchesRef = useRef(new Map<string, BatchState>());
  const mountedRef = useRef(true);
  const sequenceRef = useRef(0);
  const [collapsed, setCollapsed] = useState(false);
  const [uploads, setUploads] = useState<FileUploadRecord[]>([]);
  const hasActiveUploads = uploads.some(
    (upload) => upload.status === 'uploading',
  );
  useNavigationDirty(hasActiveUploads);

  useEffect(() => {
    mountedRef.current = true;
    const batches = batchesRef.current;
    return () => {
      mountedRef.current = false;
      for (const batch of batches.values()) {
        batch.abort?.abort();
      }
      activeUploadRef.current = false;
    };
  }, []);

  const updateUpload = useCallback(
    (uploadId: string, change: (upload: FileUploadRecord) => FileUploadRecord) => {
      if (!mountedRef.current) return;
      setUploads((current) =>
        current.map((upload) => (upload.id === uploadId ? change(upload) : upload)),
      );
    },
    [],
  );

  const runBatch = useCallback(
    (uploadId: string, startIndex: number) => {
      const batch = batchesRef.current.get(uploadId);
      if (!batch) return;
      const abortController = new AbortController();
      batch.abort = abortController;
      activeUploadRef.current = true;
      const { input } = batch;
      void runFileUploadBatch({
        client: input.client,
        files: batch.files,
        fallbackErrorMessage: t('files.errors.uploadFailed'),
        folderId: input.folderId,
        isAbort: isAbortError,
        onHandle: (index, handle) => {
          batch.handles.set(index, handle);
        },
        onProgress: (progress) => {
          updateUpload(uploadId, (upload) =>
            applyFileUploadProgress({ now: Date.now(), progress, upload }),
          );
        },
        resume: batch.handles.get(startIndex) ?? null,
        signal: abortController.signal,
        spaceId: input.spaceId,
        startIndex,
        uploadFile: ({
          client,
          file,
          folderId,
          onHandle,
          onProgress,
          resume,
          signal,
          spaceId,
          visibility,
        }) =>
          transferFile(client, spaceId, file, {
            fileId: input.fileId ?? null,
            folderId,
            onHandle,
            onProgress,
            resume,
            signal,
            visibility,
          }),
        uploadVisibility: input.uploadVisibility,
      }).then((result) => {
        batch.abort = null;
        activeUploadRef.current = false;
        if (result.ok) {
          batchesRef.current.delete(uploadId);
          updateUpload(uploadId, (upload) =>
            completeFileUploadRecord({ now: Date.now(), upload }),
          );
          if (!mountedRef.current) return;
          for (const event of getFileUploadSuccessEvents({
            folderId: input.folderId,
            spaceId: input.spaceId,
          })) {
            window.dispatchEvent(
              new CustomEvent(event.name, { detail: event.detail }),
            );
          }
          feedback.success(
            t('files.uploadProgress.completedTitle'),
            t('files.uploadProgress.completedDescription', {
              count: batch.files.length,
            }),
          );
          return;
        }
        batch.failedIndex = result.failedIndex;
        if ('cancelled' in result) {
          // The unfinished file's reservation is released; finished files stay.
          releaseHandle(batch, result.failedIndex);
          batchesRef.current.delete(uploadId);
          updateUpload(uploadId, (upload) =>
            cancelFileUploadRecord({ now: Date.now(), upload }),
          );
          if (mountedRef.current && result.failedIndex > 0) {
            window.dispatchEvent(
              new CustomEvent('files:changed', {
                detail: { folderId: input.folderId, spaceId: input.spaceId },
              }),
            );
          }
          return;
        }
        updateUpload(uploadId, (upload) =>
          failFileUploadRecord({
            errorMessage: result.errorMessage,
            now: Date.now(),
            upload,
          }),
        );
        if (mountedRef.current) {
          feedback.error(
            t('files.uploadProgress.failedTitle'),
            result.errorMessage,
          );
        }
      });
    },
    [feedback, t, updateUpload],
  );

  const dismissUpload = useCallback((uploadId: string) => {
    const batch = batchesRef.current.get(uploadId);
    if (batch && !batch.abort) {
      releaseHandle(batch, batch.failedIndex);
      batchesRef.current.delete(uploadId);
    }
    setUploads((current) => dismissFileUploadRecord(current, uploadId));
  }, []);

  const cancelRunningUpload = useCallback((uploadId: string) => {
    batchesRef.current.get(uploadId)?.abort?.abort();
  }, []);

  const retryUpload = useCallback(
    (uploadId: string) => {
      const batch = batchesRef.current.get(uploadId);
      if (!batch || batch.abort || activeUploadRef.current) {
        if (activeUploadRef.current) {
          feedback.error(t('files.errors.uploadAlreadyRunning'));
        }
        return;
      }
      setCollapsed(false);
      updateUpload(uploadId, (upload) =>
        retryFileUploadRecord({ now: Date.now(), upload }),
      );
      runBatch(uploadId, batch.failedIndex);
    },
    [feedback, runBatch, t, updateUpload],
  );

  const startUpload = useCallback(
    (input: StartFileUploadInput) => {
      if (
        !canStartFileUpload({
          files: input.files,
          hasActiveUpload: activeUploadRef.current,
        })
      ) {
        return false;
      }

      const now = Date.now();
      sequenceRef.current += 1;
      const uploadId = createFileUploadId({
        now,
        sequence: sequenceRef.current,
      });
      const files = [...input.files];
      batchesRef.current.set(uploadId, {
        input,
        files,
        handles: new Map(),
        failedIndex: 0,
        abort: null,
      });
      setCollapsed(false);
      setUploads((current) => [
        ...current,
        createFileUploadRecord({ files, id: uploadId, now }),
      ]);
      runBatch(uploadId, 0);
      return true;
    },
    [runBatch],
  );

  const value = useMemo<FileUploadManagerContextValue>(
    () => ({
      hasActiveUploads,
      startUpload,
    }),
    [hasActiveUploads, startUpload],
  );

  return (
    <FileUploadManagerContext.Provider value={value}>
      {children}
      <FileUploadFloatingPanel
        collapsed={collapsed}
        onCancel={cancelRunningUpload}
        onDismiss={dismissUpload}
        onRetry={retryUpload}
        onToggleCollapsed={() => setCollapsed((current) => !current)}
        uploads={uploads}
      />
    </FileUploadManagerContext.Provider>
  );
}

export function useFileUploadManager(): FileUploadManagerContextValue {
  const context = use(FileUploadManagerContext);
  if (!context) {
    throw new Error(FILE_UPLOAD_PROVIDER_REQUIRED_ERROR);
  }
  return context;
}

function FileUploadFloatingPanel({
  collapsed,
  onCancel,
  onDismiss,
  onRetry,
  onToggleCollapsed,
  uploads,
}: {
  collapsed: boolean;
  onCancel: (uploadId: string) => void;
  onDismiss: (uploadId: string) => void;
  onRetry: (uploadId: string) => void;
  onToggleCollapsed: () => void;
  uploads: FileUploadRecord[];
}) {
  const { t } = useTranslation(['apps', 'common']);

  if (uploads.length === 0) {
    return null;
  }

  const activeCount = uploads.filter(
    (upload) => upload.status === 'uploading',
  ).length;

  return (
    <section
      aria-label={t('files.uploadProgress.regionLabel')}
      className="fixed bottom-24 right-4 z-30 w-[min(390px,calc(100vw-2rem))] overflow-hidden rounded-lg border border-app-border bg-app-bg shadow-2xl"
    >
      <header className="flex items-center justify-between gap-3 border-b border-app-border px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-app-accent/12 text-app-accent">
            <UploadCloud size={16} />
          </span>
          <div className="min-w-0">
            <h2 className="app-text-control-sm truncate font-semibold text-app-ink">
              {t('files.uploadProgress.title')}
            </h2>
            <p
              className="app-text-micro truncate text-app-ink/55"
              role="status"
              aria-atomic="true"
              aria-live="polite"
            >
              {activeCount > 0
                ? t('files.uploadProgress.activeSummary', {
                    count: activeCount,
                  })
                : t('files.uploadProgress.finishedSummary')}
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={onToggleCollapsed}
          title={t(
            collapsed
              ? 'files.uploadProgress.expandAction'
              : 'files.uploadProgress.collapseAction',
          )}
          aria-label={t(
            collapsed
              ? 'files.uploadProgress.expandAction'
              : 'files.uploadProgress.collapseAction',
          )}
          className="flex size-8 shrink-0 items-center justify-center rounded-md text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
        >
          {collapsed ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>
      </header>

      {collapsed ? null : (
        <div className="max-h-80 overflow-y-auto">
          {uploads.map((upload) => (
            <UploadRow
              key={upload.id}
              onCancel={onCancel}
              onDismiss={onDismiss}
              onRetry={onRetry}
              upload={upload}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function UploadRow({
  onCancel,
  onDismiss,
  onRetry,
  upload,
}: {
  onCancel: (uploadId: string) => void;
  onDismiss: (uploadId: string) => void;
  onRetry: (uploadId: string) => void;
  upload: FileUploadRecord;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const isUploading = upload.status === 'uploading';
  const StatusIcon =
    upload.status === 'succeeded'
      ? CheckCircle2
      : upload.status === 'failed'
        ? AlertCircle
        : upload.status === 'cancelled'
          ? Ban
          : Loader2;

  return (
    <div className="border-b border-app-border p-3 last:border-b-0">
      <div className="flex items-start gap-2">
        <StatusIcon
          size={16}
          className={cn(
            'mt-0.5 shrink-0',
            upload.status === 'uploading' && 'animate-spin text-app-accent',
            upload.status === 'succeeded' && 'text-app-success-text',
            upload.status === 'failed' && 'text-[var(--ui-color-danger)]',
          )}
        />
        <div className="min-w-0 flex-1">
          <p className="app-text-body-sm truncate font-medium text-app-ink">
            {t('files.uploadProgress.file', {
              current: upload.currentFileIndex,
              name: upload.currentFileName,
              total: upload.totalFiles,
            })}
          </p>
          <p className="app-text-caption mt-0.5 truncate text-app-ink/55">
            {upload.status === 'failed'
              ? upload.errorMessage
              : upload.status === 'cancelled'
                ? t('drive.upload.cancelled')
                : t('files.uploadProgress.bytes', {
                  loaded: formatByteSize(
                    upload.loadedBytes,
                    UPLOAD_BYTE_SIZE_OPTIONS,
                  ),
                  percent: upload.percent,
                  total: formatByteSize(
                    upload.totalBytes,
                    UPLOAD_BYTE_SIZE_OPTIONS,
                  ),
                })}
          </p>
        </div>
        {isUploading ? (
          <button
            type="button"
            onClick={() => onCancel(upload.id)}
            title={t('drive.upload.cancelAction')}
            aria-label={t('drive.upload.cancelAction')}
            className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
          >
            <Ban size={14} />
          </button>
        ) : (
          <span className="flex shrink-0 items-center">
            {upload.status === 'failed' ? (
              <button
                type="button"
                onClick={() => onRetry(upload.id)}
                title={t('drive.upload.retryAction')}
                aria-label={t('drive.upload.retryAction')}
                className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
              >
                <RotateCcw size={14} />
              </button>
            ) : null}
            <button
              type="button"
              onClick={() => onDismiss(upload.id)}
              title={t('common:actions.close')}
              aria-label={t('files.uploadProgress.dismissAction')}
              className="flex size-7 shrink-0 items-center justify-center rounded-md text-app-ink/45 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
            >
              <X size={14} />
            </button>
          </span>
        )}
      </div>
      <progress
        className={cn(
          'mt-2 h-2 w-full overflow-hidden rounded-full appearance-none bg-app-surface-hover [&::-moz-progress-bar]:rounded-full [&::-moz-progress-bar]:transition-all [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-app-surface-hover [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:transition-all',
          upload.status === 'failed' || upload.status === 'cancelled'
            ? '[&::-moz-progress-bar]:bg-[var(--ui-color-danger)] [&::-webkit-progress-value]:bg-[var(--ui-color-danger)]'
            : '[&::-moz-progress-bar]:bg-app-accent [&::-webkit-progress-value]:bg-app-accent',
        )}
        aria-label={t('files.uploadProgress.file', {
          current: upload.currentFileIndex,
          name: upload.currentFileName,
          total: upload.totalFiles,
        })}
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={upload.percent}
        max={100}
        value={upload.percent}
      />
    </div>
  );
}

const UPLOAD_BYTE_SIZE_OPTIONS = {
  fractionDigits: 'compact',
  units: ['KB', 'MB', 'GB', 'TB'],
} as const;
