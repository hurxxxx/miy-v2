import { useCallback, useRef, useState, type SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { requestId } from '../../../../../platform-components/ui/utilities';

import { formatByteSize } from '../../../platform/format/byte-size';

import {
  canRetryAttachmentUpload,
  deleteAttachment,
  uploadAttachment,
  type PmsAttachment,
} from '../api/pms-api';
import {
  getTaskDetailMutationErrorMessage,
  notifyTaskDetailUpdated,
} from './task-detail-mutation';
import {
  type AppApi,
  useAppFileApi,
} from '../../../platform/runtime/runtime-context';

type UploadAttempt = { file: File; reservationId: string };

export function formatTaskDetailAttachmentSize(sizeBytes: number): string {
  return formatByteSize(sizeBytes);
}

export function useTaskDetailAttachments({
  workspaceSlug,
  canEdit,
  onUpdate,
  setAttachments,
  setSaveError,
  taskId,
  api,
  t,
}: {
  workspaceSlug?: string | null;
  canEdit: boolean;
  onUpdate?: () => void | Promise<void>;
  setAttachments: (value: SetStateAction<PmsAttachment[]>) => void;
  setSaveError: (message: string | null) => void;
  taskId: string;
  api: AppApi | null;
  t: TFunction;
}) {
  const fileApi = useAppFileApi();
  const [uploading, setUploading] = useState(false);
  const uploadInFlight = useRef(false);
  const [retryableUploads, setRetryableUploads] = useState<UploadAttempt[]>([]);
  const [dragOver, setDragOver] = useState(false);

  const uploadFiles = useCallback(
    async (attempts: UploadAttempt[]) => {
      if (!api || !canEdit || !attempts.length || uploadInFlight.current) return;
      uploadInFlight.current = true;
      setUploading(true);
      setSaveError(null);
      const retriedIds = new Set(attempts.map(({ reservationId }) => reservationId));
      const uploaded: PmsAttachment[] = [];
      const retry: UploadAttempt[] = [];
      let firstError: unknown;
      try {
        for (const attempt of attempts) {
          try {
            uploaded.push(
              await uploadAttachment(api, fileApi, taskId, attempt.file, workspaceSlug, {
                reservationId: attempt.reservationId,
              }),
            );
          } catch (error) {
            firstError ??= error;
            if (canRetryAttachmentUpload(error)) retry.push(attempt);
          }
        }
        setRetryableUploads((previous) => [
          ...previous.filter(({ reservationId }) => !retriedIds.has(reservationId)),
          ...retry,
        ]);
        if (uploaded.length) {
          setAttachments((previous) => [
            ...previous,
            ...uploaded.filter(({ id }) => !previous.some((item) => item.id === id)),
          ]);
          await notifyTaskDetailUpdated(onUpdate);
        }
        if (firstError) {
          setSaveError(
            getTaskDetailMutationErrorMessage(firstError, t('pms.taskDetail.errors.uploadFileFailed')),
          );
        }
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(error, t('pms.taskDetail.errors.uploadFileFailed')),
        );
      } finally {
        uploadInFlight.current = false;
        setUploading(false);
        setDragOver(false);
      }
    },
    [
      canEdit,
      fileApi,
      onUpdate,
      setAttachments,
      setSaveError,
      t,
      taskId,
      api,
      workspaceSlug,
    ],
  );
  const handleFileUpload = useCallback(
    (files: FileList | File[]) => uploadFiles(
      Array.from(files, (file) => ({ file, reservationId: requestId() })),
    ),
    [uploadFiles],
  );

  const handleDeleteAttachment = useCallback(
    async (attachmentId: string) => {
      if (!api || !canEdit) return;
      setSaveError(null);
      try {
        await deleteAttachment(api, attachmentId, workspaceSlug);
        setAttachments((prev) =>
          prev.filter((attachment) => attachment.id !== attachmentId),
        );
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.deleteAttachmentFailed'),
          ),
        );
      }
    },
    [canEdit, onUpdate, setAttachments, setSaveError, t, api, workspaceSlug],
  );

  return {
    dragOver,
    formatAttachmentSize: formatTaskDetailAttachmentSize,
    handleDeleteAttachment,
    handleFileUpload,
    retryableUploads,
    retryUploads: () => uploadFiles(retryableUploads),
    setDragOver,
    uploading,
  };
}
