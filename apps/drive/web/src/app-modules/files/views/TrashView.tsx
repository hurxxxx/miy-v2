import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { File, Folder, Loader2, RefreshCw, RotateCcw, Trash2 } from 'lucide-react';
import { ContentState, StatusSlot, useFeedback } from '@miy/ui';

import { cn } from '../../../lib/utils';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import { normalizeTimeZone } from '../../../platform/time/time-utils';
import {
  deleteTrashItemsForever,
  emptyTrash,
  listTrash,
  restoreTrashItems,
  type TrashItem,
  type TrashResponse,
} from '../api/files-api';
import { FILES_CHANGED_EVENT } from '../file-upload-session';
import { formatFileSize, formatFileUpdatedAt } from './file-manager-view-model';

type TrashAction = 'restore' | 'delete' | 'empty' | null;

function itemKey(item: Pick<TrashItem, 'kind' | 'id'>): string {
  return `${item.kind}:${item.id}`;
}

/** The selection as the server's bulk payload. */
export function trashPayload(items: readonly TrashItem[], keys: ReadonlySet<string>) {
  const chosen = items.filter((item) => keys.has(itemKey(item)));
  return {
    fileIds: chosen.filter((item) => item.kind === 'file').map((item) => item.id),
    folderIds: chosen.filter((item) => item.kind === 'folder').map((item) => item.id),
  };
}

/**
 * miy (보강): the trash. AI-DO deleted for good (soft delete plus storage
 * removal). The layout follows the file manager (header, selection bar,
 * list); restoring returns an item to its folder, or to the space root when
 * that folder is gone.
 */
export function TrashView({ spaceId }: { spaceId: string }) {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const runtime = useRuntime();
  const feedback = useFeedback();
  const timeZone = normalizeTimeZone(runtime.settings.timeZone);
  const [state, setState] = useState<{ spaceId: string; data: TrashResponse } | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [action, setAction] = useState<TrashAction>(null);
  const sequence = useRef(0);
  useNavigationDirty(action !== null);

  const load = useCallback(async () => {
    sequence.current += 1;
    const current = sequence.current;
    setLoading(true);
    setLoadError(null);
    try {
      const data = await listTrash(runtime, spaceId);
      if (sequence.current !== current) return;
      setState({ spaceId, data });
    } catch (caught) {
      if (sequence.current !== current) return;
      setLoadError(
        caught instanceof Error && caught.message ? caught.message : t('drive.trash.loadFailed'),
      );
    } finally {
      if (sequence.current === current) setLoading(false);
    }
  }, [runtime, spaceId, t]);

  useEffect(() => {
    setSelected(new Set());
    void load();
  }, [load]);

  const items = useMemo(
    () => (state?.spaceId === spaceId ? state.data.items : []),
    [spaceId, state],
  );
  const visibleKeys = useMemo(() => new Set(items.map(itemKey)), [items]);
  const selection = useMemo(
    () => new Set([...selected].filter((key) => visibleKeys.has(key))),
    [selected, visibleKeys],
  );
  const allSelected = items.length > 0 && selection.size === items.length;

  const run = async (kind: Exclude<TrashAction, null>) => {
    // The target is fixed now: a late reload does not change it.
    const payload = trashPayload(items, selection);
    if (kind !== 'empty' && payload.fileIds.length + payload.folderIds.length === 0) return;
    if (kind === 'delete') {
      const count = payload.fileIds.length + payload.folderIds.length;
      if (!window.confirm(t('drive.trash.confirmDelete', { count }))) return;
    }
    if (kind === 'empty' && !window.confirm(t('drive.trash.confirmEmpty'))) return;
    setAction(kind);
    try {
      if (kind === 'restore') {
        await restoreTrashItems(runtime, spaceId, payload);
        feedback.success(t('drive.trash.restored'));
        window.dispatchEvent(
          new CustomEvent(FILES_CHANGED_EVENT, { detail: { spaceId, folderId: null } }),
        );
      } else if (kind === 'delete') {
        await deleteTrashItemsForever(runtime, spaceId, payload);
        feedback.success(t('drive.trash.deleted'));
      } else {
        await emptyTrash(runtime, spaceId);
        feedback.success(t('drive.trash.deleted'));
      }
      setSelected(new Set());
      await load();
    } catch (caught) {
      feedback.error(
        caught instanceof Error && caught.message
          ? caught.message
          : t(kind === 'restore' ? 'drive.trash.restoreFailed' : 'drive.trash.deleteFailed'),
      );
    } finally {
      setAction(null);
    }
  };

  const toggle = (key: string, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(key);
      else next.delete(key);
      return next;
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-app-bg">
      <header className="border-b border-app-border bg-app-bg px-5 py-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-app-ink/60">
              <Trash2 size={18} />
              <span className="app-text-overline">{state?.data.space.name ?? ''}</span>
            </div>
            <h1 className="app-text-title mt-1 truncate text-app-ink">{t('drive.trash.title')}</h1>
            <p className="mt-1 app-text-body-sm text-app-ink/60">{t('drive.trash.description')}</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void load()}
              className="inline-flex h-9 items-center gap-2 rounded-md border border-app-border px-3 app-text-control text-app-ink transition-colors hover:bg-app-surface-hover"
            >
              <RefreshCw size={16} />
              {t('files.actions.refresh')}
            </button>
            <button
              type="button"
              onClick={() => void run('empty')}
              disabled={action !== null || items.length === 0}
              className="inline-flex h-9 items-center gap-2 rounded-md border border-app-danger/30 px-3 app-text-control text-app-danger transition-colors hover:bg-app-danger/10 disabled:opacity-60"
            >
              {action === 'empty' ? <Loader2 size={16} className="animate-spin" /> : <Trash2 size={16} />}
              {t('drive.actions.emptyTrash')}
            </button>
          </div>
        </div>
      </header>

      <StatusSlot
        announce="polite"
        className="mx-5 mt-4 app-text-body-sm"
        message={state ? loadError : null}
        size="compact"
        tone="error"
      />

      <main className="relative min-h-0 flex-1 overflow-y-auto p-5">
        {loadError && !state ? <ContentState kind="error" size="fill" title={loadError} /> : null}
        {loading && !state ? (
          <div className="flex h-64 items-center justify-center text-app-ink/55">
            <Loader2 size={24} className="animate-spin" />
          </div>
        ) : null}
        {state && items.length === 0 ? (
          <div className="flex h-64 items-center justify-center rounded-md border border-dashed border-app-border">
            <span className="app-text-body text-app-ink/55">{t('drive.trash.empty')}</span>
          </div>
        ) : null}
        {items.length > 0 ? (
          <>
            <div className="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-md border border-app-border bg-app-surface px-3 py-2">
              <label className="inline-flex min-w-0 items-center gap-2 app-text-body-sm text-app-ink">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={(event) =>
                    setSelected(event.currentTarget.checked ? new Set(visibleKeys) : new Set())
                  }
                  className="size-4 rounded border-app-border accent-app-accent"
                />
                <span className="truncate">
                  {t('files.selection.summary', { count: selection.size })}
                </span>
              </label>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  onClick={() => void run('restore')}
                  disabled={action !== null || selection.size === 0}
                  className="inline-flex h-8 items-center gap-2 rounded-md border border-app-border px-2.5 app-text-control-sm text-app-ink transition-colors hover:bg-app-surface-hover disabled:opacity-60"
                >
                  {action === 'restore' ? <Loader2 size={14} className="animate-spin" /> : <RotateCcw size={14} />}
                  {t('drive.actions.restore')}
                </button>
                <button
                  type="button"
                  onClick={() => void run('delete')}
                  disabled={action !== null || selection.size === 0}
                  className="inline-flex h-8 items-center gap-2 rounded-md border border-app-danger/30 px-2.5 app-text-control-sm text-app-danger transition-colors hover:bg-app-danger/10 disabled:opacity-60"
                >
                  {action === 'delete' ? <Loader2 size={14} className="animate-spin" /> : <Trash2 size={14} />}
                  {t('drive.actions.deleteForever')}
                </button>
              </div>
            </div>
            <ul className="divide-y divide-app-border rounded-md border border-app-border bg-app-surface">
              {items.map((item) => {
                const key = itemKey(item);
                const Icon = item.kind === 'folder' ? Folder : File;
                return (
                  <li
                    key={key}
                    className={cn(
                      'flex items-start gap-3 px-3 py-3',
                      selection.has(key) && 'bg-app-accent/5',
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={selection.has(key)}
                      onChange={(event) => toggle(key, event.currentTarget.checked)}
                      aria-label={t('drive.trash.select', { name: item.name })}
                      className="mt-1 size-4 shrink-0 rounded border-app-border accent-app-accent"
                    />
                    <span className="flex size-9 shrink-0 items-center justify-center rounded-md bg-app-surface-hover text-app-ink/65">
                      <Icon size={18} />
                    </span>
                    <div className="min-w-0 flex-1">
                      <p className="app-text-body-sm truncate font-semibold text-app-ink">{item.name}</p>
                      <p className="app-text-caption truncate text-app-ink/55">
                        {t('drive.trash.location', {
                          path: item.original_path || t('files.sidebar.root'),
                        })}
                      </p>
                      <p className="app-text-caption truncate text-app-ink/55">
                        {item.kind === 'folder'
                          ? t('drive.trash.folderFiles', { count: item.file_count })
                          : formatFileSize(item.size_bytes)}
                        {' · '}
                        {t('drive.trash.deletedAt', {
                          date: formatFileUpdatedAt(item.deleted_at, i18n.language, timeZone),
                        })}
                        {item.trashed_by_name
                          ? ` · ${t('drive.trash.deletedBy', { name: item.trashed_by_name })}`
                          : ''}
                      </p>
                    </div>
                  </li>
                );
              })}
            </ul>
          </>
        ) : null}
      </main>
    </div>
  );
}
