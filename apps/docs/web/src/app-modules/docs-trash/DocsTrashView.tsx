import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FileText, Files, Loader2, RotateCcw, Trash2 } from 'lucide-react';
import { useConfirm, useFeedback } from '@miy/ui';

import { useAppNavigate, useAppSearchParams } from '../../platform/runtime/app-location';
import { useNavigationDirty } from '../../platform/runtime/navigation-state';
import { useAppApi, useRuntime } from '../../platform/runtime/runtime-context';
import {
  listDocsTrash,
  purgeDocsTrashEntry,
  restoreDocsTrashEntry,
  type DocsTrashEntry,
} from '../docs/api/docs-api';
import { buildDocsDocPath } from '../docs/docs-sidebar-model';
import { refreshDocsSidebar } from '../docs/sidebar';
import { timeAgo } from '../docs/views/DocsViewParts';

/**
 * The space's trash (miy, 보강: AI-DO trashed docs and pages without a way
 * back). Docs the person manages and pages of docs they edit, one entry per
 * deletion; restore, or delete for good when they manage the doc. Emptying
 * is manual (no automatic retention).
 */
export function DocsTrashView() {
  const { t, i18n } = useTranslation(['apps', 'common']);
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const api = useAppApi();
  const runtime = useRuntime();
  const navigate = useAppNavigate();
  const feedback = useFeedback();
  const { confirm, confirmDialog } = useConfirm();
  const [searchParams] = useAppSearchParams();
  const spaceId = searchParams.get('space')?.trim() || 'global';
  const [entries, setEntries] = useState<DocsTrashEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  // A late answer for another space never replaces the list shown.
  const requested = useRef(spaceId);
  useNavigationDirty(busyId !== null);

  const load = useCallback(async () => {
    requested.current = spaceId;
    setLoading(true);
    try {
      const response = await listDocsTrash(api, spaceId);
      if (requested.current !== spaceId) return;
      setEntries(response.items);
      setError(null);
    } catch (caught) {
      if (requested.current !== spaceId) return;
      setError(caught instanceof Error && caught.message ? caught.message : t('apps:docsApp.loadFailed'));
    } finally {
      if (requested.current === spaceId) setLoading(false);
    }
  }, [api, spaceId, t]);

  useEffect(() => {
    setEntries([]);
    void load();
  }, [load]);

  const restore = async (entry: DocsTrashEntry) => {
    setBusyId(entry.id);
    try {
      const result = (await restoreDocsTrashEntry(api, entry)) as { moved_to_root?: boolean };
      feedback.success(
        result?.moved_to_root
          ? t('apps:docsApp.trash.restoredToRoot', { title: entry.title })
          : t('apps:docsApp.trash.restored', { title: entry.title }),
      );
      setEntries((current) => current.filter((item) => item.id !== entry.id));
      refreshDocsSidebar();
    } catch (caught) {
      feedback.error(caught instanceof Error && caught.message ? caught.message : t('apps:docsApp.actionFailed'));
      void load();
    } finally {
      setBusyId(null);
    }
  };

  const purge = async (entry: DocsTrashEntry) => {
    const ok = await confirm({
      title: t('apps:docsApp.trash.deleteConfirm', { title: entry.title }),
      description: t('apps:docsApp.trash.deleteDescription'),
      confirmLabel: t('apps:docsApp.trash.deleteForever'),
      cancelLabel: t('common:actions.cancel'),
      variant: 'danger',
    });
    if (!ok) return;
    setBusyId(entry.id);
    try {
      await purgeDocsTrashEntry(api, entry);
      setEntries((current) => current.filter((item) => item.id !== entry.id));
    } catch (caught) {
      feedback.error(caught instanceof Error && caught.message ? caught.message : t('apps:docsApp.actionFailed'));
      void load();
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-app-bg text-app-ink">
      {confirmDialog}
      <header className="border-b border-app-border bg-app-bg px-8 py-5">
        <h1 className="app-text-title-lg text-app-ink">{t('apps:docsApp.trash.title')}</h1>
        <p className="app-text-caption text-app-ink/55">{t('apps:docsApp.trash.description')}</p>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-8 py-5">
        {loading && entries.length === 0 ? (
          <div className="flex justify-center py-16">
            <Loader2 size={28} className="animate-spin text-app-accent" />
          </div>
        ) : error ? (
          <p role="alert" className="app-text-body text-app-danger">{error}</p>
        ) : entries.length === 0 ? (
          <p className="app-text-body py-16 text-center text-app-ink/55">{t('apps:docsApp.trash.empty')}</p>
        ) : (
          <ul className="divide-y divide-app-border rounded-lg border border-app-border">
            {entries.map((entry) => {
              const Icon = entry.kind === 'doc' ? Files : FileText;
              return (
                <li key={`${entry.kind}:${entry.id}`} className="flex items-center gap-3 px-4 py-3">
                  <Icon size={16} className="shrink-0 text-app-ink/55" />
                  <div className="min-w-0 flex-1">
                    <div className="app-text-control truncate text-app-ink">{entry.title}</div>
                    <div className="app-text-caption truncate text-app-ink/55">
                      {entry.kind === 'doc' ? (
                        t('apps:docsApp.trash.kindDoc')
                      ) : (
                        <button
                          type="button"
                          className="hover:text-app-accent hover:underline"
                          onClick={() => navigate(buildDocsDocPath({ docId: entry.doc_id, spaceId }))}
                        >
                          {t('apps:docsApp.trash.kindPage', { doc: entry.doc_title })}
                        </button>
                      )}
                      {entry.subpage_count ? ` · ${t('apps:docsApp.trash.subpages', { count: entry.subpage_count })}` : ''}
                      {entry.parent_in_trash ? ` · ${t('apps:docsApp.trash.parentInTrash')}` : ''}
                      {' · '}
                      {t('apps:docsApp.trash.trashedBy', {
                        name: entry.trashed_by_name ?? '',
                        time: timeAgo(entry.trashed_at, runtime.settings.timeZone, locale),
                      })}
                    </div>
                  </div>
                  {entry.can_restore ? (
                    <button
                      type="button"
                      disabled={busyId !== null}
                      onClick={() => void restore(entry)}
                      className="app-text-control-sm inline-flex items-center gap-1.5 rounded-md border border-app-border px-2.5 py-1.5 text-app-ink hover:bg-app-surface-hover disabled:opacity-50"
                    >
                      <RotateCcw size={14} />
                      {t('apps:docsApp.trash.restore')}
                    </button>
                  ) : null}
                  {entry.can_delete ? (
                    <button
                      type="button"
                      disabled={busyId !== null}
                      onClick={() => void purge(entry)}
                      className="app-text-control-sm inline-flex items-center gap-1.5 rounded-md border border-app-border px-2.5 py-1.5 text-app-danger-text hover:bg-app-surface-hover disabled:opacity-50"
                    >
                      <Trash2 size={14} />
                      {t('apps:docsApp.trash.deleteForever')}
                    </button>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
