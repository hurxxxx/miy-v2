import { useCallback, useState, type SetStateAction } from 'react';
import type { TFunction } from 'i18next';

import { formatByteSize } from '../../../platform/format/byte-size';

import {
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
  const [dragOver, setDragOver] = useState(false);

  const handleFileUpload = useCallback(
    async (files: FileList | File[]) => {
      if (!api || !canEdit) return;
      setUploading(true);
      setSaveError(null);
      try {
        const uploadedAttachments = await Promise.all(
          Array.from(files).map((file) =>
            uploadAttachment(api, fileApi, taskId, file, workspaceSlug),
          ),
        );
        setAttachments((prev) => [...prev, ...uploadedAttachments]);
        await notifyTaskDetailUpdated(onUpdate);
      } catch (error) {
        setSaveError(
          getTaskDetailMutationErrorMessage(
            error,
            t('pms.taskDetail.errors.uploadFileFailed'),
          ),
        );
      } finally {
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
    setDragOver,
    uploading,
  };
}
