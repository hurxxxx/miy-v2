import { useCallback, useEffect, useRef, useState } from 'react';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import {
  listDocsHub,
  type DocsHubItem,
  type DocsHubResponse,
} from '../api/docs-api';
import type { DocsViewCategory } from './docs-view-model';

const PAGE_SIZE = 50;

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
      page?: number;
      page_size?: number;
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
    loadFailed: boolean;
    loadingList: boolean;
    page: number;
    pageCount: number;
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
    setPage(page: number): void;
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
  const [loadFailed, setLoadFailed] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [sortBy, setSortBy] = useState('updated_at');
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('desc');
  const filterKey = JSON.stringify([
    spaceId,
    activeCategory,
    activeSourceApp,
    activeSourceKind,
    activeSpaceId,
    searchQuery,
    sortBy,
    sortDir,
  ]);
  const [pagination, setPagination] = useState({ filterKey, page: 1 });
  const page = pagination.filterKey === filterKey ? pagination.page : 1;
  if (pagination.filterKey !== filterKey) setPagination({ filterKey, page: 1 });
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const setPage = (nextPage: number) => {
    setPagination({ filterKey, page: Math.max(1, Math.min(pageCount, nextPage)) });
  };
  const searchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // miy: only the newest request may replace the list, so a slow answer for
  // an earlier space, view or query never paints over the current one.
  const requestSeqRef = useRef(0);

  const fetchDocs = useCallback(async () => {
    if (!api) return;
    const seq = requestSeqRef.current + 1;
    requestSeqRef.current = seq;
    setLoadingList(true);
    setLoadFailed(false);
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
          page,
          page_size: PAGE_SIZE,
        },
        spaceId,
      );
      if (seq !== requestSeqRef.current) return;
      const lastPage = Math.max(1, Math.ceil(response.total / PAGE_SIZE));
      if (page > lastPage) {
        setPagination({ filterKey, page: lastPage });
        return;
      }
      setDocs(response.items);
      setTotal(response.total);
    } catch {
      if (seq !== requestSeqRef.current) return;
      setDocs([]);
      setTotal(0);
      setLoadFailed(true);
    } finally {
      if (seq === requestSeqRef.current) setLoadingList(false);
    }
  }, [
    activeCategory,
    activeSpaceId,
    activeSourceApp,
    activeSourceKind,
    client,
    filterKey,
    page,
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
      loadFailed,
      loadingList,
      page,
      pageCount,
      searchQuery,
      sortBy,
      sortDir,
      total,
    },
    actions: {
      fetchDocs,
      handleSearchChange,
      setDocs,
      setPage,
      setSortBy,
      setSortDir,
    },
  };
}
