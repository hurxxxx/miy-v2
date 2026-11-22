import { useCallback, useEffect, useRef, useState, type DragEvent } from 'react';
import { requestId } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { Download, Loader2, Paperclip, Plus, X } from 'lucide-react';

import { cn } from '../../../lib/utils';
import { formatByteSize } from '../../../platform/format/byte-size';

import {
  useAppApi,
  useAppFileApi,
} from '../../../platform/runtime/runtime-context';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import {
  DOCS_ATTACHMENT_MAX_BYTES,
  canRetryAttachmentUpload,
  deleteDocPageAttachment,
  fetchDocPageAttachmentBlob,
  listDocPageAttachments,
  uploadDocPageAttachment,
  type DocsPageAttachment,
} from '../api/docs-api';
import {
  docPageAttachmentErrorMessage,
  sortDocPageAttachments,
  withUploadedAttachments,
  withoutAttachment,
} from './docs-page-attachments-model';

// Files kept beside the page body. Reading the page is enough to download
// them; editing it is required to add or remove one.
//
// miy: calls go through `runtime.api` / `runtime.fileApi` (10 MiB per file).
// An upload whose answer was lost or failed with a retryable error keeps its
// request ID: the next try (or the 「다시 시도」 action) recovers the same
// reservation instead of creating a second file, and nothing is deleted
// automatically.

type UploadAttempt = { file: File; reservationId: string };

export function DocsPageAttachments({
  canEdit,
  pageId,
}: {
  canEdit: boolean;
  pageId: string;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const token = useAppApi();
  const fileApi = useAppFileApi();
  const [retryable, setRetryable] = useState<UploadAttempt[]>([]);
  const uploadInFlight = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const [attachments, setAttachments] = useState<DocsPageAttachment[]>([]);
  const [loading, setLoading] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [downloading, setDownloading] = useState<string | null>(null);
  // Nested elements fire dragleave on every crossing; count entries instead.
  const dragDepth = useRef(0);
  // A slow response for a page the reader already left must not paint over
  // the page they are on now.
  const requestedPageId = useRef(pageId);

  const load = useCallback(async () => {
    if (!token) return;
    requestedPageId.current = pageId;
    setLoading(true);
    setError(null);
    try {
      const response = await listDocPageAttachments(token, pageId);
      if (requestedPageId.current !== pageId) return;
      setAttachments(sortDocPageAttachments(response.items));
    } catch (caught) {
      if (requestedPageId.current !== pageId) return;
      setError(
        docPageAttachmentErrorMessage(
          caught,
          t('apps:docs.attachments.loadFailed'),
        ),
      );
    } finally {
      if (requestedPageId.current === pageId) setLoading(false);
    }
  }, [pageId, t, token]);

  useEffect(() => {
    setAttachments([]);
    setRetryable([]);
    void load();
  }, [load]);

  const uploadAttempts = useCallback(
    async (attempts: UploadAttempt[]) => {
      if (!token || !canEdit || !attempts.length || uploadInFlight.current) return;
      const tooLarge = attempts.find(
        ({ file }) => file.size > DOCS_ATTACHMENT_MAX_BYTES,
      );
      if (tooLarge) {
        setError(
          t('errors:docs_attachment_size_limit_exceeded', {
            limit_mb: DOCS_ATTACHMENT_MAX_BYTES / (1024 * 1024),
          }),
        );
        return;
      }
      const uploadPageId = pageId;
      uploadInFlight.current = true;
      setUploading(true);
      setError(null);
      const tried = new Set(attempts.map(({ reservationId }) => reservationId));
      const uploaded: DocsPageAttachment[] = [];
      const retry: UploadAttempt[] = [];
      let firstError: unknown = null;
      try {
        for (const attempt of attempts) {
          try {
            uploaded.push(
              await uploadDocPageAttachment(
                token,
                fileApi,
                uploadPageId,
                attempt.file,
                attempt.reservationId,
              ),
            );
          } catch (caught) {
            firstError ??= caught;
            if (canRetryAttachmentUpload(caught)) retry.push(attempt);
          }
        }
        // The reader may have moved on while the upload ran; those files
        // belong to the page that was open when it started.
        if (requestedPageId.current !== uploadPageId) return;
        setRetryable((current) => [
          ...current.filter(({ reservationId }) => !tried.has(reservationId)),
          ...retry,
        ]);
        if (uploaded.length) {
          setAttachments((current) =>
            withUploadedAttachments(current, uploaded),
          );
        }
        if (firstError) {
          setError(
            docPageAttachmentErrorMessage(
              firstError,
              t('apps:docs.attachments.uploadFailed'),
            ),
          );
        }
      } finally {
        uploadInFlight.current = false;
        if (requestedPageId.current === uploadPageId) setUploading(false);
      }
    },
    [canEdit, fileApi, pageId, t, token],
  );

  const upload = useCallback(
    (files: FileList | File[]) =>
      uploadAttempts(
        Array.from(files, (file) => ({ file, reservationId: requestId() })),
      ),
    [uploadAttempts],
  );

  useNavigationDirty(uploading || retryable.length > 0);

  const remove = useCallback(
    async (attachment: DocsPageAttachment) => {
      if (!token || !canEdit) return;
      setRemoving(attachment.id);
      setError(null);
      try {
        await deleteDocPageAttachment(token, attachment.id);
        if (requestedPageId.current !== pageId) return;
        setAttachments((current) => withoutAttachment(current, attachment.id));
      } catch (caught) {
        if (requestedPageId.current !== pageId) return;
        setError(
          docPageAttachmentErrorMessage(
            caught,
            t('apps:docs.attachments.deleteFailed'),
          ),
        );
      } finally {
        setRemoving(null);
      }
    },
    [canEdit, pageId, t, token],
  );

  /**
   * Pull the bytes over the authenticated route and hand them to the browser
   * as a one-shot object URL. A plain link would arrive without the bearer
   * token and be rejected.
   */
  const download = useCallback(
    async (attachment: DocsPageAttachment) => {
      if (!token) return;
      setDownloading(attachment.id);
      setError(null);
      let objectUrl: string | null = null;
      try {
        const blob = await fetchDocPageAttachmentBlob(fileApi, attachment.id);
        objectUrl = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = objectUrl;
        link.download = attachment.filename;
        link.rel = 'noopener';
        document.body.append(link);
        link.click();
        link.remove();
      } catch (caught) {
        setError(
          docPageAttachmentErrorMessage(
            caught,
            t('apps:docs.attachments.downloadFailed'),
          ),
        );
      } finally {
        // Safari needs the URL to outlive the click; release it next tick.
        if (objectUrl) {
          const releasing = objectUrl;
          window.setTimeout(() => URL.revokeObjectURL(releasing), 0);
        }
        setDownloading(null);
      }
    },
    [fileApi, t, token],
  );

  const onDragEnter = (event: DragEvent<HTMLDivElement>) => {
    if (!canEdit || !hasFiles(event)) return;
    dragDepth.current += 1;
    setDragOver(true);
  };

  const onDragOver = (event: DragEvent<HTMLDivElement>) => {
    if (!canEdit || !hasFiles(event)) return;
    // Without this the browser navigates away to the dropped file.
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
  };

  const onDragLeave = () => {
    dragDepth.current = Math.max(0, dragDepth.current - 1);
    if (dragDepth.current === 0) setDragOver(false);
  };

  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    if (!canEdit || !hasFiles(event)) return;
    event.preventDefault();
    dragDepth.current = 0;
    setDragOver(false);
    void upload(event.dataTransfer.files);
  };

  return (
    <section
      onDragEnter={onDragEnter}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      className={cn(
        'mt-6 rounded-lg border border-app-border p-4 transition-colors',
        dragOver ? 'border-app-accent bg-app-accent/10' : '',
      )}
    >
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="app-text-control inline-flex items-center gap-1.5 text-app-ink">
          <Paperclip size={14} className="text-app-ink/55" />
          {t('apps:docs.attachments.title')}
          <span className="app-text-caption text-app-ink/55">
            {attachments.length}
          </span>
        </h2>
        {canEdit ? (
          <button
            type="button"
            disabled={uploading || !token}
            onClick={() => inputRef.current?.click()}
            className="app-text-control-sm inline-flex items-center gap-1.5 rounded-md border border-app-border px-2.5 py-1.5 text-app-ink transition-colors hover:bg-app-surface-hover disabled:opacity-50"
          >
            {uploading ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <Plus size={14} />
            )}
            {t('apps:docs.attachments.add')}
          </button>
        ) : null}
        <input
          ref={inputRef}
          type="file"
          multiple
          hidden
          onChange={(event) => {
            void upload(event.target.files ?? []);
            event.target.value = '';
          }}
        />
      </div>

      {error ? (
        <p
          role="alert"
          className="app-text-caption mb-2 rounded border border-app-danger/40 bg-app-danger/10 px-3 py-2 text-app-danger"
        >
          {error}
        </p>
      ) : null}

      {retryable.length ? (
        <div className="app-text-caption mb-2 flex items-center justify-between gap-2 rounded border border-app-border px-3 py-2 text-app-ink/70">
          <span>
            {t('apps:docsApp.attachments.retryPending', {
              count: retryable.length,
            })}
          </span>
          <button
            type="button"
            disabled={uploading}
            onClick={() => void uploadAttempts(retryable)}
            className="app-text-control-sm rounded-md border border-app-border px-2.5 py-1 text-app-ink hover:bg-app-surface-hover disabled:opacity-50"
          >
            {t('apps:docsApp.attachments.retry')}
          </button>
        </div>
      ) : null}

      {loading && !attachments.length ? (
        <p className="app-text-caption px-1 py-3 text-app-ink/45">
          {t('apps:docs.attachments.loading')}
        </p>
      ) : null}

      {!loading && !attachments.length ? (
        <p className="app-text-caption rounded border border-dashed border-app-border px-3 py-6 text-center text-app-ink/45">
          {t(
            canEdit
              ? 'apps:docs.attachments.emptyEditable'
              : 'apps:docs.attachments.empty',
          )}
        </p>
      ) : null}

      {attachments.length ? (
        <ul className="flex flex-col divide-y divide-app-border">
          {attachments.map((attachment) => (
            <li
              key={attachment.id}
              className="flex items-center gap-2 py-2 first:pt-0 last:pb-0"
            >
              <button
                type="button"
                disabled={downloading === attachment.id}
                onClick={() => void download(attachment)}
                title={t('apps:docs.attachments.download', {
                  name: attachment.filename,
                })}
                className="flex min-w-0 flex-1 items-center gap-2 text-left disabled:opacity-60"
              >
                {downloading === attachment.id ? (
                  <Loader2 size={14} className="shrink-0 animate-spin" />
                ) : (
                  <Download size={14} className="shrink-0 text-app-ink/45" />
                )}
                <span className="app-text-body-sm truncate text-app-ink hover:underline">
                  {attachment.filename}
                </span>
                <span className="app-text-caption shrink-0 text-app-ink/45">
                  {formatByteSize(attachment.size_bytes)}
                </span>
              </button>
              <span className="app-text-caption hidden shrink-0 text-app-ink/45 sm:inline">
                {attachment.uploaded_by_name}
              </span>
              {canEdit ? (
                <button
                  type="button"
                  disabled={removing === attachment.id}
                  aria-label={t('apps:docs.attachments.remove', {
                    name: attachment.filename,
                  })}
                  onClick={() => void remove(attachment)}
                  className="shrink-0 rounded p-1 text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink disabled:opacity-50"
                >
                  {removing === attachment.id ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : (
                    <X size={14} />
                  )}
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}

      {canEdit ? (
        <p className="app-text-caption mt-3 text-app-ink/45">
          {t('apps:docs.attachments.hint')}
        </p>
      ) : null}
    </section>
  );
}

/** Ignore dragged text selections; only real files should light the panel up. */
function hasFiles(event: DragEvent<HTMLDivElement>): boolean {
  return Array.from(event.dataTransfer.types).includes('Files');
}
