import { useCallback, useEffect, useRef, useState } from 'react';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  listDocsHub,
  type DocsHubItem,
  type DocsHubResponse,
} from '../api/docs-api';
import type { DocsViewCategory } from './docs-view-model';

export interface DocsHubControllerClient {
  listDocsHub(
    api: AppApi | null | undefined,
    params: {
      view?: string;
      q?: string;
      sort_by?: string;
      sort_dir?: string;
      source_app?: string;
      source_kind?: string;
      space_id?: string;
    },
    spaceId?: string | null,
  ): Promise<DocsHubResponse>;
}

export interface DocsHubControllerOptions {
  api?: AppApi | null;
  spaceId?: string | null;
  listEnabled: boolean;
  activeCategory: DocsViewCategory;
  activeSourceApp?: string;
  activeSourceKind?: string;
  activeSpaceId?: string;
  client?: DocsHubControllerClient;
  searchDebounceMs?: number;
}

export interface DocsHubController {
  state: {
    docs: DocsHubItem[];
    loadingList: boolean;
    searchQuery: string;
    sortBy: string;
    sortDir: 'asc' | 'desc';
    total: number;
  };
  actions: {
    fetchDocs(): Promise<void>;
    handleSearchChange(value: string): void;
    setDocs(
      value: DocsHubItem[] | ((current: DocsHubItem[]) => DocsHubItem[]),
    ): void;
    setSortBy(value: string): void;
    setSortDir(
      value: 'asc' | 'desc' | ((current: 'asc' | 'desc') => 'asc' | 'desc'),
    ): void;
  };
}

export const docsHubControllerClient: DocsHubControllerClient = {
  listDocsHub,
};

export function useDocsHubController({
  api,
  spaceId,
  listEnabled,
  activeCategory,
  activeSourceApp,
  activeSourceKind,
  activeSpaceId,
  client = docsHubControllerClient,
  searchDebounceMs = 250,
}: DocsHubControllerOptions): DocsHubController {
  const [docs, setDocs] = useState<DocsHubItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loadingList, setLoadingList] = useState(true);
  const [searchQuery, setSearchQuery] = useState('');
  const [sortBy, setSortBy] = useState('updated_at');
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('desc');
  const searchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // miy: only the newest request may replace the list, so a slow answer for
  // an earlier space, view or query never paints over the current one.
  const requestSeqRef = useRef(0);

  const fetchDocs = useCallback(async () => {
    if (!api) return;
    const seq = requestSeqRef.current + 1;
    requestSeqRef.current = seq;
    setLoadingList(true);
    try {
      const response = await client.listDocsHub(
        api,
        {
          view: activeCategory,
          q: searchQuery || undefined,
          sort_by: sortBy,
          sort_dir: sortDir,
          source_app: activeSourceApp,
          source_kind: activeSourceKind,
          space_id: activeSpaceId,
        },
        spaceId,
      );
      if (seq !== requestSeqRef.current) return;
      setDocs(response.items);
      setTotal(response.total);
    } catch {
      if (seq !== requestSeqRef.current) return;
      setDocs([]);
      setTotal(0);
    } finally {
      if (seq === requestSeqRef.current) setLoadingList(false);
    }
  }, [
    activeCategory,
    activeSpaceId,
    activeSourceApp,
    activeSourceKind,
    client,
    searchQuery,
    sortBy,
    sortDir,
    api,
    spaceId,
  ]);

  useEffect(() => {
    if (!api) return;
    if (listEnabled) {
      void fetchDocs();
    }
  }, [fetchDocs, listEnabled, api]);

  useEffect(
    () => () => {
      if (searchTimerRef.current) {
        clearTimeout(searchTimerRef.current);
      }
    },
    [],
  );

  const handleSearchChange = useCallback(
    (value: string) => {
      if (searchTimerRef.current) {
        clearTimeout(searchTimerRef.current);
      }
      searchTimerRef.current = setTimeout(
        () => setSearchQuery(value),
        searchDebounceMs,
      );
    },
    [searchDebounceMs],
  );

  return {
    state: {
      docs,
      loadingList,
      searchQuery,
      sortBy,
      sortDir,
      total,
    },
    actions: {
      fetchDocs,
      handleSearchChange,
      setDocs,
      setSortBy,
      setSortDir,
    },
  };
}
