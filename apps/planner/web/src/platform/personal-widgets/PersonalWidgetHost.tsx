// Port of AI-DO apps/web/src/platform/personal-widgets/PersonalWidgetHost.tsx
// (0002af062). The markup, classes and behaviour of the to-do and memo panels
// are unchanged. What miy changed:
// - The state and actions moved into `usePersonalWidgets` so the same panel
//   also renders as its own narrow route (`/widgets/todos`, `/widgets/memos`,
//   see PersonalWidgetPanelRoute) for a later secondary frame (M5).
// - The dock is shown next to the planner screen (AI-DO showed it in the
//   portal shell). Other apps' dock panels (DM, PMS, Today Planner) and the
//   "convert to PMS" action are not part of this release.
// - `runtime.api` replaces the auth token, and unsaved/in-flight work is
//   reported with runtime.setNavigationState (useNavigationDirty).
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import { LazyMotion, domAnimation, m } from 'motion/react';
import {
  Check,
  CheckCircle2,
  Circle,
  FileText,
  ListTodo,
  Loader2,
  Maximize2,
  Minimize2,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from 'lucide-react';
import { Button, Input } from '@miy/ui';

import { cn } from '../../lib/utils';
import { ApiRequestError } from '../api/client';
import { useAppApi } from '../runtime/runtime-context';
import { useNavigationDirty } from '../runtime/navigation-state';

import {
  createPersonalTodo,
  deletePersonalTodo,
  getPersonalMemo,
  listPersonalTodos,
  savePersonalMemo,
  updatePersonalTodo,
  type PersonalMemo,
  type PersonalTodoItem,
} from './personal-widgets-api';
import {
  DEFAULT_PERSONAL_WIDGET_PREFERENCES,
  PERSONAL_WIDGET_STORAGE_KEY,
  countOpenTodos,
  parsePersonalWidgetPreferences,
  serializePersonalWidgetPreferences,
  sortPersonalTodos,
  type PersonalWidgetId,
  type PersonalWidgetMode,
  type PersonalWidgetPreferences,
} from './personal-widget-state';
type TodoMutation = 'create' | 'delete' | 'toggle';
type MemoEditor = {
  saved: PersonalMemo | null;
  body: string;
  conflict: { latest: PersonalMemo | null } | null;
};
type PersonalWidgetDockItem = {
  ariaLabel: string;
  badgeClassName?: string;
  badgeCount?: number;
  dirty?: boolean;
  id: PersonalWidgetId;
  label: string;
  renderIcon: (size: number) => ReactNode;
};

function readStoredPreferences(): PersonalWidgetPreferences {
  if (typeof window === 'undefined') {
    return DEFAULT_PERSONAL_WIDGET_PREFERENCES;
  }
  // miy: the frame's storage can be unavailable; fall back like a first visit.
  try {
    return parsePersonalWidgetPreferences(
      window.localStorage.getItem(PERSONAL_WIDGET_STORAGE_KEY),
    );
  } catch {
    return DEFAULT_PERSONAL_WIDGET_PREFERENCES;
  }
}

function writeStoredPreferences(preferences: PersonalWidgetPreferences): void {
  if (typeof window === 'undefined') {
    return;
  }
  try {
    window.localStorage.setItem(
      PERSONAL_WIDGET_STORAGE_KEY,
      serializePersonalWidgetPreferences(preferences),
    );
  } catch {
    // The preference still applies in memory.
  }
}

/** To-do list and memo state shared by the dock panel and the panel routes. */
export function usePersonalWidgets() {
  const api = useAppApi();
  const memoApi = useRef(api);
  const hasMemoApi = api != null;
  const { t } = useTranslation('shell');
  const [todos, setTodos] = useState<PersonalTodoItem[]>([]);
  const [{ saved: memo, body: memoBody, conflict: memoConflict }, setMemoEditor] =
    useState<MemoEditor>({ saved: null, body: '', conflict: null });
  const memoRead = useRef(0);
  const memoWrite = useRef(false);
  const [draftTitle, setDraftTitle] = useState('');
  const [editingTodoId, setEditingTodoId] = useState<string | null>(null);
  const [editingTodoTitle, setEditingTodoTitle] = useState('');
  const pendingTodoEditFocusIdRef = useRef<string | null>(null);
  const todoEditButtonRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
  const [loading, setLoading] = useState(false);
  const [memoLoading, setMemoLoading] = useState(false);
  const [memoSaving, setMemoSaving] = useState(false);
  const [memoError, setMemoError] = useState<'memoLoadFailed' | 'memoSaveFailed' | null>(null);
  const [lastFailedMemoBody, setLastFailedMemoBody] = useState<string | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [mutating, setMutating] = useState<TodoMutation | null>(null);
  const [mutatingTodoIds, setMutatingTodoIds] = useState<Set<string>>(
    () => new Set(),
  );
  const openTodoCount = useMemo(() => countOpenTodos(todos), [todos]);
  const memoDirty = memoBody !== (memo?.body ?? '');
  const memoAutoSaveStatus = memoSaving
    ? t('personalWidgets.memo.saving')
    : memoDirty
      ? t('personalWidgets.memo.unsaved')
      : t('personalWidgets.memo.saved');

  const editingTodo = todos.find((todo) => todo.id === editingTodoId);
  useNavigationDirty(
    memoDirty ||
      memoSaving ||
      mutating !== null ||
      mutatingTodoIds.size > 0 ||
      draftTitle.trim().length > 0 ||
      (editingTodo !== undefined &&
        editingTodoTitle.trim() !== editingTodo.title),
  );

  useEffect(() => {
    if (editingTodoId !== null || !pendingTodoEditFocusIdRef.current) {
      return;
    }
    const todoId = pendingTodoEditFocusIdRef.current;
    pendingTodoEditFocusIdRef.current = null;
    todoEditButtonRefs.current.get(todoId)?.focus();
  }, [editingTodoId]);

  useEffect(() => {
    memoApi.current = api;
  }, [api]);

  const reloadTodos = useCallback(async () => {
    if (!api) {
      setTodos([]);
      setError(null);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const response = await listPersonalTodos(api);
      setTodos(sortPersonalTodos(response.items));
    } catch {
      setError(t('personalWidgets.errors.loadFailed'));
    } finally {
      setLoading(false);
    }
  }, [t, api]);

  const reloadMemo = useCallback(async () => {
    const api = memoApi.current;
    if (!api) return;
    const read = ++memoRead.current;
    setMemoLoading(true);
    setMemoError(null);
    try {
      const latest = await getPersonalMemo(api);
      if (read !== memoRead.current) return;
      setMemoEditor((current) => {
        const clean = current.body === (current.saved?.body ?? '');
        if (clean || current.body === latest.body) {
          return { saved: latest, body: latest.body, conflict: null };
        }
        // Refresh can discover a newer revision, but only an explicit choice
        // may replace a dirty draft or apply it over that revision.
        if (current.conflict || current.saved?.updatedAt !== latest.updatedAt) {
          return { ...current, conflict: { latest } };
        }
        return current;
      });
    } catch {
      if (read === memoRead.current) setMemoError('memoLoadFailed');
    } finally {
      if (read === memoRead.current) setMemoLoading(false);
    }
  }, []);

  useEffect(() => {
    void reloadTodos();
  }, [reloadTodos]);
  useEffect(() => {
    if (hasMemoApi) void reloadMemo();
  }, [hasMemoApi, reloadMemo]);

  const addTodo = useCallback(async () => {
    const title = draftTitle.trim();
    if (!api || !title) {
      return;
    }
    setMutating('create');
    setError(null);
    try {
      const created = await createPersonalTodo(api, title);
      setTodos((current) => sortPersonalTodos([...current, created]));
      setDraftTitle('');
    } catch {
      setError(t('personalWidgets.errors.createFailed'));
    } finally {
      setMutating(null);
    }
  }, [draftTitle, t, api]);

  const toggleTodo = useCallback(
    async (todo: PersonalTodoItem) => {
      if (!api) {
        return;
      }
      setMutating('toggle');
      setMutatingTodoIds((current) => new Set(current).add(todo.id));
      setError(null);
      try {
        const updated = await updatePersonalTodo(api, todo.id, {
          completed: !todo.completed,
        });
        setTodos((current) =>
          sortPersonalTodos(
            current.map((item) => (item.id === updated.id ? updated : item)),
          ),
        );
      } catch {
        setError(t('personalWidgets.errors.updateFailed'));
      } finally {
        setMutating(null);
        setMutatingTodoIds((current) => {
          const next = new Set(current);
          next.delete(todo.id);
          return next;
        });
      }
    },
    [t, api],
  );

  const startEditingTodo = useCallback((todo: PersonalTodoItem) => {
    setEditingTodoId(todo.id);
    setEditingTodoTitle(todo.title);
    setError(null);
  }, []);

  const cancelEditingTodo = useCallback(() => {
    setEditingTodoId((current) => {
      pendingTodoEditFocusIdRef.current = current;
      return null;
    });
    setEditingTodoTitle('');
  }, []);

  const saveTodoTitle = useCallback(
    async (todo: PersonalTodoItem) => {
      const title = editingTodoTitle.trim();
      if (!api || !title) {
        return;
      }
      if (title === todo.title) {
        cancelEditingTodo();
        return;
      }

      setMutatingTodoIds((current) => new Set(current).add(todo.id));
      setError(null);
      try {
        const updated = await updatePersonalTodo(api, todo.id, { title });
        setTodos((current) =>
          sortPersonalTodos(
            current.map((item) => (item.id === updated.id ? updated : item)),
          ),
        );
        cancelEditingTodo();
      } catch {
        setError(t('personalWidgets.errors.updateFailed'));
      } finally {
        setMutatingTodoIds((current) => {
          const next = new Set(current);
          next.delete(todo.id);
          return next;
        });
      }
    },
    [cancelEditingTodo, editingTodoTitle, t, api],
  );

  const removeTodo = useCallback(
    async (todo: PersonalTodoItem) => {
      if (!api) {
        return;
      }
      setMutating('delete');
      setMutatingTodoIds((current) => new Set(current).add(todo.id));
      setError(null);
      try {
        await deletePersonalTodo(api, todo.id);
        setTodos((current) => current.filter((item) => item.id !== todo.id));
      } catch {
        setError(t('personalWidgets.errors.deleteFailed'));
      } finally {
        setMutating(null);
        setMutatingTodoIds((current) => {
          const next = new Set(current);
          next.delete(todo.id);
          return next;
        });
      }
    },
    [t, api],
  );

  const saveMemo = useCallback(
    async (body: string, revision: string | null) => {
      const api = memoApi.current;
      if (!api || memoWrite.current) return;
      memoWrite.current = true;
      setMemoSaving(true);
      setMemoError(null);
      try {
        const saved = await savePersonalMemo(api, body, revision);
        setMemoEditor((current) => ({
          saved,
          body: current.body === body ? saved.body : current.body,
          conflict: null,
        }));
        setLastFailedMemoBody(null);
      } catch (error) {
        setLastFailedMemoBody(body);
        if (
          error instanceof ApiRequestError && error.status === 409 &&
          typeof error.payload === 'object' && error.payload !== null &&
          'code' in error.payload && error.payload.code === 'personal_widgets_memo_conflict'
        ) {
          setMemoEditor((current) => ({ ...current, conflict: { latest: null } }));
          await reloadMemo();
        } else {
          setMemoError('memoSaveFailed');
        }
      } finally {
        memoWrite.current = false;
        setMemoSaving(false);
      }
    },
    [reloadMemo],
  );

  useEffect(() => {
    if (
      !hasMemoApi ||
      !memo ||
      memoConflict ||
      memoLoading ||
      memoSaving ||
      !memoDirty ||
      lastFailedMemoBody === memoBody
    ) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      void saveMemo(memoBody, memo.updatedAt);
    }, 800);
    return () => window.clearTimeout(timeoutId);
  }, [
    lastFailedMemoBody,
    memo,
    memoBody,
    memoConflict,
    memoDirty,
    memoLoading,
    memoSaving,
    saveMemo,
    hasMemoApi,
  ]);

  const updateMemoBody = useCallback((body: string) => {
    setMemoEditor((current) => ({ ...current, body }));
    setLastFailedMemoBody(null);
    setMemoError(null);
  }, []);

  const acceptSavedMemo = (latest: PersonalMemo) => {
    setMemoEditor({ saved: latest, body: latest.body, conflict: null });
    setLastFailedMemoBody(null);
    setMemoError(null);
  };

  return {
    acceptSavedMemo,
    addTodo,
    cancelEditingTodo,
    draftTitle,
    editingTodoId,
    editingTodoTitle,
    error,
    loading,
    memoAutoSaveStatus,
    memoReady: memo !== null,
    memoBody,
    memoConflict,
    memoDirty,
    memoError: memoError ? t(`personalWidgets.errors.${memoError}`) : null,
    memoLoading,
    memoSaving,
    mutating,
    mutatingTodoIds,
    openTodoCount,
    reloadMemo,
    reloadTodos,
    removeTodo,
    replaceMemo: (latest: PersonalMemo) => saveMemo(memoBody, latest.updatedAt),
    retryMemo: memo && memoDirty && !memoConflict && lastFailedMemoBody === memoBody
      ? () => saveMemo(memoBody, memo.updatedAt)
      : null,
    saveTodoTitle,
    setDraftTitle,
    setEditingTodoTitle,
    startEditingTodo,
    todoEditButtonRefs,
    todos,
    toggleTodo,
    updateMemoBody,
  };
}

export type PersonalWidgetsState = ReturnType<typeof usePersonalWidgets>;

/** The panel body (AI-DO's `activePanelSection === 'personal'` section). */
export function PersonalWidgetSection({
  activeWidget,
  state,
}: {
  activeWidget: PersonalWidgetId;
  state: PersonalWidgetsState;
}) {
  const { t } = useTranslation('shell');
  const {
    acceptSavedMemo,
    addTodo,
    cancelEditingTodo,
    draftTitle,
    editingTodoId,
    editingTodoTitle,
    error,
    loading,
    memoAutoSaveStatus,
    memoBody,
    memoConflict,
    memoDirty,
    memoError,
    memoLoading,
    memoReady,
    memoSaving,
    mutating,
    mutatingTodoIds,
    openTodoCount,
    removeTodo,
    replaceMemo,
    retryMemo,
    saveTodoTitle,
    setDraftTitle,
    setEditingTodoTitle,
    startEditingTodo,
    todoEditButtonRefs,
    todos,
    toggleTodo,
    updateMemoBody,
  } = state;
  const widgetError = activeWidget === 'memo' ? memoError : error;
  const latestMemo = memoConflict?.latest;
  return (
    <section className="flex min-h-0 min-w-0 w-full flex-1 flex-col overflow-hidden">
      <div className="grid shrink-0 gap-3 border-b border-app-border px-4 py-3">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1">
            <p className="app-text-caption text-app-ink/55">
              {activeWidget === 'memo'
                ? t('personalWidgets.memo.summary')
                : t('personalWidgets.todo.summary', {
                    completed: todos.length - openTodoCount,
                    open: openTodoCount,
                  })}
            </p>
          </div>
          {activeWidget === 'todo' ? (
            <span className="app-text-micro shrink-0 rounded-full bg-app-surface px-2 py-1 text-app-ink/55">
              {openTodoCount}
            </span>
          ) : memoDirty ? (
            <span className="app-text-micro shrink-0 rounded-full bg-app-surface px-2 py-1 text-app-ink/55">
              {memoAutoSaveStatus}
            </span>
          ) : null}
        </div>
      </div>

      {activeWidget === 'todo' ? (
        <form
          className="flex shrink-0 gap-2 border-b border-app-border px-4 py-3"
          onSubmit={(event) => {
            event.preventDefault();
            void addTodo();
          }}
        >
          <Input
            aria-label={t('personalWidgets.todo.inputLabel')}
            disabled={mutating === 'create'}
            maxLength={240}
            placeholder={t('personalWidgets.todo.inputPlaceholder')}
            value={draftTitle}
            onChange={(event) => setDraftTitle(event.target.value)}
          />
          <Button
            aria-label={t('personalWidgets.todo.add')}
            disabled={mutating === 'create' || draftTitle.trim().length === 0}
            size="icon"
            type="submit"
            variant="primary"
          >
            {mutating === 'create' ? (
              <Loader2 aria-hidden="true" className="animate-spin" size={15} />
            ) : (
              <Plus aria-hidden="true" size={16} />
            )}
          </Button>
        </form>
      ) : null}

      {widgetError ? (
        <div
          className="mx-4 mt-3 rounded-md border border-[var(--ui-color-danger)]/35 bg-[var(--ui-color-danger)]/8 px-3 py-2"
          role="alert"
        >
          <p className="app-text-caption text-[var(--ui-color-danger)]">
            {widgetError}
          </p>
        </div>
      ) : null}

      <div className="custom-scrollbar min-h-0 flex-1 overflow-y-auto">
        {activeWidget === 'memo' ? (
          <div className="flex h-full flex-col gap-3 p-4">
            {memoLoading && !memoReady ? (
              <div className="flex h-full items-center justify-center text-app-ink/40">
                <Loader2
                  aria-hidden="true"
                  className="animate-spin"
                  size={20}
                />
              </div>
            ) : (
              <>
                <textarea
                  aria-label={t('personalWidgets.memo.inputLabel')}
                  className="custom-scrollbar min-h-[260px] flex-1 resize-none rounded-md border border-app-border bg-app-surface px-3 py-2 text-[0.86rem] leading-6 text-app-ink outline-none transition-colors placeholder:text-app-ink/35 focus:border-app-accent focus:ring-2 focus:ring-app-accent/20"
                  maxLength={20000}
                  placeholder={t('personalWidgets.memo.inputPlaceholder')}
                  readOnly={!memoReady}
                  value={memoBody}
                  onChange={(event) => updateMemoBody(event.target.value)}
                />
                {memoConflict ? (
                  <div className="grid shrink-0 gap-2 rounded-md border border-app-border p-3">
                    <p role="alert" className="app-text-caption">
                      {t('personalWidgets.memo.conflict')}
                    </p>
                    {latestMemo ? (
                      <>
                        <p className="app-text-caption">
                          {t('personalWidgets.memo.savedCopy')}
                        </p>
                        <pre className="max-h-32 overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-app-surface px-2 py-1">
                          {latestMemo.body || t('common:empty.none')}
                        </pre>
                        <div className="flex flex-wrap gap-2">
                          <Button size="dense" variant="secondary" disabled={memoLoading || memoSaving}
                            onClick={() => acceptSavedMemo(latestMemo)}>
                            {t('personalWidgets.memo.useSavedCopy')}
                          </Button>
                          <Button size="dense" disabled={memoLoading || memoSaving}
                            onClick={() => void replaceMemo(latestMemo)}>
                            {t('personalWidgets.memo.replaceSavedCopy')}
                          </Button>
                        </div>
                      </>
                    ) : null}
                  </div>
                ) : retryMemo ? (
                  <Button size="dense" variant="secondary" className="self-start"
                    disabled={memoLoading || memoSaving} onClick={() => void retryMemo()}>
                    {t('personalWidgets.memo.retrySave')}
                  </Button>
                ) : null}
                <div className="flex shrink-0 items-center justify-between gap-3">
                  <span className="app-text-caption text-app-ink/45">
                    {t('personalWidgets.memo.characterCount', {
                      count: memoBody.length,
                    })}
                  </span>
                  <span
                    className={cn(
                      'app-text-caption inline-flex items-center gap-1.5 text-app-ink/55',
                      memoSaving && 'text-app-accent',
                      memoDirty &&
                        !memoSaving &&
                        'text-[var(--ui-color-warning)]',
                    )}
                  >
                    {memoSaving ? (
                      <Loader2
                        aria-hidden="true"
                        className="animate-spin"
                        size={15}
                      />
                    ) : memoDirty ? (
                      <Circle aria-hidden="true" size={14} />
                    ) : (
                      <CheckCircle2 aria-hidden="true" size={14} />
                    )}
                    <span>{memoAutoSaveStatus}</span>
                  </span>
                </div>
              </>
            )}
          </div>
        ) : loading && todos.length === 0 ? (
          <div className="flex h-full items-center justify-center text-app-ink/40">
            <Loader2 aria-hidden="true" className="animate-spin" size={20} />
          </div>
        ) : todos.length === 0 ? (
          <div className="flex h-full items-center justify-center px-8 text-center">
            <p className="app-text-body-sm text-app-ink/45">
              {t('personalWidgets.todo.empty')}
            </p>
          </div>
        ) : (
          <ul className="divide-y divide-app-border">
            {todos.map((todo) => {
              const busy = mutatingTodoIds.has(todo.id);
              const editing = editingTodoId === todo.id;
              return (
                <li key={todo.id} className="flex items-start gap-2 px-4 py-3">
                  <button
                    type="button"
                    aria-label={t('personalWidgets.todo.toggle', {
                      title: todo.title,
                    })}
                    className="mt-0.5 shrink-0 rounded-md p-1 text-app-ink/45 transition-colors hover:bg-app-surface-hover hover:text-app-accent disabled:opacity-50"
                    disabled={busy || editing}
                    onClick={() => void toggleTodo(todo)}
                  >
                    {busy && mutating === 'toggle' ? (
                      <Loader2
                        aria-hidden="true"
                        className="animate-spin"
                        size={17}
                      />
                    ) : todo.completed ? (
                      <CheckCircle2
                        aria-hidden="true"
                        className="text-app-accent"
                        size={17}
                      />
                    ) : (
                      <Circle aria-hidden="true" size={17} />
                    )}
                  </button>
                  {editing ? (
                    <form
                      className="flex min-w-0 flex-1 items-center gap-1"
                      onSubmit={(event) => {
                        event.preventDefault();
                        void saveTodoTitle(todo);
                      }}
                    >
                      <Input
                        autoFocus
                        aria-label={t('personalWidgets.todo.editInputLabel', {
                          title: todo.title,
                        })}
                        disabled={busy}
                        maxLength={240}
                        value={editingTodoTitle}
                        onChange={(event) =>
                          setEditingTodoTitle(event.target.value)
                        }
                        onKeyDown={(event) => {
                          if (
                            event.key === 'Enter' &&
                            event.nativeEvent.isComposing
                          ) {
                            event.preventDefault();
                            return;
                          }
                          if (event.key === 'Escape') {
                            event.preventDefault();
                            event.stopPropagation();
                            cancelEditingTodo();
                          }
                        }}
                      />
                      <button
                        type="submit"
                        aria-label={t('personalWidgets.todo.saveEdit')}
                        className="shrink-0 rounded-md p-1 text-app-accent transition-colors hover:bg-app-surface-hover disabled:opacity-50"
                        disabled={busy || editingTodoTitle.trim().length === 0}
                      >
                        {busy ? (
                          <Loader2
                            aria-hidden="true"
                            className="animate-spin"
                            size={16}
                          />
                        ) : (
                          <Check aria-hidden="true" size={16} />
                        )}
                      </button>
                      <button
                        type="button"
                        aria-label={t('personalWidgets.todo.cancelEdit')}
                        className="shrink-0 rounded-md p-1 text-app-ink/40 transition-colors hover:bg-app-surface-hover hover:text-app-ink disabled:opacity-50"
                        disabled={busy}
                        onClick={cancelEditingTodo}
                      >
                        <X aria-hidden="true" size={16} />
                      </button>
                    </form>
                  ) : (
                    <span
                      className={cn(
                        'app-text-body-sm min-w-0 flex-1 break-words pt-1 text-app-ink',
                        todo.completed && 'text-app-ink/45 line-through',
                      )}
                    >
                      {todo.title}
                    </span>
                  )}
                  {!editing ? (
                    <button
                      ref={(node) => {
                        if (node) {
                          todoEditButtonRefs.current.set(todo.id, node);
                        } else {
                          todoEditButtonRefs.current.delete(todo.id);
                        }
                      }}
                      type="button"
                      aria-label={t('personalWidgets.todo.edit', {
                        title: todo.title,
                      })}
                      className="mt-0.5 shrink-0 rounded-md p-1 text-app-ink/35 transition-colors hover:bg-app-surface-hover hover:text-app-accent disabled:opacity-50"
                      disabled={busy || editingTodoId !== null}
                      onClick={() => startEditingTodo(todo)}
                      title={t('personalWidgets.todo.edit', {
                        title: todo.title,
                      })}
                    >
                      <Pencil aria-hidden="true" size={16} />
                    </button>
                  ) : null}
                  {!editing ? (
                    <button
                      type="button"
                      aria-label={t('personalWidgets.todo.delete', {
                        title: todo.title,
                      })}
                      className="mt-0.5 shrink-0 rounded-md p-1 text-app-ink/35 transition-colors hover:bg-app-surface-hover hover:text-[var(--ui-color-danger)] disabled:opacity-50"
                      disabled={busy}
                      onClick={() => void removeTodo(todo)}
                      title={t('personalWidgets.todo.delete', {
                        title: todo.title,
                      })}
                    >
                      {busy && mutating === 'delete' ? (
                        <Loader2
                          aria-hidden="true"
                          className="animate-spin"
                          size={16}
                        />
                      ) : (
                        <Trash2 aria-hidden="true" size={16} />
                      )}
                    </button>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}

/** Header button reloading the active widget (AI-DO `reloadPersonalWidget`). */
export function PersonalWidgetReloadButton({
  activeWidget,
  state,
}: {
  activeWidget: PersonalWidgetId;
  state: PersonalWidgetsState;
}) {
  const { t } = useTranslation('shell');
  const reloadBusy = activeWidget === 'memo'
    ? state.memoLoading || state.memoSaving
    : state.loading;
  return (
    <Button
      aria-label={t('personalWidgets.reload')}
      disabled={reloadBusy}
      onClick={() => {
        if (activeWidget === 'memo') {
          void state.reloadMemo();
        } else {
          void state.reloadTodos();
        }
      }}
      size="icon"
      title={t('personalWidgets.reload')}
      variant="ghost"
    >
      <RefreshCw
        aria-hidden="true"
        className={reloadBusy ? 'animate-spin' : undefined}
        size={15}
      />
    </Button>
  );
}

/**
 * The right-edge dock with the floating to-do/memo panel, as in AI-DO's
 * shell. `onOpenPanelRoute` opens the same widget as its own narrow route.
 */
export function PersonalWidgetHost({
  onOpenPanelRoute,
}: {
  onOpenPanelRoute?: (widget: PersonalWidgetId) => void;
} = {}) {
  const { t } = useTranslation('shell');
  const [preferences, setPreferences] = useState(readStoredPreferences);
  const state = usePersonalWidgets();
  const { memoDirty, openTodoCount } = state;
  const mode = preferences.mode;
  const activeWidget = preferences.activeWidget;
  const activeWidgetTitle =
    activeWidget === 'memo'
      ? t('personalWidgets.memo.title')
      : t('personalWidgets.todo.title');

  useEffect(() => {
    writeStoredPreferences(preferences);
  }, [preferences]);

  const setMode = useCallback((nextMode: PersonalWidgetMode) => {
    setPreferences((current) => ({ ...current, mode: nextMode }));
  }, []);

  const setActiveWidget = useCallback((nextWidget: PersonalWidgetId) => {
    setPreferences((current) => ({ ...current, activeWidget: nextWidget }));
  }, []);

  const handleDockItemClick = useCallback(
    (itemId: PersonalWidgetId) => {
      if (mode !== 'collapsed' && activeWidget === itemId) {
        setMode('collapsed');
        return;
      }
      setActiveWidget(itemId);
      setMode('panel');
    },
    [activeWidget, mode, setActiveWidget, setMode],
  );

  useEffect(() => {
    if (mode === 'collapsed') {
      return;
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setMode('collapsed');
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [mode, setMode]);

  const dockItems: PersonalWidgetDockItem[] = [
    {
      ariaLabel: t('personalWidgets.todo.open'),
      badgeClassName: 'bg-[var(--ui-color-danger)] text-white',
      badgeCount: openTodoCount,
      id: 'todo' as const,
      label: t('personalWidgets.todo.shortTitle'),
      renderIcon: (size: number) => (
        <ListTodo aria-hidden="true" size={size} strokeWidth={2.1} />
      ),
    },
    {
      ariaLabel: t('personalWidgets.memo.open'),
      dirty: memoDirty,
      id: 'memo' as const,
      label: t('personalWidgets.memo.shortTitle'),
      renderIcon: (size: number) => (
        <FileText aria-hidden="true" size={size} strokeWidth={2.1} />
      ),
    },
  ];
  const panelControls = (
    <div className="flex shrink-0 items-center gap-1">
      <PersonalWidgetReloadButton activeWidget={activeWidget} state={state} />
      {onOpenPanelRoute ? (
        <Button
          aria-label={t('personalWidgets.openPanel')}
          onClick={() => onOpenPanelRoute(activeWidget)}
          size="icon"
          title={t('personalWidgets.openPanel')}
          variant="ghost"
        >
          {activeWidget === 'memo' ? (
            <FileText aria-hidden="true" size={15} />
          ) : (
            <ListTodo aria-hidden="true" size={15} />
          )}
        </Button>
      ) : null}
      <Button
        aria-label={
          mode === 'fullscreen'
            ? t('personalWidgets.exitFullscreen')
            : t('personalWidgets.enterFullscreen')
        }
        onClick={() => setMode(mode === 'fullscreen' ? 'panel' : 'fullscreen')}
        size="icon"
        title={
          mode === 'fullscreen'
            ? t('personalWidgets.exitFullscreen')
            : t('personalWidgets.enterFullscreen')
        }
        variant="ghost"
      >
        {mode === 'fullscreen' ? (
          <Minimize2 aria-hidden="true" size={15} />
        ) : (
          <Maximize2 aria-hidden="true" size={15} />
        )}
      </Button>
      <Button
        aria-label={t('personalWidgets.close')}
        onClick={() => setMode('collapsed')}
        size="icon"
        title={t('personalWidgets.close')}
        variant="ghost"
      >
        <X aria-hidden="true" size={15} />
      </Button>
    </div>
  );
  const panelFrameClassName =
    mode === 'fullscreen'
      ? 'bottom-3 left-3 right-12 top-3 rounded-lg max-[640px]:bottom-0 max-[640px]:left-0 max-[640px]:right-10 max-[640px]:top-0 max-[640px]:rounded-none'
      : 'right-12 top-1/2 h-[min(740px,calc(100vh-2rem))] w-[min(440px,calc(100vw-4.75rem))] -translate-y-1/2 rounded-lg max-[640px]:bottom-0 max-[640px]:left-0 max-[640px]:right-10 max-[640px]:top-auto max-[640px]:h-[min(82dvh,680px)] max-[640px]:w-auto max-[640px]:translate-y-0 max-[640px]:rounded-b-none max-[640px]:rounded-t-lg';

  return (
    <LazyMotion features={domAnimation}>
      {mode !== 'collapsed' ? (
        <m.aside
          aria-label={t('personalWidgets.panelLabel')}
          initial={{ opacity: 0, x: mode === 'fullscreen' ? 0 : 32 }}
          animate={{ opacity: 1, x: 0 }}
          className={cn(
            'fixed z-[75] flex flex-col overflow-hidden border border-app-border bg-app-bg text-app-ink shadow-2xl outline-none',
            panelFrameClassName,
          )}
        >
          <header className="flex shrink-0 items-center justify-between gap-3 border-b border-app-border px-4 py-3">
            <div className="min-w-0">
              <p className="app-text-micro text-app-ink/45">
                {t('personalWidgets.eyebrow')}
              </p>
              <h2 className="app-text-title-md truncate text-app-ink">
                {activeWidgetTitle}
              </h2>
            </div>
            {panelControls}
          </header>

          <div className="flex min-h-0 min-w-0 w-full flex-1 overflow-x-hidden">
            <PersonalWidgetSection activeWidget={activeWidget} state={state} />
          </div>
        </m.aside>
      ) : null}

      <nav
        aria-label={t('personalWidgets.dockLabel')}
        className="scrollbar-none relative z-[80] flex h-full w-10 shrink-0 flex-col overflow-x-hidden overflow-y-auto border-l border-app-border bg-app-bg/95 shadow-[var(--ui-shadow-side-dock)] backdrop-blur"
      >
        {dockItems.map((item, index) => {
          const active = mode !== 'collapsed' && activeWidget === item.id;
          return (
            <button
              key={item.id}
              type="button"
              aria-label={item.ariaLabel}
              aria-pressed={active}
              className={cn(
                'relative flex min-h-[76px] w-10 flex-col items-center justify-center gap-1 border-app-border px-1 py-2 text-app-ink/60 transition-colors hover:bg-app-surface-hover hover:text-app-ink focus:outline-none focus:ring-2 focus:ring-inset focus:ring-app-accent/35',
                index > 0 && 'border-t',
                active &&
                  'bg-app-accent text-app-accent-fg hover:bg-app-accent hover:text-app-accent-fg',
              )}
              title={item.ariaLabel}
              onClick={() => handleDockItemClick(item.id)}
            >
              <span
                className="flex size-5 shrink-0 items-center justify-center"
                aria-hidden="true"
              >
                {item.renderIcon(15)}
              </span>
              <span className="app-text-micro font-semibold uppercase leading-none [text-orientation:mixed] [writing-mode:vertical-rl]">
                {item.label}
              </span>
              {item.badgeCount && item.badgeCount > 0 ? (
                <span
                  aria-hidden="true"
                  className={cn(
                    'absolute right-1 top-1.5 inline-flex h-4 min-w-4 max-w-8 items-center justify-center rounded-full px-1 text-[0.62rem] font-bold leading-none shadow-sm',
                    item.badgeClassName,
                  )}
                >
                  {item.badgeCount}
                </span>
              ) : item.dirty ? (
                <span
                  aria-hidden="true"
                  className="absolute right-1.5 top-2 size-2.5 rounded-full bg-[var(--ui-color-warning)] shadow-sm"
                />
              ) : null}
            </button>
          );
        })}
      </nav>
    </LazyMotion>
  );
}

/**
 * A widget on its own, filling a narrow frame: the panel header and body
 * without the dock, fullscreen or close controls. Routes `/widgets/todos`
 * and `/widgets/memos`.
 */
export function PersonalWidgetPanelRoute({
  widget,
}: {
  widget: PersonalWidgetId;
}) {
  const { t } = useTranslation('shell');
  const state = usePersonalWidgets();
  return (
    <aside
      aria-label={t('personalWidgets.panelLabel')}
      className="flex h-full min-h-0 flex-col overflow-hidden bg-app-bg text-app-ink"
    >
      <header className="flex shrink-0 items-center justify-between gap-3 border-b border-app-border px-4 py-3">
        <div className="min-w-0">
          <p className="app-text-micro text-app-ink/45">
            {t('personalWidgets.eyebrow')}
          </p>
          <h2 className="app-text-title-md truncate text-app-ink">
            {widget === 'memo'
              ? t('personalWidgets.memo.title')
              : t('personalWidgets.todo.title')}
          </h2>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <PersonalWidgetReloadButton activeWidget={widget} state={state} />
        </div>
      </header>
      <div className="flex min-h-0 min-w-0 w-full flex-1 overflow-x-hidden">
        <PersonalWidgetSection activeWidget={widget} state={state} />
      </div>
    </aside>
  );
}
