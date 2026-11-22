import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  History,
  PencilRuler,
  Plus,
  Star,
  Trash2,
  User,
  Users,
  type LucideIcon,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  Button,
  Dialog,
  FormMessage,
  Input,
  Select,
  ShareEditor,
  useFeedback,
} from '@miy/ui';

import { cn } from '../../lib/utils';
import { apiErrorMessage } from '../../platform/api/api-error';
import {
  GLOBAL_SPACE_ID,
  readSpaceId,
  useAppNavigate,
  useAppSearchParams,
  type AppRoute,
} from '../../platform/runtime/app-location';
import { useNavigationDirty } from '../../platform/runtime/navigation-state';
import { useRuntime } from '../../platform/runtime/runtime-context';
import {
  createSpace,
  deleteSpace,
  directoryProvider,
  listSpaces,
  spaceShareProvider,
  type WhiteboardSpace,
} from '../../platform/whiteboard/spaces-api';

import { viewFromSearch, type WhiteboardHubView } from './views/whiteboard-hub-model';

/**
 * AI-DO app-modules/whiteboard manifest.ts nav items (All, My, Recent,
 * Favorites, Trash) and sidebar.tsx's create action. AI-DO drew them in the
 * portal's sub-sidebar; the frame draws them at its left edge and links
 * become `runtime.navigate`. miy addition: the space picker (AI-DO's
 * workspace switcher lived in the portal shell), with space creation and the
 * members editor (SDK resource ACL).
 */
const NAV_ITEMS: Array<{ view: WhiteboardHubView; labelKey: string; icon: LucideIcon }> = [
  { view: 'all', labelKey: 'shell:nav.whiteboard-all', icon: PencilRuler },
  { view: 'mine', labelKey: 'shell:nav.whiteboard-my', icon: User },
  { view: 'recent', labelKey: 'shell:nav.whiteboard-recent', icon: History },
  { view: 'favorites', labelKey: 'shell:nav.whiteboard-favorites', icon: Star },
  { view: 'archived', labelKey: 'shell:nav.whiteboard-archived', icon: Trash2 },
];

function hubPath(spaceId: string, view: WhiteboardHubView): string {
  const params = new URLSearchParams();
  if (view !== 'all') params.set('view', view);
  if (spaceId !== GLOBAL_SPACE_ID) params.set('space', spaceId);
  const query = params.toString();
  return `/${query ? `?${query}` : ''}`;
}

export function WhiteboardSidebar({ route }: { route: AppRoute }) {
  const { t } = useTranslation(['apps', 'shell']);
  const runtime = useRuntime();
  const { api } = runtime;
  const navigate = useAppNavigate();
  const feedback = useFeedback();
  const [searchParams] = useAppSearchParams();
  const spaceId = readSpaceId(searchParams);
  const activeView = route.kind === 'hub' ? viewFromSearch(searchParams.get('view')) : null;
  const [spaces, setSpaces] = useState<WhiteboardSpace[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [creating, setCreating] = useState(false);
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    listSpaces(api)
      .then((items) => {
        if (cancelled) return;
        setSpaces(items);
        setLoadError(null);
      })
      .catch(() => {
        if (cancelled) return;
        setLoadError(t('whiteboardApp.spaces.loadFailed'));
      });
    return () => {
      cancelled = true;
    };
  }, [api, reloadKey, t]);

  const space = spaces.find((item) => item.id === spaceId) ?? null;
  const spaceOptions = useMemo(
    () => [
      { label: t('whiteboardApp.spaces.global'), value: GLOBAL_SPACE_ID },
      ...spaces.map((item) => ({ label: item.name, value: item.id })),
    ],
    [spaces, t],
  );

  const go = useCallback((path: string) => navigate(path), [navigate]);

  const removeSpace = async () => {
    if (!space) return;
    if (!window.confirm(t('whiteboardApp.spaces.deleteConfirm', { name: space.name })))
      return;
    try {
      await deleteSpace(api, space.id);
      go(hubPath(GLOBAL_SPACE_ID, 'all'));
      setReloadKey((key) => key + 1);
    } catch (error) {
      feedback.error(apiErrorMessage(error, t('whiteboardApp.spaces.deleteFailed')));
    }
  };

  return (
    <nav
      aria-label={t('whiteboardApp.title')}
      className="flex h-full w-56 shrink-0 flex-col gap-5 overflow-y-auto border-r border-app-border bg-app-surface-sidebar px-2 py-3"
    >
      <section className="space-y-2 px-1">
        <div className="sidebar-section-label px-2 py-1 text-app-ink/45">
          {t('whiteboardApp.spaces.label')}
        </div>
        <Select
          className="w-full"
          aria-label={t('whiteboardApp.spaces.select')}
          options={spaceOptions}
          placeholder={t('whiteboardApp.spaces.select')}
          value={spaceId}
          onValueChange={(next) => go(hubPath(next, activeView ?? 'all'))}
        />
        {loadError ? (
          <p className="px-2 app-text-caption text-app-danger dark:text-app-danger-text">{loadError}</p>
        ) : null}
        <div className="flex flex-wrap gap-1">
          <Button size="dense" variant="ghost" onClick={() => setCreating(true)}>
            <Plus size={14} />
            {t('whiteboardApp.spaces.create')}
          </Button>
          {space?.canManage ? (
            <>
              <Button size="dense" variant="ghost" onClick={() => setSharing(true)}>
                <Users size={14} />
                {t('whiteboardApp.spaces.members')}
              </Button>
              <Button size="dense" variant="ghost" onClick={() => void removeSpace()}>
                <Trash2 size={14} />
                {t('whiteboardApp.spaces.delete')}
              </Button>
            </>
          ) : null}
        </div>
      </section>

      <section className="space-y-1 border-t border-app-border pt-3">
        <div className="sidebar-section-label px-3 py-1 text-app-ink/45">
          {t('whiteboard.whiteboard')}
        </div>
        {NAV_ITEMS.map(({ view, labelKey, icon: Icon }) => (
          <button
            key={view}
            type="button"
            className={cn(
              'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
              activeView === view && 'sidebar-submenu-item-active',
            )}
            onClick={() => go(hubPath(spaceId, view))}
          >
            <Icon size={16} className="text-app-ink/55 dark:text-app-ink/65" />
            <span className="sidebar-submenu-label">{t(labelKey)}</span>
          </button>
        ))}
        <button
          type="button"
          className="sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left"
          onClick={() => window.dispatchEvent(new CustomEvent('whiteboard:create'))}
        >
          <Plus size={16} className="text-app-ink/55 dark:text-app-ink/65" />
          <span className="sidebar-submenu-label">{t('whiteboard.new')}</span>
        </button>
      </section>

      {creating ? (
        <SpaceCreateDialog
          onClose={() => setCreating(false)}
          onCreated={(created) => {
            setCreating(false);
            setReloadKey((key) => key + 1);
            go(hubPath(created.id, 'all'));
          }}
        />
      ) : null}
      {sharing && space ? (
        <SpaceMembersDialog space={space} onClose={() => setSharing(false)} />
      ) : null}
    </nav>
  );
}

function SpaceCreateDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (space: WhiteboardSpace) => void;
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
      feedback.success(t('whiteboardApp.spaces.created', { name: created.name }));
      onCreated(created);
    } catch (caught) {
      setError(apiErrorMessage(caught, t('whiteboardApp.spaces.createFailed')));
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t('whiteboardApp.spaces.createTitle')}
      closeLabel={t('whiteboardApp.spaces.close')}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('whiteboardApp.spaces.cancel')}
          </Button>
          <Button
            disabled={!name.trim() || saving}
            variant="primary"
            onClick={() => void submit()}
          >
            {t('whiteboardApp.spaces.create')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Input
          aria-label={t('whiteboardApp.spaces.namePlaceholder')}
          autoFocus
          maxLength={120}
          placeholder={t('whiteboardApp.spaces.namePlaceholder')}
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <textarea
          aria-label={t('whiteboardApp.spaces.descriptionPlaceholder')}
          className="app-text-body w-full resize-none rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none focus:border-app-accent"
          placeholder={t('whiteboardApp.spaces.descriptionPlaceholder')}
          rows={3}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
        <FormMessage id="whiteboard-space-error" message={error} tone="error" variant="form" />
      </div>
    </Dialog>
  );
}

function SpaceMembersDialog({
  space,
  onClose,
}: {
  space: WhiteboardSpace;
  onClose: () => void;
}) {
  const { t } = useTranslation('apps');
  const runtime = useRuntime();
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
      title={t('whiteboardApp.spaces.membersTitle', { name: space.name })}
      closeLabel={t('whiteboardApp.spaces.close')}
      maxWidth="max-w-2xl"
    >
      <ShareEditor
        key={space.id}
        provider={provider}
        directoryProvider={people}
        title={t('whiteboardApp.spaces.members')}
        onDirtyChange={setDirty}
      />
    </Dialog>
  );
}
