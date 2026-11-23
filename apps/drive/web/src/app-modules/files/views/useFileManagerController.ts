import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent,
} from 'react';
import { useTranslation } from 'react-i18next';
import { useFeedback } from '@miy/ui';

import { i18n as driveI18n } from '../../../platform/i18n';
import { useAppSearchParams } from '../../../platform/runtime/app-location';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import { normalizeTimeZone } from '../../../platform/time/time-utils';
import {
  browseFiles,
  bulkDeleteFileItems,
  createFileFolder,
  deleteDriveFile,
  deleteFileFolder,
  getFileDownloadUrl,
  getFilePreviewBlob,
  moveFileItems,
  planFileArchive,
  updateDriveFile,
  updateFileFolder,
  type FileBrowseResponse,
  type FileFolderItem,
  type FileIngressBlockReason,
  type FileItem,
  type FileVisibility,
} from '../api/files-api';
import { MAX_UPLOAD_BYTES } from '../api/file-transfer';
import { useFileUploadManager } from '../file-upload-provider';
import { FILES_CHANGED_EVENT } from '../file-upload-session';
import {
  createEmptyFileManagerSelection,
  filterSelectionToVisibleItems,
  planBulkDeleteCommand,
  planCreateFolderCommand,
  planDeleteFolderCommand,
  planFileManagerBulkDownload,
  selectAllVisibleFileManagerItems,
  summarizeFileManagerSelection,
  toggleFileSelection,
  toggleFolderSelection,
} from './FileManagerWorkflow';
import { sendDownloadBatch, startDownload, type DownloadBatch } from './file-downloads';

export type LoadState = 'idle' | 'loading' | 'ready' | 'error';
export type BulkAction = 'delete' | 'download' | 'move' | null;
export type FolderDialogState =
  | { mode: 'create'; folder: null }
  | { mode: 'edit'; folder: FileFolderItem };
export type ImagePreviewState = {
  file: FileItem;
  url: string;
};
/** miy (보강): the move dialog's selection. */
export type MoveDialogState = {
  fileIds: string[];
  folderIds: string[];
};

function isFileDrag(event: DragEvent<HTMLElement>) {
  return Array.from(event.dataTransfer.types).includes('Files');
}

/**
 * AI-DO useFileManagerController with miy's frame runtime: the space comes
 * from the route, the folder from `?folder=`, the API from `runtime.api`,
 * downloads go through `runtime.download` links and previews through a Blob
 * URL. The RAG status polling and corpus ingress administration are not
 * ported. Added (보강): rename, move and versions.
 */
export function useFileManagerController(spaceId: string) {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const runtime = useRuntime();
  const { api, fileApi } = runtime;
  const [searchParams, setSearchParams] = useAppSearchParams();
  const feedback = useFeedback();
  const timeZone = normalizeTimeZone(runtime.settings.timeZone);
  const { hasActiveUploads, startUpload } = useFileUploadManager();
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const dragDepthRef = useRef(0);
  const loadAbortRef = useRef<AbortController | null>(null);
  const loadRequestSeqRef = useRef(0);
  const folderId = searchParams.get('folder');
  const browseScope = useMemo(() => ({ api, spaceId, folderId }), [api, spaceId, folderId]);
  const committedBrowseScope = useRef<typeof browseScope | null>(null);
  const isCurrentBrowse = useCallback(() => committedBrowseScope.current === browseScope, [browseScope]);
  const [browseSnapshot, setBrowseSnapshot] = useState<{
    folderId: string | null;
    response: FileBrowseResponse;
    spaceId: string;
  } | null>(null);
  const [loadState, setLoadState] = useState<LoadState>('idle');
  const [loadError, setLoadError] = useState<string | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [uploadVisibility, setUploadVisibility] =
    useState<FileVisibility>('private');
  const [draggingFiles, setDraggingFiles] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [previewBusyId, setPreviewBusyId] = useState<string | null>(null);
  const [bulkAction, setBulkAction] = useState<BulkAction>(null);
  const [downloadBatch, setDownloadBatch] = useState<DownloadBatch | null>(null);
  const downloadAbort = useRef<AbortController | null>(null);
  const [selection, setSelection] = useState(createEmptyFileManagerSelection);
  const [imagePreview, setImagePreviewState] =
    useState<ImagePreviewState | null>(null);
  const previewAbort = useRef<AbortController | null>(null);
  const previewUrl = useRef<string | null>(null);
  const [folderDialog, setFolderDialog] = useState<FolderDialogState | null>(
    null,
  );
  const [folderName, setFolderName] = useState('');
  const [folderVisibility, setFolderVisibility] =
    useState<FileVisibility>('private');
  const [savingFolder, setSavingFolder] = useState(false);
  const [renameTarget, setRenameTarget] = useState<FileItem | null>(null);
  const [moveDialog, setMoveDialog] = useState<MoveDialogState | null>(null);
  const [versionsTarget, setVersionsTarget] = useState<FileItem | null>(null);

  useEffect(() => {
    setDownloadBatch(null);
    setBulkAction((current) => current === 'download' ? null : current);
    return () => {
      downloadAbort.current?.abort();
      downloadAbort.current = null;
    };
  }, [spaceId]);

  const browse =
    browseSnapshot?.folderId === folderId && browseSnapshot.spaceId === spaceId
      ? browseSnapshot.response
      : null;
  const currentFolder = browse?.current_folder ?? null;
  const canIngest = loadState === 'ready' && (browse?.can_ingest ?? false);
  const ingestBlockReason: FileIngressBlockReason | null =
    browse?.ingest_block_reason ?? null;
  const effectiveUploadVisibility =
    currentFolder?.visibility ?? uploadVisibility;
  const workspaceName = browse?.space.name ?? '';

  // An open folder dialog with input is unsaved work (miy contract).
  useNavigationDirty(
    bulkAction !== null ||
      busyId !== null ||
      downloadBatch !== null ||
      savingFolder ||
      (folderDialog !== null &&
        (folderDialog.mode === 'create'
          ? folderName.trim() !== ''
          : folderName !== folderDialog.folder.name)),
  );

  const setImagePreview = useCallback((value: ImagePreviewState | null) => {
    if (previewUrl.current && previewUrl.current !== value?.url) {
      URL.revokeObjectURL(previewUrl.current);
    }
    previewUrl.current = value?.url ?? null;
    setImagePreviewState(value);
  }, []);

  useLayoutEffect(() => {
    committedBrowseScope.current = browseScope;
    return () => {
      committedBrowseScope.current = null;
      previewAbort.current?.abort();
      previewAbort.current = null;
      if (previewUrl.current) URL.revokeObjectURL(previewUrl.current);
      previewUrl.current = null;
    };
  }, [browseScope]);

  const load = useCallback(
    async (options: { silent?: boolean } = {}) => {
      if (!isCurrentBrowse()) return false;
      const sequence = loadRequestSeqRef.current + 1;
      loadRequestSeqRef.current = sequence;
      loadAbortRef.current?.abort();
      const controller = new AbortController();
      loadAbortRef.current = controller;
      if (!options.silent) {
        setLoadState('loading');
        setLoadError(null);
      }
      try {
        const response = await browseFiles({ api }, spaceId, folderId, {
          signal: controller.signal,
        });
        if (
          isCurrentBrowse() && !controller.signal.aborted &&
          loadRequestSeqRef.current === sequence
        ) {
          setBrowseSnapshot({ folderId, response, spaceId });
          setLoadState('ready');
          return true;
        }
        return false;
      } catch (caughtError) {
        if (isCurrentBrowse() && !controller.signal.aborted && loadRequestSeqRef.current === sequence) {
          setLoadState('error');
          setLoadError(
            caughtError instanceof Error
              ? caughtError.message
              : driveI18n.t('apps:files.errors.loadFailed'),
          );
        }
        return false;
      } finally {
        if (loadAbortRef.current === controller) {
          loadAbortRef.current = null;
        }
      }
    },
    [api, folderId, isCurrentBrowse, spaceId],
  );

  useEffect(() => {
    void load();
    return () => {
      loadAbortRef.current?.abort();
    };
  }, [load]);

  useEffect(() => {
    setFolderDialog(null);
    setRenameTarget(null);
    setMoveDialog(null);
    setVersionsTarget(null);
    setImagePreview(null);
    setPreviewBusyId(null);
    setSelection(createEmptyFileManagerSelection());
  }, [browseScope, setImagePreview]);

  const openCreateFolder = useCallback(() => {
    if (!canIngest) {
      return;
    }
    setFolderDialog({ mode: 'create', folder: null });
    setFolderError(null);
    setFolderName('');
    setFolderVisibility(effectiveUploadVisibility);
  }, [canIngest, effectiveUploadVisibility]);

  useEffect(() => {
    const filesChanged = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          folderId?: string | null;
          spaceId?: string;
        }>
      ).detail;
      if (
        detail?.spaceId !== spaceId ||
        (detail.folderId !== undefined && detail.folderId !== folderId)
      ) {
        return;
      }
      void load({ silent: true });
    };
    window.addEventListener(FILES_CHANGED_EVENT, filesChanged);
    return () => window.removeEventListener(FILES_CHANGED_EVENT, filesChanged);
  }, [folderId, load, spaceId]);

  const folders = useMemo(() => browse?.folders ?? [], [browse?.folders]);
  const files = useMemo(() => browse?.files ?? [], [browse?.files]);

  const isEmpty =
    loadState === 'ready' && folders.length === 0 && files.length === 0;
  const filteredSelection = useMemo(
    () =>
      filterSelectionToVisibleItems(selection, {
        files,
        folders,
      }),
    [files, folders, selection],
  );
  const { allVisibleSelected, selectedCount, selectedVisibleSize } = useMemo(
    () =>
      summarizeFileManagerSelection(filteredSelection, {
        files,
        folders,
      }),
    [files, filteredSelection, folders],
  );

  const clearSelection = useCallback(() => {
    setSelection(createEmptyFileManagerSelection());
  }, []);

  const emitChanged = useCallback(() => {
    window.dispatchEvent(
      new CustomEvent(FILES_CHANGED_EVENT, { detail: { spaceId } }),
    );
  }, [spaceId]);

  function navigateToFolder(nextFolderId: string | null) {
    const next = new URLSearchParams(searchParams);
    next.delete('view');
    if (nextFolderId) {
      next.set('folder', nextFolderId);
    } else {
      next.delete('folder');
    }
    setSearchParams(next);
  }

  function openEditFolder(folder: FileFolderItem) {
    setFolderDialog({ mode: 'edit', folder });
    setFolderError(null);
    setFolderName(folder.name);
    setFolderVisibility(folder.visibility);
  }

  function setAllVisibleSelected(checked: boolean) {
    setSelection(
      selectAllVisibleFileManagerItems(
        {
          files,
          folders,
        },
        checked,
      ),
    );
  }

  function setFileSelected(fileId: string, checked: boolean) {
    setSelection((current) => toggleFileSelection(current, fileId, checked));
  }

  function setFolderSelected(folderIdValue: string, checked: boolean) {
    setSelection((current) =>
      toggleFolderSelection(current, folderIdValue, checked),
    );
  }

  async function handleFolderSubmit() {
    if (!folderDialog) {
      return;
    }
    if (folderDialog.mode === 'create' && !canIngest) {
      setFolderDialog(null);
      return;
    }
    // The dialog's target is fixed when it opened (a late reload must not
    // change which folder is saved).
    const dialog = folderDialog;
    setSavingFolder(true);
    setFolderError(null);
    try {
      if (dialog.mode === 'create') {
        const plan = planCreateFolderCommand({
          name: folderName,
          currentFolderId: folderId,
          currentFolderVisibility: currentFolder?.visibility,
          selectedVisibility: folderVisibility,
        });
        await createFileFolder(runtime, spaceId, {
          name: plan.payload.name,
          parent_id: plan.payload.parent_id,
          visibility: plan.payload.visibility,
        });
      } else {
        await updateFileFolder(runtime, spaceId, dialog.folder.id, {
          name: folderName,
        });
      }
      if (isCurrentBrowse()) setFolderDialog(null);
      emitChanged();
    } catch (caughtError) {
      if (isCurrentBrowse()) setFolderError(
        caughtError instanceof Error
          ? caughtError.message
          : t('files.errors.saveFolderFailed'),
      );
    } finally {
      setSavingFolder(false);
    }
  }

  const uploadFiles = useCallback(
    (selectedFilesValue: File[], fileId: string | null = null) => {
      if (selectedFilesValue.length === 0) {
        return;
      }
      if (fileId === null && !canIngest) {
        return;
      }
      if (selectedFilesValue.some((file) => file.size > MAX_UPLOAD_BYTES)) {
        feedback.error(
          t('errors:files_file_size_limit_exceeded', {
            limit_mb: MAX_UPLOAD_BYTES / 1024 / 1024,
          }),
        );
        return;
      }
      const accepted = startUpload({
        client: { api, fileApi },
        files: selectedFilesValue,
        fileId,
        folderId,
        spaceId,
        uploadVisibility: effectiveUploadVisibility,
      });
      if (!accepted) {
        feedback.error(t('files.errors.uploadAlreadyRunning'));
      }
    },
    [
      canIngest,
      effectiveUploadVisibility,
      folderId,
      startUpload,
      feedback,
      api,
      fileApi,
      spaceId,
      t,
    ],
  );

  function handleFileChange(event: ChangeEvent<HTMLInputElement>) {
    const selectedFilesValue = Array.from(event.target.files ?? []);
    event.target.value = '';
    uploadFiles(selectedFilesValue);
  }

  function handleDragEnter(event: DragEvent<HTMLElement>) {
    if (!isFileDrag(event)) {
      return;
    }
    event.preventDefault();
    if (!canIngest || hasActiveUploads) {
      event.dataTransfer.dropEffect = 'none';
      setDraggingFiles(false);
      return;
    }
    dragDepthRef.current += 1;
    setDraggingFiles(true);
  }

  function handleDragOver(event: DragEvent<HTMLElement>) {
    if (!isFileDrag(event)) {
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect =
      !canIngest || hasActiveUploads ? 'none' : 'copy';
    if (canIngest && !hasActiveUploads) {
      setDraggingFiles(true);
    }
  }

  function handleDragLeave(event: DragEvent<HTMLElement>) {
    if (!isFileDrag(event)) {
      return;
    }
    event.preventDefault();
    dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
    if (dragDepthRef.current === 0) {
      setDraggingFiles(false);
    }
  }

  function handleDrop(event: DragEvent<HTMLElement>) {
    if (!isFileDrag(event)) {
      return;
    }
    event.preventDefault();
    dragDepthRef.current = 0;
    setDraggingFiles(false);
    if (!canIngest) {
      return;
    }
    if (hasActiveUploads) {
      feedback.error(t('files.errors.uploadAlreadyRunning'));
      return;
    }
    uploadFiles(Array.from(event.dataTransfer.files));
  }

  async function handleDownload(file: FileItem) {
    setBusyId(file.id);
    try {
      await startDownload(runtime, file.id, (fileId) =>
        getFileDownloadUrl(runtime, spaceId, fileId),
      );
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('files.errors.downloadFailed'),
      );
    } finally {
      setBusyId(null);
    }
  }

  async function handlePreview(file: FileItem) {
    previewAbort.current?.abort();
    const controller = new AbortController();
    previewAbort.current = controller;
    setPreviewBusyId(file.id);
    try {
      const blob = await getFilePreviewBlob(runtime, file.id, { signal: controller.signal });
      if (controller.signal.aborted || !isCurrentBrowse()) return;
      setImagePreview({ file, url: URL.createObjectURL(blob) });
    } catch (caughtError) {
      if (controller.signal.aborted || !isCurrentBrowse()) return;
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('files.errors.previewFailed'),
      );
    } finally {
      if (previewAbort.current === controller) {
        previewAbort.current = null;
        setPreviewBusyId(null);
      }
    }
  }

  async function handleDeleteFile(file: FileItem) {
    if (
      !window.confirm(t('files.confirm.deleteFile', { name: file.filename }))
    ) {
      return;
    }
    setBusyId(file.id);
    try {
      await deleteDriveFile(runtime, spaceId, file.id);
      emitChanged();
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('files.errors.deleteFailed'),
      );
    } finally {
      setBusyId(null);
    }
  }

  async function handleDeleteFolder(folder: FileFolderItem) {
    if (
      !window.confirm(t('files.confirm.deleteFolder', { name: folder.name }))
    ) {
      return;
    }
    const plan = planDeleteFolderCommand({
      folderId: folder.id,
      currentFolderId: folderId,
    });
    setBusyId(plan.busyId);
    try {
      await deleteFileFolder(runtime, spaceId, plan.folderId);
      if (isCurrentBrowse() && plan.afterSuccess.kind === 'navigate-root-and-clear-browse') {
        navigateToFolder(plan.afterSuccess.targetFolderId);
        setBrowseSnapshot(null);
      }
      emitChanged();
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('files.errors.deleteFailed'),
      );
    } finally {
      setBusyId(null);
    }
  }

  async function handleBulkDelete() {
    if (selectedCount === 0) {
      return;
    }
    if (
      !window.confirm(
        t('files.confirm.deleteSelected', { count: selectedCount }),
      )
    ) {
      return;
    }
    const plan = planBulkDeleteCommand(filteredSelection);
    if (plan.kind === 'none') {
      return;
    }
    setBulkAction('delete');
    try {
      await bulkDeleteFileItems(runtime, spaceId, plan.payload);
      if (isCurrentBrowse()) clearSelection();
      emitChanged();
    } catch (caughtError) {
      feedback.error(
        caughtError instanceof Error
          ? caughtError.message
          : t('files.errors.deleteFailed'),
      );
    } finally {
      setBulkAction(null);
    }
  }

  async function runDownloadBatch(plan: () => Promise<DownloadBatch>) {
    const download = runtime.downloadAndWait;
    if (!download) {
      feedback.error(t('drive.download.unavailable'));
      return;
    }
    if (downloadAbort.current) return;
    const controller = new AbortController();
    const { signal } = controller;
    downloadAbort.current = controller;
    setBulkAction('download');
    let current: DownloadBatch | null = null;
    try {
      current = await plan();
      if (signal.aborted) return;
      if (current.total === 0) {
        feedback.error(t('drive.bulkDownload.empty'));
        return;
      }
      setDownloadBatch(current);
      await sendDownloadBatch(
        download, current, (id) => getFileDownloadUrl(runtime, spaceId, id), signal,
        (next) => {
          current = next;
          setDownloadBatch(next);
        },
      );
      setDownloadBatch(null);
      const result = t('drive.bulkDownload.finished', { sent: current.sent, total: current.total });
      if (current.sent === current.total) {
        feedback.success(result);
      } else {
        feedback.info(result);
      }
    } catch (error) {
      if (signal.aborted) return;
      if (current) {
        feedback.error(t('drive.bulkDownload.paused', { path: current.remaining[0]?.path }));
      } else {
        feedback.error(error instanceof Error ? error.message : t('files.errors.downloadFailed'));
      }
    } finally {
      if (downloadAbort.current === controller) {
        downloadAbort.current = null;
        setBulkAction(null);
      }
    }
  }

  async function handleBulkDownload() {
    if (selectedCount === 0 || downloadBatch || downloadAbort.current) return;
    const plan = planFileManagerBulkDownload(filteredSelection, files);
    if (plan.kind === 'none') {
      return;
    }
    if (plan.kind === 'single-file') {
      await handleDownload(plan.file);
      return;
    }

    await runDownloadBatch(async () => {
      const archive = await planFileArchive(runtime, spaceId, plan.payload);
      const remaining = archive.entries.flatMap(({ file_id, path }) =>
        file_id === null ? [] : [{ id: file_id, path }],
      );
      return { remaining, total: remaining.length, sent: 0 };
    });
  }

  function resumeDownloadBatch(skip = false) {
    if (!downloadBatch) return;
    const batch = downloadBatch;
    void runDownloadBatch(async () =>
      skip ? { ...batch, remaining: batch.remaining.slice(1) } : batch,
    );
  }

  function stopDownloadBatch() {
    downloadAbort.current?.abort();
    setDownloadBatch(null);
  }

  async function handleRename(file: FileItem, filename: string) {
    const updated = await updateDriveFile(runtime, spaceId, file.id, {
      filename,
    });
    if (isCurrentBrowse()) setRenameTarget(null);
    emitChanged();
    return updated;
  }

  function openMoveSelected() {
    if (selectedCount === 0) return;
    setMoveDialog({
      fileIds: Array.from(filteredSelection.fileIds),
      folderIds: Array.from(filteredSelection.folderIds),
    });
  }

  async function handleMove(target: MoveDialogState, targetFolderId: string | null) {
    setBulkAction('move');
    try {
      await moveFileItems(runtime, spaceId, {
        fileIds: target.fileIds,
        folderIds: target.folderIds,
        targetFolderId,
      });
      if (isCurrentBrowse()) {
        setMoveDialog(null);
        clearSelection();
      }
      feedback.success(
        t('drive.move.done', {
          count: target.fileIds.length + target.folderIds.length,
        }),
      );
      emitChanged();
    } finally {
      setBulkAction(null);
    }
  }

  function uploadNewVersion(file: FileItem, picked: File | undefined) {
    if (!picked) return;
    uploadFiles([picked], file.id);
  }

  return {
    allVisibleSelected,
    browse,
    bulkAction,
    busyId,
    canIngest,
    currentFolder,
    draggingFiles,
    downloadBatch,
    effectiveUploadVisibility,
    folderError,
    fileInputRef,
    files,
    filteredSelection,
    folderDialog,
    folderName,
    folders,
    folderVisibility,
    handleBulkDelete,
    handleBulkDownload,
    handleDeleteFile,
    handleDeleteFolder,
    handleDownload,
    handleDragEnter,
    handleDragLeave,
    handleDragOver,
    handleDrop,
    handleFileChange,
    handleFolderSubmit,
    handleMove,
    handlePreview,
    handleRename,
    hasActiveUploads,
    imagePreview,
    ingestBlockReason,
    i18nLanguage: i18n.language,
    isEmpty,
    load,
    loadError,
    loadState,
    moveDialog,
    navigateToFolder,
    openCreateFolder,
    openEditFolder,
    openMoveSelected,
    previewBusyId,
    renameTarget,
    resumeDownloadBatch,
    savingFolder,
    selectedCount,
    selectedVisibleSize,
    setAllVisibleSelected,
    setFileSelected,
    setFolderDialog,
    setFolderName,
    setFolderSelected,
    setFolderVisibility,
    setImagePreview,
    setMoveDialog,
    setRenameTarget,
    setUploadVisibility,
    setVersionsTarget,
    spaceId,
    stopDownloadBatch,
    timeZone,
    uploadNewVersion,
    uploadVisibility,
    versionsTarget,
    workspaceName,
  };
}
