import { useEffect, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Megaphone, Pencil, Pin, Plus, Trash2, X } from 'lucide-react';
import { Button } from '@miy/ui';

import { useIsBoardManager } from '../../../platform/board/board-context';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import {
  useAppApi,
  useRuntime,
} from '../../../platform/runtime/runtime-context';
import {
  formatDateTime,
  normalizeTimeZone,
} from '../../../platform/time/time-utils';
import {
  createAnnouncement,
  deleteAnnouncement,
  getAnnouncement,
  listAnnouncements,
  updateAnnouncement,
  type Announcement,
  type AnnouncementScope,
} from '../api/announcements-api';

/*
 * AI-DO app-modules/home/views/WorkspaceHomeView/AnnouncementsBoard.tsx. In
 * AI-DO the workspace home showed two of these (workspace and company). miy:
 * the space comes from the notices screen (`spaceId`), the API client is
 * `runtime.api`, a company notice is written by a board manager (AI-DO:
 * platform admin) and a space notice by a space admin (AI-DO: workspace
 * admin/owner). A notification link opens one notice (`openNoticeId`).
 */

const NOTICE_PAGE_SIZE = 8;

function AnnouncementRow({
  announcement,
  timeZone,
  locale,
  onSelect,
}: {
  announcement: Announcement;
  timeZone: string;
  locale: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="-mx-2 flex w-full items-center gap-3 border-b border-app-border px-2 py-3 text-left transition-colors last:border-b-0 hover:bg-app-surface-hover"
    >
      {announcement.isPinned ? (
        <Pin size={15} className="shrink-0 text-app-accent" />
      ) : (
        <Megaphone size={15} className="shrink-0 text-app-ink/45" />
      )}
      <span className="app-text-body min-w-0 flex-1 truncate text-app-ink">
        {announcement.title}
      </span>
      <span className="app-text-caption shrink-0 text-app-ink/55">
        {announcement.authorName}
      </span>
      <span className="app-text-caption shrink-0 text-app-ink/45">
        {formatDateTime(announcement.createdAt, {
          locale,
          month: 'short',
          day: 'numeric',
          timeZone,
        })}
      </span>
    </button>
  );
}

function AnnouncementEditorModal({
  scope,
  spaceId,
  title,
  announcement,
  onClose,
  onSaved,
}: {
  scope: AnnouncementScope;
  spaceId: string | null;
  title: string;
  announcement: Announcement | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const isEdit = announcement !== null;
  const [draftTitle, setDraftTitle] = useState(announcement?.title ?? '');
  const [body, setBody] = useState(announcement?.body ?? '');
  const [pinned, setPinned] = useState(announcement?.isPinned ?? false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // miy contract: unsaved notice drafts are reported to the portal.
  useNavigationDirty(
    submitting ||
      draftTitle !== (announcement?.title ?? '') ||
      body !== (announcement?.body ?? '') ||
      pinned !== (announcement?.isPinned ?? false),
  );

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!api || !draftTitle.trim() || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      if (isEdit) {
        await updateAnnouncement(api, spaceId, announcement.id, {
          title: draftTitle.trim(),
          body: body.trim(),
          isPinned: pinned,
        });
      } else {
        await createAnnouncement(api, spaceId, {
          title: draftTitle.trim(),
          body: body.trim(),
          scope,
          isPinned: pinned,
        });
      }
      onSaved();
    } catch {
      setError(
        isEdit
          ? t('home.announcementUpdateError')
          : t('home.announcementSubmitError'),
      );
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div className="w-full max-w-md rounded-xl border border-app-border bg-app-surface p-5 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h3 className="app-text-title-md text-app-ink">
            {isEdit
              ? t('home.announcementEditTitle')
              : t('home.announcementCreate')}{' '}
            · {title}
          </h3>
          <button
            type="button"
            onClick={onClose}
            className="text-app-ink/45 transition-colors hover:text-app-ink"
            aria-label={t('home.announcementCancel')}
          >
            <X size={18} />
          </button>
        </div>
        <form onSubmit={submit} className="space-y-3">
          <input
            type="text"
            value={draftTitle}
            onChange={(event) => setDraftTitle(event.target.value)}
            placeholder={t('home.announcementTitlePlaceholder')}
            maxLength={200}
            autoFocus
            className="app-text-body w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none focus:border-app-accent"
          />
          <textarea
            value={body}
            onChange={(event) => setBody(event.target.value)}
            placeholder={t('home.announcementBodyPlaceholder')}
            rows={5}
            className="app-text-body w-full resize-none rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none focus:border-app-accent"
          />
          <label className="app-text-body-sm flex items-center gap-2 text-app-ink">
            <input
              type="checkbox"
              checked={pinned}
              onChange={(event) => setPinned(event.target.checked)}
            />
            {t('home.announcementPin')}
          </label>
          {error ? (
            <p className="app-text-caption text-app-danger">{error}</p>
          ) : null}
          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="app-text-body-sm rounded-md border border-app-border px-3 py-2 text-app-ink transition-colors hover:bg-app-surface-hover"
            >
              {t('home.announcementCancel')}
            </button>
            <button
              type="submit"
              disabled={!draftTitle.trim() || submitting}
              className="app-text-body-sm flex items-center gap-1.5 rounded-md bg-app-accent px-3 py-2 text-app-accent-fg transition-opacity disabled:opacity-50"
            >
              {submitting ? (
                <Loader2 size={14} className="animate-spin" />
              ) : null}
              {t('home.announcementSubmit')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function AnnouncementDetailModal({
  announcement,
  spaceId,
  timeZone,
  locale,
  canManage,
  onClose,
  onEdit,
  onDeleted,
}: {
  announcement: Announcement;
  spaceId: string | null;
  timeZone: string;
  locale: string;
  canManage: boolean;
  onClose: () => void;
  onEdit: () => void;
  onDeleted: () => void;
}) {
  const { t } = useTranslation('apps');
  const api = useAppApi();
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const remove = async () => {
    if (!api || deleting) return;
    if (!window.confirm(t('home.announcementDeleteConfirm'))) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteAnnouncement(api, spaceId, announcement.id);
      onDeleted();
    } catch {
      setError(t('home.announcementDeleteError'));
      setDeleting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4">
      <div className="flex max-h-[80vh] w-full max-w-md flex-col rounded-xl border border-app-border bg-app-surface p-5 shadow-xl">
        <div className="mb-3 flex items-start justify-between gap-3">
          <div className="flex min-w-0 items-start gap-2">
            {announcement.isPinned ? (
              <Pin size={16} className="mt-0.5 shrink-0 text-app-accent" />
            ) : (
              <Megaphone
                size={16}
                className="mt-0.5 shrink-0 text-app-ink/45"
              />
            )}
            <h3 className="app-text-title-md break-words text-app-ink">
              {announcement.title}
            </h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="shrink-0 text-app-ink/45 transition-colors hover:text-app-ink"
            aria-label={t('home.announcementClose')}
          >
            <X size={18} />
          </button>
        </div>
        <div className="app-text-caption mb-3 flex items-center gap-2 text-app-ink/55">
          <span>{announcement.authorName}</span>
          <span>·</span>
          <span>
            {formatDateTime(announcement.createdAt, {
              locale,
              year: 'numeric',
              month: 'short',
              day: 'numeric',
              hour: '2-digit',
              minute: '2-digit',
              timeZone,
            })}
          </span>
        </div>
        <div className="app-text-body min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap break-words text-app-ink">
          {announcement.body.trim() ? (
            announcement.body
          ) : (
            <span className="text-app-ink/55">
              {t('home.announcementNoBody')}
            </span>
          )}
        </div>
        {error ? (
          <p className="app-text-caption mt-3 text-app-danger">{error}</p>
        ) : null}
        {canManage ? (
          <div className="mt-4 flex justify-end gap-2 border-t border-app-border pt-3">
            <button
              type="button"
              onClick={remove}
              disabled={deleting}
              className="app-text-body-sm flex items-center gap-1.5 rounded-md border border-app-border px-3 py-2 text-app-danger transition-colors hover:bg-app-surface-hover disabled:opacity-50"
            >
              {deleting ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Trash2 size={14} />
              )}
              {t('home.announcementDelete')}
            </button>
            <button
              type="button"
              onClick={onEdit}
              className="app-text-body-sm flex items-center gap-1.5 rounded-md border border-app-border px-3 py-2 text-app-ink transition-colors hover:bg-app-surface-hover"
            >
              <Pencil size={14} className="text-app-ink/55" />
              {t('home.announcementEdit')}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function AnnouncementsBoard({
  scope,
  title,
  spaceId = null,
  spaceCanManage = false,
  openNoticeId = null,
  onOpenNoticeChange,
}: {
  scope: AnnouncementScope;
  title: string;
  spaceId?: string | null;
  spaceCanManage?: boolean;
  openNoticeId?: string | null;
  onOpenNoticeChange?: (noticeId: string | null) => void;
}) {
  const { t, i18n } = useTranslation('apps');
  const api = useAppApi();
  const { settings } = useRuntime();
  const isBoardManager = useIsBoardManager();
  const timeZone = normalizeTimeZone(settings.timeZone);
  const canCreate = scope === 'company' ? isBoardManager : spaceCanManage;

  const [items, setItems] = useState<Announcement[]>([]);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [reloadKey, setReloadKey] = useState(0);
  const [listFailed, setListFailed] = useState(false);
  const [detailFailed, setDetailFailed] = useState(false);
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<Announcement | null>(null);
  const [editing, setEditing] = useState<Announcement | null>(null);

  useEffect(() => {
    if (!api || (scope === 'workspace' && !spaceId)) return undefined;
    let cancelled = false;
    setLoading(true);
    setListFailed(false);
    listAnnouncements(api, spaceId, {
      scope,
      limit: NOTICE_PAGE_SIZE,
      offset: page * NOTICE_PAGE_SIZE,
    })
      .then((response) => {
        if (cancelled) return;
        setItems(response.items);
        setHasMore(response.hasMore);
      })
      .catch(() => {
        if (cancelled) return;
        setItems([]);
        setHasMore(false);
        setListFailed(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, spaceId, scope, page, reloadKey]);

  // A notification link can open a notice on any page.
  useEffect(() => {
    setDetailFailed(false);
    setSelected((current) => current?.id === openNoticeId ? current : null);
    if (!api || !openNoticeId) return undefined;
    let cancelled = false;
    getAnnouncement(api, spaceId, openNoticeId)
      .then((notice) => {
        if (!cancelled && notice.scope === scope) setSelected(notice);
      })
      .catch(() => {
        // Both notice boards receive the route. Show a single error.
        if (!cancelled && scope === 'company') setDetailFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [api, openNoticeId, scope, spaceId, reloadKey]);

  const reload = () => setReloadKey((key) => key + 1);
  const select = (notice: Announcement | null) => {
    setSelected(notice);
    onOpenNoticeChange?.(notice ? notice.id : null);
  };

  return (
    <section className="flex h-full flex-col rounded-lg border border-app-border bg-app-surface p-4">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <Megaphone size={16} className="shrink-0 text-app-accent" />
          <h2 className="app-text-title-md truncate text-app-ink">{title}</h2>
        </div>
        {canCreate ? (
          <button
            type="button"
            onClick={() => setCreating(true)}
            className="app-text-caption flex shrink-0 items-center gap-1 rounded-md border border-app-border px-2.5 py-1 text-app-ink transition-colors hover:border-app-accent/40 hover:bg-app-surface-hover"
          >
            <Plus size={12} className="text-app-ink/55" />
            {t('home.announcementCreate')}
          </button>
        ) : null}
      </div>
      {detailFailed ? (
        <div className="flex items-center justify-between gap-2 py-2" role="alert">
          <span>{t('home.announcementDetailError')}</span>
          <Button size="dense" variant="secondary" onClick={reload}>{t('home.announcementRetry')}</Button>
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex justify-center py-6">
            <Loader2 size={16} className="animate-spin text-app-ink/45" />
          </div>
        ) : listFailed ? (
          <div className="flex items-center justify-between gap-2 py-2" role="alert">
            <span>{t('home.announcementsLoadError')}</span>
            <Button size="dense" variant="secondary" onClick={reload}>{t('home.announcementRetry')}</Button>
          </div>
        ) : items.length === 0 ? (
          <p className="app-text-body py-6 text-center text-app-ink/55">
            {t('home.announcementsEmpty')}
          </p>
        ) : (
          items.map((announcement) => (
            <AnnouncementRow
              key={announcement.id}
              announcement={announcement}
              timeZone={timeZone}
              locale={i18n.language}
              onSelect={() => select(announcement)}
            />
          ))
        )}
      </div>
      {page > 0 || hasMore ? (
        <div className="mt-2 flex justify-end gap-2">
          <Button
            disabled={loading || page === 0}
            onClick={() => setPage((current) => current - 1)}
            size="dense"
            variant="secondary"
          >
            {t('community.previousPage')}
          </Button>
          <Button
            disabled={loading || !hasMore}
            onClick={() => setPage((current) => current + 1)}
            size="dense"
            variant="secondary"
          >
            {t('community.nextPage')}
          </Button>
        </div>
      ) : null}
      {creating ? (
        <AnnouncementEditorModal
          scope={scope}
          spaceId={spaceId}
          title={title}
          announcement={null}
          onClose={() => setCreating(false)}
          onSaved={() => {
            setCreating(false);
            setPage(0);
            reload();
          }}
        />
      ) : null}
      {editing ? (
        <AnnouncementEditorModal
          scope={scope}
          spaceId={spaceId}
          title={title}
          announcement={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            select(null);
            reload();
          }}
        />
      ) : null}
      {selected && !editing ? (
        <AnnouncementDetailModal
          announcement={selected}
          spaceId={spaceId}
          timeZone={timeZone}
          locale={i18n.language}
          canManage={selected.canManage}
          onClose={() => select(null)}
          onEdit={() => setEditing(selected)}
          onDeleted={() => {
            select(null);
            reload();
          }}
        />
      ) : null}
    </section>
  );
}
