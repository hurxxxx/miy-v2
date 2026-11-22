import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Button,
  ContentState,
  DirectoryPicker,
  useFeedback,
  type DirectoryOption,
} from '@miy/ui';

import { cn } from '../../../lib/utils';
import { apiErrorMessage } from '../../../platform/api/api-error';
import {
  addBoardManager,
  directoryProvider,
  listBoardManagers,
  removeBoardManager,
  type BoardManager,
} from '../../../platform/board/board-api';
import { useBoard } from '../../../platform/board/board-context';
import { useRuntime } from '../../../platform/runtime/runtime-context';

import {
  addFilterTerm,
  listFilterTerms,
  removeFilterTerm,
  type FilterTerm,
} from '../api/board-admin-api';
import { CommunitySection } from './admin-community-section';

type AdminTab = 'channels' | 'filterTerms' | 'managers';

const BUTTON =
  'app-text-body-sm rounded-md border border-app-border px-3 py-1.5 text-app-ink hover:bg-app-surface-hover disabled:opacity-50';

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

/**
 * Board settings for board managers (miy). AI-DO kept these in its platform
 * admin console: the community channel manager (admin-community-section) and
 * the DM safety filter terms (admin-dm-safety-section). Board managers are a
 * miy addition (they replace AI-DO's platform administrator for this app).
 */
export function BoardAdminView() {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const { me, loading } = useBoard();
  const [tab, setTab] = useState<AdminTab>('channels');

  if (loading && !me) {
    return <ContentState kind="loading" size="fill" title={t('board.loading')} />;
  }
  if (!me?.isManager) {
    return (
      <ContentState
        className="p-5"
        kind="empty"
        size="fill"
        title={t('board.managerRequired')}
      />
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-app-bg text-app-ink">
      <header className="border-b border-app-border bg-app-bg px-4 py-4">
        <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center justify-between gap-3">
          <h1 className="app-text-title text-app-ink">{t('board.admin')}</h1>
          <div className="flex gap-1" role="tablist">
            {(['channels', 'filterTerms', 'managers'] as const).map((id) => (
              <button
                aria-selected={tab === id}
                className={cn(
                  'app-text-body-sm rounded-md px-3 py-1.5 transition-colors',
                  tab === id
                    ? 'bg-app-accent/10 font-semibold text-app-ink'
                    : 'text-app-ink/65 hover:bg-app-surface-hover',
                )}
                key={id}
                onClick={() => setTab(id)}
                role="tab"
                type="button"
              >
                {t(`board.adminTabs.${id}`)}
              </button>
            ))}
          </div>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-6xl px-4 py-4">
          {tab === 'channels' ? <CommunitySection api={api} /> : null}
          {tab === 'filterTerms' ? <FilterTerms /> : null}
          {tab === 'managers' ? <Managers /> : null}
        </div>
      </div>
    </div>
  );
}

/** AI-DO admin-dm-safety-section.tsx `DmSafetyFilterTerms`, for the board. */
function FilterTerms() {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const feedback = useFeedback();
  const [items, setItems] = useState<FilterTerm[]>([]);
  const [limit, setLimit] = useState(0);
  const [term, setTerm] = useState('');
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const response = await listFilterTerms(api);
      setItems(response.items);
      setLimit(response.limit);
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.loadFailed')));
    }
  }, [feedback, t, api]);

  useEffect(() => {
    void load();
  }, [load]);

  const add = async () => {
    if (!term.trim() || busy) return;
    setBusy(true);
    try {
      await addFilterTerm(api, term.trim());
      setTerm('');
      await load();
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed')));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    try {
      await removeFilterTerm(api, id);
      await load();
    } catch (caught) {
      feedback.error(errorText(caught, t('admin.console.dmSafety.actionFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="grid gap-2 rounded-md border border-app-border p-4">
      <div className="app-text-title-sm text-app-ink">{t('admin.console.dmSafety.filterTitle')}</div>
      <p className="app-text-caption text-app-ink/60">{t('board.filterScope')}</p>
      <div className="flex flex-wrap gap-2">
        <input
          aria-label={t('admin.console.dmSafety.filterInput')}
          className="app-text-body-sm min-w-48 flex-1 rounded-md border border-app-border bg-app-bg px-3 py-1.5"
          maxLength={50}
          onChange={(event) => setTerm(event.target.value)}
          placeholder={t('admin.console.dmSafety.filterInput')}
          value={term}
        />
        <button className={BUTTON} disabled={busy || !term.trim()} onClick={() => void add()} type="button">
          {t('admin.console.dmSafety.filterAdd')}
        </button>
      </div>
      <div className="app-text-caption text-app-ink/50">
        {t('admin.console.dmSafety.filterCount', { count: items.length, limit })}
      </div>
      <ul className="flex flex-wrap gap-1.5">
        {items.map((item) => (
          <li key={item.id}>
            <button
              aria-label={t('admin.console.dmSafety.filterRemove', { term: item.term })}
              className="app-text-caption rounded-full border border-app-border px-2 py-1 text-app-ink hover:bg-app-surface-hover"
              disabled={busy}
              onClick={() => void remove(item.id)}
              type="button"
            >
              {item.term} ×
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Board managers (miy). */
function Managers() {
  const { t } = useTranslation('apps');
  const runtime = useRuntime();
  const { api } = runtime;
  const { me, reload } = useBoard();
  const feedback = useFeedback();
  const [items, setItems] = useState<BoardManager[]>([]);
  const [selected, setSelected] = useState<DirectoryOption[]>([]);
  const [busy, setBusy] = useState(false);
  const people = useMemo(
    () => directoryProvider(runtime.directory),
    [runtime.directory],
  );
  const directoryLabels = useMemo(
    () => ({
      users: t('board.directory.users'),
      groups: t('board.directory.groups'),
      searchPlaceholder: t('board.directory.searchPlaceholder'),
      loading: t('board.directory.loading'),
      noMatches: t('board.directory.noMatches'),
      unavailable: t('board.directory.unavailable'),
      retry: t('board.directory.retry'),
      loadMore: t('board.directory.loadMore'),
      currentUser: t('board.directory.currentUser'),
      locked: t('board.directory.locked'),
    }),
    [t],
  );

  const load = useCallback(async () => {
    try {
      setItems(await listBoardManagers(api));
    } catch (caught) {
      feedback.error(apiErrorMessage(caught, t('board.loadFailed')));
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
      await addBoardManager(api, person.id);
      setSelected([]);
      feedback.success(t('board.managers.added', { name: person.name }));
      await load();
    } catch (caught) {
      feedback.error(apiErrorMessage(caught, t('board.managers.addFailed')));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (manager: BoardManager) => {
    setBusy(true);
    try {
      await removeBoardManager(api, manager.userId);
      feedback.success(
        t('board.managers.removed', { name: manager.name ?? manager.userId }),
      );
      await load();
      if (manager.userId === me?.userId) reload();
    } catch (caught) {
      feedback.error(apiErrorMessage(caught, t('board.managers.removeFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="grid gap-3 rounded-md border border-app-border p-4">
      <div className="app-text-title-sm text-app-ink">{t('board.adminTabs.managers')}</div>
      <p className="app-text-caption text-app-ink/60">{t('board.managers.description')}</p>
      <ul className="grid gap-1">
        {items.length === 0 ? (
          <li className="app-text-body-sm text-app-ink/45">{t('board.managers.empty')}</li>
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
                    {t('board.managers.you')}
                  </span>
                ) : null}
              </span>
              <button
                className={BUTTON}
                disabled={busy || items.length <= 1}
                onClick={() => void remove(manager)}
                type="button"
              >
                {t('board.managers.remove', { name: manager.name ?? manager.userId })}
              </button>
            </li>
          ))
        )}
      </ul>
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
        <DirectoryPicker
          currentUserId={me?.userId}
          kinds={['user']}
          label={t('board.managers.add')}
          labels={directoryLabels}
          provider={people}
          value={selected}
          onChange={(value) => setSelected(value.slice(-1))}
        />
        <Button disabled={busy || selected.length === 0} variant="primary" onClick={() => void add()}>
          {t('board.managers.add')}
        </Button>
      </div>
    </section>
  );
}
