import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  FolderOpen,
  HardDrive,
  Pencil,
  Plus,
  Search,
  Trash2,
  Users,
} from 'lucide-react';
import { Button, Dialog, FormMessage, ShareEditor, useFeedback } from '@miy/ui';

import { cn } from '../../lib/utils';
import { useDrive } from '../../platform/drive/drive-context';
import {
  buildSpacePath,
  buildTrashPath,
  useAppNavigate,
  type AppRoute,
} from '../../platform/runtime/app-location';
import { useNavigationDirty } from '../../platform/runtime/navigation-state';
import { useRuntime } from '../../platform/runtime/runtime-context';
import {
  createSpace,
  deleteSpace,
  directoryProvider,
  renameSpace,
  spaceShareProvider,
  type DriveSpace,
} from './api/files-api';
import { FilesSidebarFolders } from './sidebar-folders';

type SpaceDialog = { kind: 'create' } | { kind: 'rename' | 'members'; spaceId: string };

/**
 * AI-DO drew the Files navigation in the portal sidebar (manifest navItems
 * `files-all`, `files-search`, `files-chat` and the folder tree,
 * sidebar.tsx). The frame has no portal sidebar, so drive draws the same
 * items on its left with the space (AI-DO workspace) picker above them. The
 * documents AI chat is not ported. Space management (create, rename, members
 * with the common ShareEditor, delete) is miy's (AI-DO managed workspaces in
 * its admin console).
 */
export function DriveSidebar({
  activeFolderId,
  route,
  searchSelected,
}: {
  activeFolderId: string | null;
  route: AppRoute;
  searchSelected: boolean;
}) {
  const { t } = useTranslation('apps');
  const navigate = useAppNavigate();
  const { spaces } = useDrive();
  const spaceId = route.kind === 'home' ? null : route.spaceId;
  const space = spaces.find((item) => item.id === spaceId) ?? null;
  const [dialog, setDialog] = useState<SpaceDialog | null>(null);
  useEffect(() => {
    setDialog((current) => current && current.kind !== 'create' && current.spaceId !== spaceId ? null : current);
  }, [spaceId]);

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-app-border bg-app-surface">
      <div className="border-b border-app-border px-3 py-3">
        <div className="flex items-center gap-2 text-app-ink">
          <HardDrive size={16} className="text-app-ink/60" />
          <span className="app-text-body-sm font-semibold">{t('drive.title')}</span>
        </div>
        <label className="mt-3 block">
          <span className="sr-only">{t('drive.spaces.select')}</span>
          <select
            value={spaceId ?? ''}
            onChange={(event) => {
              if (event.currentTarget.value) {
                navigate(buildSpacePath(event.currentTarget.value));
              }
            }}
            className="app-text-body-sm h-9 w-full rounded-md border border-app-border bg-app-bg px-2 text-app-ink"
          >
            {spaceId === null ? (
              <option value="">{t('drive.spaces.select')}</option>
            ) : null}
            {spaces.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <div className="mt-2 flex flex-wrap gap-1">
          <Button size="dense" variant="ghost" onClick={() => setDialog({ kind: 'create' })}>
            <Plus size={14} />
            {t('drive.spaces.create')}
          </Button>
          {space?.can_manage ? (
            <>
              <Button
                size="icon"
                variant="ghost"
                title={t('drive.spaces.rename')}
                aria-label={t('drive.spaces.rename')}
                onClick={() => setDialog({ kind: 'rename', spaceId: space.id })}
              >
                <Pencil size={14} />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                title={t('drive.spaces.members')}
                aria-label={t('drive.spaces.members')}
                onClick={() => setDialog({ kind: 'members', spaceId: space.id })}
              >
                <Users size={14} />
              </Button>
            </>
          ) : null}
        </div>
      </div>

      {spaceId ? (
        <nav className="flex min-h-0 flex-1 flex-col px-1 py-2">
          <button
            type="button"
            onClick={() => navigate(buildSpacePath(spaceId))}
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              route.kind === 'space' &&
                !searchSelected &&
                activeFolderId === null &&
                'sidebar-submenu-item-active',
            )}
          >
            <FolderOpen size={14} className="text-app-ink/55" />
            <span className="sidebar-submenu-label truncate">{t('drive.nav.all')}</span>
          </button>
          <button
            type="button"
            onClick={() => navigate(buildSpacePath(spaceId, { view: 'search' }))}
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              route.kind === 'space' && searchSelected && 'sidebar-submenu-item-active',
            )}
          >
            <Search size={14} className="text-app-ink/55" />
            <span className="sidebar-submenu-label truncate">{t('drive.nav.search')}</span>
          </button>
          <button
            type="button"
            onClick={() => navigate(buildTrashPath(spaceId))}
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              route.kind === 'trash' && 'sidebar-submenu-item-active',
            )}
          >
            <Trash2 size={14} className="text-app-ink/55" />
            <span className="sidebar-submenu-label truncate">{t('drive.nav.trash')}</span>
          </button>
          <FilesSidebarFolders
            activeFolderId={route.kind === 'space' && !searchSelected ? activeFolderId : null}
            onOpenFolder={(folderId) => navigate(buildSpacePath(spaceId, { folder: folderId }))}
            rootActive={false}
            spaceId={spaceId}
          />
        </nav>
      ) : null}

      {dialog?.kind === 'create' ? (
        <SpaceNameDialog mode="create" onClose={() => setDialog(null)} space={null} />
      ) : null}
      {dialog?.kind === 'rename' && space?.id === dialog.spaceId ? (
        <SpaceNameDialog mode="rename" onClose={() => setDialog(null)} space={space} />
      ) : null}
      {dialog?.kind === 'members' && space?.id === dialog.spaceId ? (
        <SpaceMembersDialog onClose={() => setDialog(null)} space={space} />
      ) : null}
    </aside>
  );
}

function SpaceNameDialog({
  mode,
  onClose,
  space,
}: {
  mode: 'create' | 'rename';
  onClose: () => void;
  space: DriveSpace | null;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const runtime = useRuntime();
  const navigate = useAppNavigate();
  const feedback = useFeedback();
  const { reload } = useDrive();
  const initial = space?.name ?? '';
  const [name, setName] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useNavigationDirty(saving || deleting || name !== initial);

  const submit = async () => {
    setSaving(true);
    setError(null);
    try {
      if (mode === 'create') {
        const created = await createSpace(runtime, name.trim());
        await reload();
        feedback.success(t('drive.spaces.created', { name: created.name }));
        if (mounted.current) {
          onClose();
          navigate(buildSpacePath(created.id));
        }
      } else if (space) {
        await renameSpace(runtime, space.id, name.trim());
        await reload();
        if (mounted.current) onClose();
      }
    } catch (caught) {
      setError(
        caught instanceof Error && caught.message
          ? caught.message
          : t(mode === 'create' ? 'drive.spaces.createFailed' : 'drive.spaces.renameFailed'),
      );
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!space || !window.confirm(t('drive.spaces.deleteConfirm', { name: space.name }))) {
      return;
    }
    setDeleting(true);
    setError(null);
    try {
      await deleteSpace(runtime, space.id);
      await reload();
      if (mounted.current) {
        onClose();
        navigate('/', { replace: true });
      }
    } catch (caught) {
      setError(
        caught instanceof Error && caught.message
          ? caught.message
          : t('drive.spaces.deleteFailed'),
      );
    } finally {
      setDeleting(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t(mode === 'create' ? 'drive.spaces.createTitle' : 'drive.spaces.renameTitle')}
      closeLabel={t('common:actions.close')}
      actions={
        <>
          {mode === 'rename' && space ? (
            <Button variant="ghost" disabled={deleting || saving} onClick={() => void remove()}>
              <Trash2 size={14} />
              {t('drive.spaces.delete')}
            </Button>
          ) : null}
          <Button onClick={onClose}>{t('common:actions.cancel')}</Button>
          <Button
            variant="primary"
            disabled={saving || deleting || !name.trim() || name === initial}
            onClick={() => void submit()}
          >
            {t(mode === 'create' ? 'common:actions.create' : 'common:actions.save')}
          </Button>
        </>
      }
    >
      <label className="block">
        <span className="app-text-control-sm text-app-ink">{t('drive.spaces.label')}</span>
        <input
          value={name}
          maxLength={120}
          placeholder={t('drive.spaces.namePlaceholder')}
          onChange={(event) => setName(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing && name.trim()) {
              void submit();
            }
          }}
          className="app-text-body-sm mt-1 w-full rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none focus:border-app-accent"
        />
      </label>
      <FormMessage
        className="mt-3"
        id="drive-space-form-error"
        message={error}
        tone="error"
        variant="form"
      />
    </Dialog>
  );
}

function SpaceMembersDialog({
  onClose,
  space,
}: {
  onClose: () => void;
  space: DriveSpace;
}) {
  const { t } = useTranslation(['apps', 'common']);
  const runtime = useRuntime();
  const { userId, reload } = useDrive();
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
        if (!open) {
          onClose();
          void reload();
        }
      }}
      title={t('drive.spaces.membersTitle', { name: space.name })}
      description={t('drive.spaces.membersHint')}
      closeLabel={t('common:actions.close')}
      maxWidth="max-w-2xl"
    >
      <ShareEditor
        key={`${space.id}-${userId ?? ''}`}
        provider={provider}
        directoryProvider={people}
        currentUserId={userId ?? undefined}
        title={t('drive.spaces.members')}
        onDirtyChange={setDirty}
      />
    </Dialog>
  );
}
