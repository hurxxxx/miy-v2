import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  createSavedFilter,
  deleteSavedFilter,
  listSavedFilters,
  setDefaultSavedFilter,
  updateSavedFilter,
  type PmsSavedFilter,
  type PmsSavedFilterInput,
  type PmsSavedFilterScope,
} from '../api/pms-api';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { apiErrorMessage } from '../../../platform/api/api-error';

export function savedFilterScopeKey(scope: PmsSavedFilterScope | null): string {
  if (!scope) return '';
  return scope.listId !== undefined
    ? `list:${scope.listId}`
    : `space:${scope.spaceId}`;
}

export type PmsSavedFiltersController = {
  scopeKey: string;
  items: PmsSavedFilter[];
  loading: boolean;
  /** Loading the list failed. */
  error: string | null;
  /** The last write failed (shown next to the saved filters). */
  actionError: string | null;
  clearActionError: () => void;
  create: (input: PmsSavedFilterInput) => Promise<boolean>;
  update: (
    filter: PmsSavedFilter,
    patch: Partial<PmsSavedFilterInput>,
  ) => Promise<boolean>;
  remove: (filter: PmsSavedFilter) => Promise<boolean>;
  setDefault: (filter: PmsSavedFilter | null) => Promise<boolean>;
};

type LoadedState = {
  scopeKey: string;
  items: PmsSavedFilter[];
  loading: boolean;
  error: string | null;
};

/**
 * miy reinforcement: the saved filters of one view, kept on the server (AI-DO
 * kept them in browser storage). A list answering for a view the person has
 * already left is dropped. `onOpeningDefault` runs once when a view's filters
 * first arrive with an "open with" filter; the screen decides whether the
 * person already changed the filters meanwhile.
 */
export function usePmsSavedFilters({
  scope,
  onOpeningDefault,
}: {
  scope: PmsSavedFilterScope | null;
  onOpeningDefault?: (filter: PmsSavedFilter, scopeKey: string) => void;
}): PmsSavedFiltersController {
  const api = useAppApi();
  const [actionError, setActionError] = useState<string | null>(null);
  const { t } = useTranslation('apps');
  const scopeKey = savedFilterScopeKey(scope);
  const scopeRef = useRef(scope);
  scopeRef.current = scope;
  const scopeKeyRef = useRef(scopeKey);
  scopeKeyRef.current = scopeKey;
  const generationRef = useRef(0);
  const onOpeningDefaultRef = useRef(onOpeningDefault);
  onOpeningDefaultRef.current = onOpeningDefault;
  const [state, setState] = useState<LoadedState>({
    scopeKey: '',
    items: [],
    loading: false,
    error: null,
  });

  const fetchList = useCallback(
    async (opening: boolean) => {
      const currentScope = scopeRef.current;
      const key = scopeKeyRef.current;
      if (!api || !currentScope || !key) return;
      const generation = generationRef.current + 1;
      generationRef.current = generation;
      setState((current) =>
        current.scopeKey === key
          ? { ...current, loading: true }
          : { scopeKey: key, items: [], loading: true, error: null },
      );
      try {
        const response = await listSavedFilters(api, currentScope);
        if (generationRef.current !== generation || scopeKeyRef.current !== key)
          return;
        setState({
          scopeKey: key,
          items: response.items,
          loading: false,
          error: null,
        });
        if (opening && response.default_filter_id) {
          const opened = response.items.find(
            (item) => item.id === response.default_filter_id,
          );
          if (opened) onOpeningDefaultRef.current?.(opened, key);
        }
      } catch (caught) {
        if (generationRef.current !== generation || scopeKeyRef.current !== key)
          return;
        setState({
          scopeKey: key,
          items: [],
          loading: false,
          error: apiErrorMessage(caught, t('pms.savedFilters.errors.loadFailed')),
        });
      }
    },
    [api, t],
  );

  useEffect(() => {
    setActionError(null);
    if (!scopeKey) return;
    void fetchList(true);
  }, [fetchList, scopeKey]);

  /** Run a write for the view it was asked in; refresh that view after. */
  const mutate = useCallback(
    async (
      write: (currentScope: PmsSavedFilterScope) => Promise<unknown>,
      failedKey: string,
    ): Promise<boolean> => {
      const currentScope = scopeRef.current;
      const key = scopeKeyRef.current;
      if (!api || !currentScope) return false;
      setActionError(null);
      try {
        await write(currentScope);
        if (scopeKeyRef.current === key) await fetchList(false);
        return true;
      } catch (caught) {
        if (scopeKeyRef.current === key) {
          setActionError(apiErrorMessage(caught, t(failedKey)));
        }
        return false;
      }
    },
    [api, fetchList, t],
  );

  const create = useCallback(
    (input: PmsSavedFilterInput) =>
      mutate(
        (currentScope) => createSavedFilter(api!, currentScope, input),
        'pms.savedFilters.errors.saveFailed',
      ),
    [api, mutate],
  );
  const update = useCallback(
    (filter: PmsSavedFilter, patch: Partial<PmsSavedFilterInput>) =>
      mutate(
        () => updateSavedFilter(api!, filter.id, patch),
        'pms.savedFilters.errors.saveFailed',
      ),
    [api, mutate],
  );
  const remove = useCallback(
    (filter: PmsSavedFilter) =>
      mutate(
        () => deleteSavedFilter(api!, filter.id),
        'pms.savedFilters.errors.deleteFailed',
      ),
    [api, mutate],
  );
  const setDefault = useCallback(
    (filter: PmsSavedFilter | null) =>
      mutate(
        (currentScope) =>
          setDefaultSavedFilter(api!, currentScope, filter?.id ?? null),
        'pms.savedFilters.errors.defaultFailed',
      ),
    [api, mutate],
  );

  const current = state.scopeKey === scopeKey;
  return {
    scopeKey,
    items: current ? state.items : [],
    loading: current ? state.loading : Boolean(scopeKey),
    error: current ? state.error : null,
    actionError,
    clearActionError: () => setActionError(null),
    create,
    update,
    remove,
    setDefault,
  };
}
