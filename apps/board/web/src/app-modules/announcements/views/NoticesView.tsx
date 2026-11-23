import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, Users } from 'lucide-react';
import {
  Button,
  Dialog,
  FormMessage,
  Input,
  Select,
  ShareEditor,
  useFeedback,
} from '@miy/ui';

import { apiErrorMessage } from '../../../platform/api/api-error';
import {
  createSpace,
  deleteSpace,
  directoryProvider,
  listSpaces,
  spaceShareProvider,
  type BoardSpace,
} from '../../../platform/board/board-api';
import { useBoard } from '../../../platform/board/board-context';
import {
  buildNoticesUrl,
  useAppNavigate,
  useAppSearchParams,
} from '../../../platform/runtime/app-location';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';

import { AnnouncementsBoard } from './AnnouncementsBoard';

/**
 * Notices screen (miy). AI-DO showed two AnnouncementsBoards on the workspace
 * home: the current workspace's notices and the company notices. Here the
 * workspace is a board space chosen on this screen (`?space=`); the company
 * board needs no space. Board managers create spaces; a space admin manages
 * its members with the common ShareEditor (SDK resource ACL).
 */
export function NoticesView({ noticeId }: { noticeId: string | null }) {
  const { t } = useTranslation('apps');
  const runtime = useRuntime();
  const { api } = runtime;
  const { me } = useBoard();
  const navigate = useAppNavigate();
  const feedback = useFeedback();
  const [searchParams] = useAppSearchParams();
  const requestedSpaceId = searchParams.get('space');
  const [spaces, setSpaces] = useState<BoardSpace[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadFailed, setLoadFailed] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const [creating, setCreating] = useState(false);
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listSpaces(api)
      .then((items) => {
        if (cancelled) return;
        setSpaces(items);
        setLoadFailed(false);
      })
      .catch(() => {
        if (cancelled) return;
        setSpaces([]);
        setLoadFailed(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, reloadKey]);

  const space =
    spaces.find((item) => item.id === requestedSpaceId) ?? spaces[0] ?? null;
  const spaceId = space?.id ?? null;

  const selectSpace = useCallback(
    (nextSpaceId: string) => {
      navigate(buildNoticesUrl(nextSpaceId), { replace: true });
    },
    [navigate],
  );

  const onOpenNoticeChange = useCallback(
    (nextNoticeId: string | null) => {
      if (nextNoticeId === noticeId) return;
      navigate(buildNoticesUrl(spaceId ?? requestedSpaceId, nextNoticeId), {
        replace: true,
      });
    },
    [navigate, noticeId, requestedSpaceId, spaceId],
  );

  const spaceOptions = useMemo(
    () => spaces.map((item) => ({ label: item.name, value: item.id })),
    [spaces],
  );

  const removeSpace = async () => {
    if (!space) return;
    if (!window.confirm(t('board.spaces.deleteConfirm', { name: space.name })))
      return;
    try {
      await deleteSpace(api, space.id);
      navigate(buildNoticesUrl(), { replace: true });
      setReloadKey((key) => key + 1);
    } catch (error) {
      feedback.error(apiErrorMessage(error, t('board.spaces.deleteFailed')));
    }
  };

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-app-bg text-app-ink">
      <header className="border-b border-app-border bg-app-bg px-4 py-4">
        <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center justify-between gap-3">
          <h1 className="app-text-title text-app-ink">{t('board.notices')}</h1>
          <div className="flex flex-wrap items-center gap-2">
            {spaces.length > 0 ? (
              <label className="flex items-center gap-2 app-text-caption text-app-ink/55">
                <span>{t('board.spaces.label')}</span>
                <Select
                  className="min-w-[180px]"
                  options={spaceOptions}
                  placeholder={t('board.spaces.select')}
                  value={spaceId ?? ''}
                  onValueChange={selectSpace}
                />
              </label>
            ) : null}
            {space?.canManage ? (
              <>
                <Button size="dense" variant="secondary" onClick={() => setSharing(true)}>
                  <Users size={15} />
                  {t('board.spaces.members')}
                </Button>
                <Button size="dense" variant="ghost" onClick={() => void removeSpace()}>
                  <Trash2 size={15} />
                  {t('board.spaces.delete')}
                </Button>
              </>
            ) : null}
            {me?.isManager ? (
              <Button size="dense" variant="primary" onClick={() => setCreating(true)}>
                <Plus size={15} />
                {t('board.spaces.create')}
              </Button>
            ) : null}
          </div>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto grid w-full max-w-6xl gap-4 px-4 py-4 lg:grid-cols-2">
          <div className="min-h-[320px]">
            {space ? (
              <AnnouncementsBoard
                key={`workspace-${space.id}`}
                scope="workspace"
                title={`${space.name} ${t('home.announcements')}`}
                spaceId={space.id}
                spaceCanManage={space.canManage}
                openNoticeId={noticeId}
                onOpenNoticeChange={onOpenNoticeChange}
              />
            ) : (
              <section className="flex h-full flex-col justify-center rounded-lg border border-dashed border-app-border bg-app-surface p-4 text-center">
                <p className="app-text-body text-app-ink/55">
                  {loading ? t('board.loading') : t(loadFailed ? 'board.spaces.loadFailed' : 'board.spaces.none')}
                </p>
              </section>
            )}
          </div>
          <div className="min-h-[320px]">
            <AnnouncementsBoard
              key={`company-${spaceId ?? 'none'}`}
              scope="company"
              title={t('home.announcementsCompany')}
              spaceId={spaceId}
              openNoticeId={noticeId}
              onOpenNoticeChange={onOpenNoticeChange}
            />
          </div>
        </div>
      </div>
      {creating ? (
        <SpaceCreateDialog
          onClose={() => setCreating(false)}
          onCreated={(created) => {
            setCreating(false);
            setReloadKey((key) => key + 1);
            navigate(buildNoticesUrl(created.id), { replace: true });
          }}
        />
      ) : null}
      {sharing && space ? (
        <SpaceMembersDialog space={space} onClose={() => setSharing(false)} />
      ) : null}
    </div>
  );
}

function SpaceCreateDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (space: BoardSpace) => void;
}) {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const feedback = useFeedback();
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useNavigationDirty(saving || name.trim() !== '' || description.trim() !== '');

  const submit = async () => {
    if (!name.trim() || saving) return;
    setSaving(true);
    setError(null);
    try {
      const created = await createSpace(api, {
        name: name.trim(),
        description: description.trim(),
      });
      feedback.success(t('board.spaces.created', { name: created.name }));
      onCreated(created);
    } catch (caught) {
      setError(apiErrorMessage(caught, t('board.spaces.createFailed')));
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t('board.spaces.createTitle')}
      closeLabel={t('board.spaces.close')}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('community.cancel')}
          </Button>
          <Button
            disabled={!name.trim() || saving}
            variant="primary"
            onClick={() => void submit()}
          >
            {t('board.spaces.create')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Input
          aria-label={t('board.spaces.namePlaceholder')}
          autoFocus
          maxLength={120}
          placeholder={t('board.spaces.namePlaceholder')}
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <textarea
          aria-label={t('board.spaces.descriptionPlaceholder')}
          className="app-text-body w-full resize-none rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none focus:border-app-accent"
          placeholder={t('board.spaces.descriptionPlaceholder')}
          rows={3}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
        <FormMessage id="board-space-error" message={error} tone="error" variant="form" />
      </div>
    </Dialog>
  );
}

function SpaceMembersDialog({
  space,
  onClose,
}: {
  space: BoardSpace;
  onClose: () => void;
}) {
  const { t } = useTranslation('apps');
  const runtime = useRuntime();
  const { me } = useBoard();
  const [dirty, setDirty] = useState(false);
  useNavigationDirty(dirty);
  const provider = useMemo(
    () => spaceShareProvider(runtime.api, space.id),
    [runtime.api, space.id],
  );
  const people = useMemo(
    () => directoryProvider(runtime.directory),
    [runtime.directory],
  );
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t('board.spaces.membersTitle', { name: space.name })}
      closeLabel={t('board.spaces.close')}
      maxWidth="max-w-2xl"
    >
      <ShareEditor
        key={`${space.id}-${me?.userId ?? ''}`}
        provider={provider}
        directoryProvider={people}
        currentUserId={me?.userId}
        title={t('board.spaces.members')}
        onDirtyChange={setDirty}
      />
    </Dialog>
  );
}
