import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, Users } from 'lucide-react';
import {
  Button,
  ContentState,
  Dialog,
  DirectoryPicker,
  FormMessage,
  Input,
  ShareEditor,
  useFeedback,
  type DirectoryOption,
} from '@miy/ui';

import { apiErrorMessage } from '../../platform/api/api-error';
import {
  addDocsManager,
  bootstrapDocsManager,
  createSpace,
  deleteSpace,
  directoryProvider,
  listDocsManagers,
  removeDocsManager,
  spaceShareProvider,
  type DocsManager,
  type DocsSpace,
} from '../../platform/docs/docs-admin-api';
import { useDocsApp } from '../../platform/docs/docs-context';
import { useNavigationDirty } from '../../platform/runtime/navigation-state';
import { useRuntime } from '../../platform/runtime/runtime-context';

const BUTTON =
  'app-text-body-sm rounded-md border border-app-border px-3 py-1.5 text-app-ink hover:bg-app-surface-hover disabled:opacity-50';

function useDirectoryLabels() {
  const { t } = useTranslation('apps');
  return useMemo(
    () => ({
      users: t('docsApp.directory.users'),
      groups: t('docsApp.directory.groups'),
      searchPlaceholder: t('docsApp.directory.searchPlaceholder'),
      loading: t('docsApp.directory.loading'),
      noMatches: t('docsApp.directory.noMatches'),
      unavailable: t('docsApp.directory.unavailable'),
      retry: t('docsApp.directory.retry'),
      loadMore: t('docsApp.directory.loadMore'),
      currentUser: t('docsApp.directory.currentUser'),
      locked: t('docsApp.directory.locked'),
    }),
    [t],
  );
}

/**
 * Docs settings (miy). AI-DO kept docs spaces in the platform launcher (the
 * space chooser and app-space administration) and the app role in the admin
 * console; here docs managers (board_managers pattern) create spaces, and a
 * space admin manages its members with the common ShareEditor (SDK ACL).
 */
export function DocsSettingsView() {
  const { t } = useTranslation('apps');
  const { me, spaces, loading, reload } = useDocsApp();
  const managedSpaces = spaces.filter((space) => space.canManage);

  if (loading && !me) {
    return <ContentState kind="loading" size="fill" title={t('docsApp.loading')} />;
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-app-bg text-app-ink">
      <header className="border-b border-app-border bg-app-bg px-4 py-4">
        <h1 className="app-text-title text-app-ink">{t('docsApp.settings.title')}</h1>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto grid w-full max-w-4xl gap-4 px-4 py-4">
          {me?.canBootstrap ? <Bootstrap onDone={reload} /> : null}
          <SpacesSection
            canCreate={Boolean(me?.isManager)}
            spaces={managedSpaces}
            onChanged={reload}
          />
          {me?.isManager ? <Managers /> : null}
        </div>
      </div>
    </div>
  );
}

function Bootstrap({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const feedback = useFeedback();
  const [busy, setBusy] = useState(false);
  const claim = async () => {
    setBusy(true);
    try {
      await bootstrapDocsManager(api);
      feedback.success(t('docsApp.bootstrap.claimed'));
    } catch (error) {
      feedback.error(apiErrorMessage(error, t('docsApp.bootstrap.failed')));
    } finally {
      setBusy(false);
      onDone();
    }
  };
  return (
    <section className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-app-border bg-app-surface p-4">
      <div className="min-w-0">
        <p className="app-text-body-sm font-semibold text-app-ink">
          {t('docsApp.bootstrap.title')}
        </p>
        <p className="app-text-caption text-app-ink/60">{t('docsApp.bootstrap.body')}</p>
      </div>
      <Button disabled={busy} size="dense" variant="secondary" onClick={() => void claim()}>
        {t('docsApp.bootstrap.claim')}
      </Button>
    </section>
  );
}

function SpacesSection({
  canCreate,
  spaces,
  onChanged,
}: {
  canCreate: boolean;
  spaces: DocsSpace[];
  onChanged: () => void;
}) {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const feedback = useFeedback();
  const [creating, setCreating] = useState(false);
  const [sharing, setSharing] = useState<DocsSpace | null>(null);

  const removeSpace = async (space: DocsSpace) => {
    if (!window.confirm(t('docsApp.spaces.deleteConfirm', { name: space.name }))) return;
    try {
      await deleteSpace(api, space.id);
      onChanged();
    } catch (error) {
      feedback.error(apiErrorMessage(error, t('docsApp.spaces.deleteFailed')));
    }
  };

  return (
    <section className="grid gap-3 rounded-md border border-app-border p-4">
      <div className="flex items-center justify-between gap-2">
        <div className="app-text-title-sm text-app-ink">{t('docsApp.spaces.title')}</div>
        {canCreate ? (
          <Button size="dense" variant="primary" onClick={() => setCreating(true)}>
            <Plus size={15} />
            {t('docsApp.spaces.create')}
          </Button>
        ) : null}
      </div>
      <p className="app-text-caption text-app-ink/60">{t('docsApp.spaces.description')}</p>
      <ul className="grid gap-1">
        {spaces.length === 0 ? (
          <li className="app-text-body-sm text-app-ink/45">{t('docsApp.spaces.none')}</li>
        ) : (
          spaces.map((space) => (
            <li
              key={space.id}
              className="flex items-center justify-between gap-2 rounded-md border border-app-border px-3 py-2"
            >
              <span className="app-text-body-sm min-w-0 truncate text-app-ink">{space.name}</span>
              <span className="flex shrink-0 gap-2">
                <button className={BUTTON} type="button" onClick={() => setSharing(space)}>
                  <Users size={14} className="mr-1 inline" />
                  {t('docsApp.spaces.members')}
                </button>
                <button className={BUTTON} type="button" onClick={() => void removeSpace(space)}>
                  <Trash2 size={14} className="mr-1 inline" />
                  {t('docsApp.spaces.delete')}
                </button>
              </span>
            </li>
          ))
        )}
      </ul>
      {creating ? (
        <SpaceCreateDialog
          onClose={() => setCreating(false)}
          onCreated={() => {
            setCreating(false);
            onChanged();
          }}
        />
      ) : null}
      {sharing ? (
        <SpaceMembersDialog space={sharing} onClose={() => setSharing(null)} />
      ) : null}
    </section>
  );
}

function SpaceCreateDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (space: DocsSpace) => void;
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
      feedback.success(t('docsApp.spaces.created', { name: created.name }));
      onCreated(created);
    } catch (caught) {
      // The typed name and description stay for another try.
      setError(apiErrorMessage(caught, t('docsApp.spaces.createFailed')));
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t('docsApp.spaces.createTitle')}
      closeLabel={t('docsApp.close')}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('docsApp.cancel')}
          </Button>
          <Button disabled={!name.trim() || saving} variant="primary" onClick={() => void submit()}>
            {t('docsApp.spaces.create')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Input
          aria-label={t('docsApp.spaces.namePlaceholder')}
          autoFocus
          maxLength={120}
          placeholder={t('docsApp.spaces.namePlaceholder')}
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <textarea
          aria-label={t('docsApp.spaces.descriptionPlaceholder')}
          className="app-text-body w-full resize-none rounded-md border border-app-border bg-app-bg px-3 py-2 text-app-ink outline-none focus:border-app-accent"
          placeholder={t('docsApp.spaces.descriptionPlaceholder')}
          rows={3}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
        <FormMessage id="docs-space-error" message={error} tone="error" variant="form" />
      </div>
    </Dialog>
  );
}

function SpaceMembersDialog({ space, onClose }: { space: DocsSpace; onClose: () => void }) {
  const { t } = useTranslation('apps');
  const runtime = useRuntime();
  const { me } = useDocsApp();
  const [dirty, setDirty] = useState(false);
  useNavigationDirty(dirty);
  const provider = useMemo(() => spaceShareProvider(runtime.api, space.id), [runtime.api, space.id]);
  const people = useMemo(() => directoryProvider(runtime.directory), [runtime.directory]);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t('docsApp.spaces.membersTitle', { name: space.name })}
      closeLabel={t('docsApp.close')}
      maxWidth="max-w-2xl"
    >
      <ShareEditor
        key={`${space.id}-${me?.userId ?? ''}`}
        provider={provider}
        directoryProvider={people}
        currentUserId={me?.userId}
        title={t('docsApp.spaces.members')}
        onDirtyChange={setDirty}
      />
    </Dialog>
  );
}

function Managers() {
  const { t } = useTranslation('apps');
  const runtime = useRuntime();
  const { api } = runtime;
  const { me, reload } = useDocsApp();
  const feedback = useFeedback();
  const [items, setItems] = useState<DocsManager[]>([]);
  const [selected, setSelected] = useState<DirectoryOption[]>([]);
  const [busy, setBusy] = useState(false);
  const people = useMemo(() => directoryProvider(runtime.directory), [runtime.directory]);
  const directoryLabels = useDirectoryLabels();
  useNavigationDirty(busy || selected.length > 0);

  const load = useCallback(async () => {
    try {
      setItems(await listDocsManagers(api));
    } catch (caught) {
      feedback.error(apiErrorMessage(caught, t('docsApp.loadFailed')));
    }
  }, [api, feedback, t]);

  useEffect(() => {
    void load();
  }, [load]);

  const add = async () => {
    const person = selected[0];
    if (!person || busy) return;
    setBusy(true);
    try {
      await addDocsManager(api, person.id);
      setSelected([]);
      feedback.success(t('docsApp.managers.added', { name: person.name }));
      await load();
    } catch (caught) {
      feedback.error(apiErrorMessage(caught, t('docsApp.managers.addFailed')));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (manager: DocsManager) => {
    setBusy(true);
    try {
      await removeDocsManager(api, manager.userId);
      feedback.success(t('docsApp.managers.removed', { name: manager.name ?? manager.userId }));
      await load();
      if (manager.userId === me?.userId) reload();
    } catch (caught) {
      feedback.error(apiErrorMessage(caught, t('docsApp.managers.removeFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="grid gap-3 rounded-md border border-app-border p-4">
      <div className="app-text-title-sm text-app-ink">{t('docsApp.managers.title')}</div>
      <p className="app-text-caption text-app-ink/60">{t('docsApp.managers.description')}</p>
      <ul className="grid gap-1">
        {items.length === 0 ? (
          <li className="app-text-body-sm text-app-ink/45">{t('docsApp.managers.empty')}</li>
        ) : (
          items.map((manager) => (
            <li
              className="flex items-center justify-between gap-2 rounded-md border border-app-border px-3 py-2"
              key={manager.userId}
            >
              <span className="app-text-body-sm text-app-ink">
                {manager.name ?? manager.userId}
                {manager.userId === me?.userId ? (
                  <span className="ml-2 app-text-caption text-app-ink/45">
                    {t('docsApp.managers.you')}
                  </span>
                ) : null}
              </span>
              <button
                className={BUTTON}
                disabled={busy || items.length <= 1}
                onClick={() => void remove(manager)}
                type="button"
              >
                {t('docsApp.managers.remove', { name: manager.name ?? manager.userId })}
              </button>
            </li>
          ))
        )}
      </ul>
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
        <DirectoryPicker
          currentUserId={me?.userId}
          kinds={['user']}
          label={t('docsApp.managers.add')}
          labels={directoryLabels}
          provider={people}
          value={selected}
          onChange={(value) => setSelected(value.slice(-1))}
        />
        <Button disabled={busy || selected.length === 0} variant="primary" onClick={() => void add()}>
          {t('docsApp.managers.add')}
        </Button>
      </div>
    </section>
  );
}
