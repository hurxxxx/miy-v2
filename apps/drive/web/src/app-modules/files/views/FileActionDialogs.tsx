import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Download, Folder, FolderOpen, Loader2, Upload, X } from 'lucide-react';
import { FormMessage, useFeedback } from '@miy/ui';

import { cn } from '../../../lib/utils';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import {
  getFileVersionDownloadUrl,
  listFileVersions,
  type FileFolderItem,
  type FileItem,
  type FileVersionItem,
} from '../api/files-api';
import { buildFolderTree, type FolderNode } from '../folder-tree-model';
import { startDownloads } from './file-downloads';
import { formatFileSize, formatFileUpdatedAt } from './file-manager-view-model';
import type { MoveDialogState } from './useFileManagerController';

/**
 * miy (보강): rename, move and versions dialogs. AI-DO had no such actions;
 * the markup follows AI-DO's folder dialog (FileManagerView `FolderDialog`).
 */
function DialogFrame({
  children,
  labelledBy,
  onClose,
  title,
}: {
  children: React.ReactNode;
  labelledBy: string;
  onClose: () => void;
  title: string;
}) {
  const { t } = useTranslation(['apps', 'common']);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 p-4">
      <dialog
        open
        className="flex max-h-[85vh] w-full max-w-md flex-col rounded-md border border-app-border bg-app-surface p-4 shadow-2xl"
        aria-labelledby={labelledBy}
      >
        <div className="flex items-center justify-between gap-3">
          <h2 id={labelledBy} className="app-text-title truncate text-app-ink">
            {title}
          </h2>
          <button
            type="button"
            onClick={onClose}
            title={t('common:actions.close')}
            aria-label={t('common:actions.close')}
            className="flex size-8 shrink-0 items-center justify-center rounded-md text-app-ink/55 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
          >
            <X size={16} />
          </button>
        </div>
        {children}
      </dialog>
    </div>
  );
}

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

export function RenameFileDialog({
  file,
  onClose,
  onSubmit,
}: {
  file: FileItem;
  onClose: () => void;
  onSubmit: (name: string) => Promise<unknown>;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const [name, setName] = useState(file.filename);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useNavigationDirty(saving || name !== file.filename);
  const submit = async () => {
    setSaving(true);
    setError(null);
    try {
      await onSubmit(name.trim());
    } catch (caught) {
      setError(errorText(caught, t('drive.rename.failed')));
    } finally {
      setSaving(false);
    }
  };
  return (
    <DialogFrame
      labelledBy="drive-rename-dialog-title"
      onClose={onClose}
      title={t('drive.rename.title')}
    >
      <label className="app-text-control-sm mt-4 block text-app-ink">
        <span>{t('drive.rename.label')}</span>
        <input
          value={name}
          maxLength={512}
          onChange={(event) => setName(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing && name.trim()) {
              void submit();
            }
          }}
          className="app-text-body-sm mt-1 w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none transition-colors focus:border-app-accent"
        />
      </label>
      <FormMessage
        className="mt-4"
        id="drive-rename-form-error"
        message={error}
        tone="error"
        variant="form"
      />
      <div className="mt-5 flex justify-end gap-2">
        <button
          type="button"
          onClick={onClose}
          className="inline-flex h-9 items-center rounded-md border border-app-border px-3 app-text-control text-app-ink transition-colors hover:bg-app-surface-hover"
        >
          {t('common:actions.cancel')}
        </button>
        <button
          type="button"
          onClick={() => void submit()}
          disabled={saving || !name.trim() || name === file.filename}
          className="inline-flex h-9 items-center gap-2 rounded-md bg-app-accent px-3 app-text-control text-app-accent-fg transition-opacity disabled:opacity-60"
        >
          {saving ? <Loader2 size={16} className="animate-spin" /> : null}
          {t('common:actions.save')}
        </button>
      </div>
    </DialogFrame>
  );
}

/** Folders under any selected folder cannot be targets. */
export function excludedMoveTargets(
  folders: readonly FileFolderItem[],
  selectedFolderIds: readonly string[],
): Set<string> {
  const children = new Map<string, string[]>();
  for (const folder of folders) {
    if (!folder.parent_id) continue;
    const list = children.get(folder.parent_id) ?? [];
    list.push(folder.id);
    children.set(folder.parent_id, list);
  }
  const excluded = new Set<string>();
  const stack = [...selectedFolderIds];
  while (stack.length > 0) {
    const current = stack.pop() as string;
    if (excluded.has(current)) continue;
    excluded.add(current);
    stack.push(...(children.get(current) ?? []));
  }
  return excluded;
}

export function MoveItemsDialog({
  folders,
  onClose,
  onSubmit,
  selection,
}: {
  folders: readonly FileFolderItem[];
  onClose: () => void;
  onSubmit: (targetFolderId: string | null) => Promise<unknown>;
  selection: MoveDialogState;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const [target, setTarget] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useNavigationDirty(saving);
  const excluded = useMemo(
    () => excludedMoveTargets(folders, selection.folderIds),
    [folders, selection.folderIds],
  );
  const tree = useMemo(() => buildFolderTree([...folders]), [folders]);
  const count = selection.fileIds.length + selection.folderIds.length;
  const submit = async () => {
    setSaving(true);
    setError(null);
    try {
      await onSubmit(target);
    } catch (caught) {
      setError(errorText(caught, t('drive.move.failed')));
    } finally {
      setSaving(false);
    }
  };
  const renderNode = (node: FolderNode, level: number): React.ReactNode => {
    if (excluded.has(node.id)) return null;
    return (
      <div key={node.id}>
        <button
          type="button"
          onClick={() => setTarget(node.id)}
          aria-pressed={target === node.id}
          className={cn(
            'sidebar-submenu-item w-full',
            target === node.id && 'sidebar-submenu-item-active',
          )}
          style={{ paddingLeft: `${12 + level * 12}px` }}
        >
          <Folder size={14} className="text-app-ink/55" />
          <span className="sidebar-submenu-label truncate">{node.name}</span>
        </button>
        {node.children.map((child) => renderNode(child, level + 1))}
      </div>
    );
  };
  return (
    <DialogFrame
      labelledBy="drive-move-dialog-title"
      onClose={onClose}
      title={t('drive.move.title', { count })}
    >
      <p className="app-text-caption mt-3 text-app-ink/60">{t('drive.move.target')}</p>
      <div className="mt-2 min-h-32 flex-1 space-y-0.5 overflow-y-auto rounded-md border border-app-border p-1">
        <button
          type="button"
          onClick={() => setTarget(null)}
          aria-pressed={target === null}
          className={cn(
            'sidebar-submenu-item w-full',
            target === null && 'sidebar-submenu-item-active',
          )}
        >
          <FolderOpen size={14} className="text-app-ink/55" />
          <span className="sidebar-submenu-label truncate">{t('files.sidebar.root')}</span>
        </button>
        {tree.map((node) => renderNode(node, 1))}
      </div>
      <FormMessage
        className="mt-4"
        id="drive-move-form-error"
        message={error}
        tone="error"
        variant="form"
      />
      <div className="mt-5 flex justify-end gap-2">
        <button
          type="button"
          onClick={onClose}
          className="inline-flex h-9 items-center rounded-md border border-app-border px-3 app-text-control text-app-ink transition-colors hover:bg-app-surface-hover"
        >
          {t('common:actions.cancel')}
        </button>
        <button
          type="button"
          onClick={() => void submit()}
          disabled={saving}
          className="inline-flex h-9 items-center gap-2 rounded-md bg-app-accent px-3 app-text-control text-app-accent-fg transition-opacity disabled:opacity-60"
        >
          {saving ? <Loader2 size={16} className="animate-spin" /> : null}
          {t('drive.move.submit')}
        </button>
      </div>
    </DialogFrame>
  );
}

export function FileVersionsDialog({
  file,
  onClose,
  onUploadVersion,
  spaceId,
  timeZone,
}: {
  file: FileItem;
  onClose: () => void;
  onUploadVersion: (file: File | undefined) => void;
  spaceId: string;
  timeZone: string;
}) {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const runtime = useRuntime();
  const feedback = useFeedback();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [versions, setVersions] = useState<FileVersionItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    setVersions(null);
    setError(null);
    listFileVersions(runtime, spaceId, file.id, { signal: controller.signal })
      .then((response) => {
        if (!controller.signal.aborted) setVersions(response.versions);
      })
      .catch((caught) => {
        if (!controller.signal.aborted) {
          setError(errorText(caught, t('drive.versions.loadFailed')));
        }
      });
    return () => controller.abort();
  }, [file.id, runtime, spaceId, t]);

  const download = async (version: FileVersionItem) => {
    setBusyId(version.id);
    try {
      await startDownloads(runtime, [version.id], (versionId) =>
        getFileVersionDownloadUrl(runtime, spaceId, file.id, versionId),
      );
    } catch (caught) {
      feedback.error(errorText(caught, t('files.errors.downloadFailed')));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <DialogFrame
      labelledBy="drive-versions-dialog-title"
      onClose={onClose}
      title={t('drive.versions.title', { name: file.filename })}
    >
      <FormMessage
        className="mt-4"
        id="drive-versions-error"
        message={error}
        tone="error"
        variant="form"
      />
      {versions === null && !error ? (
        <div className="flex h-24 items-center justify-center text-app-ink/55">
          <Loader2 size={20} className="animate-spin" />
        </div>
      ) : null}
      {versions && versions.length === 0 ? (
        <p className="app-text-body-sm mt-4 text-app-ink/55">{t('drive.versions.empty')}</p>
      ) : null}
      {versions && versions.length > 0 ? (
        <ul className="mt-3 flex-1 divide-y divide-app-border overflow-y-auto rounded-md border border-app-border">
          {versions.map((version) => (
            <li key={version.id} className="flex items-center gap-3 px-3 py-2">
              <div className="min-w-0 flex-1">
                <p className="app-text-body-sm truncate font-semibold text-app-ink">
                  {t('drive.versions.version', { number: version.version_number })}
                  {version.is_current ? (
                    <span className="ml-2 rounded-full border border-app-accent/40 px-1.5 py-0.5 app-text-micro text-app-accent">
                      {t('drive.versions.current')}
                    </span>
                  ) : null}
                </p>
                <p className="app-text-caption truncate text-app-ink/55">
                  {version.filename} · {formatFileSize(version.size_bytes)}
                </p>
                <p className="app-text-caption truncate text-app-ink/55">
                  {t('drive.versions.uploadedBy', {
                    name: version.uploader_name || version.uploaded_by,
                    date: formatFileUpdatedAt(version.created_at, i18n.language, timeZone),
                  })}
                </p>
              </div>
              <button
                type="button"
                onClick={() => void download(version)}
                disabled={busyId === version.id}
                title={t('files.actions.download')}
                aria-label={t('files.actions.download')}
                className="flex size-8 shrink-0 items-center justify-center rounded-md text-app-ink/60 transition-colors hover:bg-app-surface-hover hover:text-app-ink disabled:opacity-60"
              >
                {busyId === version.id ? (
                  <Loader2 size={15} className="animate-spin" />
                ) : (
                  <Download size={15} />
                )}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {file.can_delete ? (
        <div className="mt-4 flex justify-end">
          <button
            type="button"
            onClick={() => inputRef.current?.click()}
            className="inline-flex h-9 items-center gap-2 rounded-md bg-app-accent px-3 app-text-control text-app-accent-fg transition-opacity disabled:opacity-60"
          >
            <Upload size={16} />
            {t('drive.actions.uploadVersion')}
          </button>
          <input
            ref={inputRef}
            type="file"
            className="hidden"
            aria-label={t('drive.actions.uploadVersion')}
            onChange={(event) => {
              const picked = event.currentTarget.files?.[0];
              event.currentTarget.value = '';
              onUploadVersion(picked);
            }}
          />
        </div>
      ) : null}
    </DialogFrame>
  );
}
