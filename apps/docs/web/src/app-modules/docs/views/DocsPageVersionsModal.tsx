import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { History, Loader2, RotateCcw, X } from 'lucide-react';
import { useConfirm } from '@miy/ui';

import { BlockViewer, type BlockContent } from '../../../components/editor';
import { cn } from '../../../lib/utils';
import { useAppApi, useRuntime } from '../../../platform/runtime/runtime-context';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { formatDateTime } from '../../../platform/time/time-utils';
import {
  getDocPageVersion,
  listDocPageVersions,
  type DocsPageItem,
  type DocsPageVersion,
} from '../api/docs-api';

/**
 * A page's version history (miy, 보강; AI-DO kept none): every saved body
 * revision, newest first, with who saved it and when; a read-only preview;
 * and "restore", which saves that content as a new revision (history is
 * never rewritten). The screen owns the restore itself (`onRestore`), since
 * it must first settle the page's unsaved draft.
 */
export function DocsPageVersionsModal({
  page,
  canRestore,
  resolveFileUrl,
  onRestore,
  onClose,
}: {
  page: Pick<DocsPageItem, 'id' | 'title' | 'revision'>;
  canRestore: boolean;
  resolveFileUrl?: (url: string) => Promise<string>;
  onRestore: (version: DocsPageVersion) => Promise<boolean>;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const api = useAppApi();
  const runtime = useRuntime();
  const { confirm, confirmDialog } = useConfirm();
  const [versions, setVersions] = useState<DocsPageVersion[]>([]);
  const [serverCanRestore, setServerCanRestore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [preview, setPreview] = useState<DocsPageVersion | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [restoring, setRestoring] = useState(false);
  // The preview shown is the one last asked for; earlier answers are dropped.
  const previewRequest = useRef<string | null>(null);
  useNavigationDirty(restoring);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const response = await listDocPageVersions(api, page.id);
      setVersions(response.items);
      setServerCanRestore(response.can_restore);
      setError(null);
      setSelectedId((current) =>
        current && response.items.some((item) => item.id === current)
          ? current
          : (response.items[0]?.id ?? null),
      );
    } catch (caught) {
      setError(
        caught instanceof Error && caught.message
          ? caught.message
          : t('apps:docsApp.versions.loadFailed'),
      );
    } finally {
      setLoading(false);
    }
  }, [api, page.id, t]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!selectedId) {
      setPreview(null);
      return;
    }
    previewRequest.current = selectedId;
    setPreviewError(null);
    getDocPageVersion(api, page.id, selectedId)
      .then((version) => {
        if (previewRequest.current === selectedId) setPreview(version);
      })
      .catch((caught) => {
        if (previewRequest.current !== selectedId) return;
        setPreview(null);
        setPreviewError(
          caught instanceof Error && caught.message
            ? caught.message
            : t('apps:docsApp.versions.loadFailed'),
        );
      });
  }, [api, page.id, selectedId, t]);

  const restore = async (version: DocsPageVersion) => {
    const ok = await confirm({
      title: t('apps:docsApp.versions.restoreConfirm', { revision: version.revision }),
      description: t('apps:docsApp.versions.restoreDescription'),
      confirmLabel: t('apps:docsApp.versions.restore'),
      cancelLabel: t('common:actions.cancel'),
    });
    if (!ok) return;
    setRestoring(true);
    try {
      if (await onRestore(version)) await load();
    } finally {
      setRestoring(false);
    }
  };

  const shown = preview && preview.id === selectedId ? preview : null;
  const allowed = canRestore && serverCanRestore;

  return (
    <div className="fixed inset-0 z-[115] flex items-center justify-center">
      {confirmDialog}
      <button
        type="button"
        aria-label={t('common:actions.close')}
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="docs-versions-title"
        className="relative flex h-[80vh] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-app-border bg-app-surface-sidebar"
      >
        <div className="flex items-center justify-between gap-4 border-b border-app-border px-5 py-3">
          <div className="min-w-0">
            <h2 id="docs-versions-title" className="app-text-title-md flex items-center gap-2 text-app-ink">
              <History size={16} />
              {t('apps:docsApp.versions.title')}
            </h2>
            <p className="app-text-caption truncate text-app-ink/55">{page.title}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('common:actions.close')}
            className="rounded p-1.5 text-app-ink/55 hover:bg-app-surface-hover"
          >
            <X size={18} />
          </button>
        </div>
        <div className="flex min-h-0 flex-1">
          <ul
            aria-label={t('apps:docsApp.versions.title')}
            className="w-72 shrink-0 overflow-y-auto border-r border-app-border py-1"
          >
            {loading && !versions.length ? (
              <li className="flex justify-center py-8">
                <Loader2 size={20} className="animate-spin text-app-accent" />
              </li>
            ) : null}
            {error ? (
              <li role="alert" className="app-text-caption px-4 py-3 text-app-danger">
                {error}
              </li>
            ) : null}
            {versions.map((version) => (
              <li key={version.id}>
                <button
                  type="button"
                  aria-current={version.id === selectedId}
                  onClick={() => setSelectedId(version.id)}
                  className={cn(
                    'flex w-full flex-col gap-0.5 px-4 py-2 text-left hover:bg-app-surface-hover',
                    version.id === selectedId && 'bg-app-surface-hover',
                  )}
                >
                  <span className="app-text-control-sm text-app-ink">
                    {formatDateTime(version.created_at, { locale, timeZone: runtime.settings.timeZone })}
                  </span>
                  <span className="app-text-caption text-app-ink/55">
                    {version.created_by_name || t('apps:docsApp.versions.unknownAuthor')}
                    {' · '}
                    {t('apps:docsApp.versions.revision', { revision: version.revision })}
                    {version.is_current ? ` · ${t('apps:docsApp.versions.current')}` : ''}
                  </span>
                  {version.restored_from_revision ? (
                    <span className="app-text-caption text-app-ink/45">
                      {t('apps:docsApp.versions.restoredFrom', {
                        revision: version.restored_from_revision,
                      })}
                    </span>
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex items-center justify-between gap-3 border-b border-app-border px-5 py-2">
              <span className="app-text-caption text-app-ink/55">
                {shown ? shown.title : ''}
              </span>
              {shown && allowed && !shown.is_current ? (
                <button
                  type="button"
                  disabled={restoring}
                  onClick={() => void restore(shown)}
                  className="app-text-control-sm inline-flex items-center gap-1.5 rounded-md bg-app-accent px-3 py-1.5 text-app-accent-fg hover:opacity-90 disabled:opacity-50"
                >
                  {restoring ? <Loader2 size={13} className="animate-spin" /> : <RotateCcw size={13} />}
                  {t('apps:docsApp.versions.restore')}
                </button>
              ) : null}
            </div>
            <div className="prose dark:prose-invert min-h-0 max-w-none flex-1 overflow-y-auto bg-white px-6 py-5 dark:bg-app-surface">
              {previewError ? (
                <p role="alert" className="app-text-caption text-app-danger">{previewError}</p>
              ) : shown ? (
                <BlockViewer
                  key={shown.id}
                  content={(shown.content_blocks ?? []) as BlockContent}
                  resolveFileUrl={resolveFileUrl}
                />
              ) : selectedId ? (
                <div className="flex justify-center py-8">
                  <Loader2 size={20} className="animate-spin text-app-accent" />
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
