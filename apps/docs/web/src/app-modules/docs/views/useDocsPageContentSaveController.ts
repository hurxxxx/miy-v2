import { useCallback, useEffect, useRef, useState } from 'react';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import { isDocsRevisionConflict, type DocsPageItem } from '../api/docs-api';

/**
 * AI-DO useDocsPageContentSaveController (800 ms debounced PATCH of the page
 * body) for miy's single-editor saving. AI-DO saved block pages through the
 * Yjs collaboration snapshot and HTML pages through `content_text`; both are
 * gone (real-time co-editing and HTML pages are excluded). What changed:
 *
 * - every save sends `expected_revision`, the revision the editor's content
 *   is based on, and learns the new revision from the answer;
 * - saves of one page never overlap: an edit made while a save runs waits
 *   and is sent with the revision that save returned;
 * - a 409 `docs_page_revision_conflict` stops autosave for that page and
 *   keeps the local draft; the screen offers "reload" (take the server's
 *   page) or "keep mine" (save the draft over the newer revision);
 * - a failed save keeps the draft (`failed`) and is sent again with the next
 *   edit or `retry()`; nothing is dropped silently.
 */
export type DocsSaveStatus = 'idle' | 'pending' | 'saving' | 'failed' | 'conflict';

type ContentPayload = {
  content_blocks: Record<string, unknown>[];
  expected_revision: number;
};

export interface DocsPageContentSaveControllerOptions {
  api: AppApi | null | undefined;
  debounceMs?: number;
  savePage: (
    api: AppApi | null | undefined,
    pageId: string,
    payload: ContentPayload,
  ) => Promise<DocsPageItem>;
  /** `hasNewerDraft`: the person typed more while this save ran. */
  onSavedPage: (page: DocsPageItem, hasNewerDraft: boolean) => void;
}

export interface DocsSaveState {
  status: DocsSaveStatus;
  pageId: string | null;
}

export function useDocsPageContentSaveController({
  api,
  debounceMs = 800,
  savePage,
  onSavedPage,
}: DocsPageContentSaveControllerOptions) {
  const configRef = useRef({ api, savePage, onSavedPage });
  configRef.current = { api, savePage, onSavedPage };
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // One draft per page; saves run one at a time, oldest page first.
  const draftsRef = useRef(new Map<string, Record<string, unknown>[]>());
  const revisionsRef = useRef(new Map<string, number>());
  const inFlightRef = useRef(false);
  const blockedRef = useRef(new Set<string>());
  const [state, setState] = useState<DocsSaveState>({
    status: 'idle',
    pageId: null,
  });

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  /** The revision the editor of `pageId` is based on (set when a page loads). */
  const setBaseRevision = useCallback((pageId: string, revision: number) => {
    const known = revisionsRef.current.get(pageId);
    if (known === undefined || revision > known) {
      revisionsRef.current.set(pageId, revision);
    }
  }, []);

  const flush = useCallback(async (): Promise<void> => {
    clearTimer();
    if (inFlightRef.current) return;
    const config = configRef.current;
    if (!config.api) return;
    const next = [...draftsRef.current.entries()].find(
      ([pageId]) =>
        !blockedRef.current.has(pageId) && revisionsRef.current.has(pageId),
    );
    if (!next) return;
    const [pageId, blocks] = next;
    const revision = revisionsRef.current.get(pageId) as number;
    inFlightRef.current = true;
    setState({ status: 'saving', pageId });
    try {
      const updated = await config.savePage(config.api, pageId, {
        content_blocks: blocks,
        expected_revision: revision,
      });
      revisionsRef.current.set(updated.id, updated.revision);
      const newer = draftsRef.current.get(pageId) !== blocks;
      if (!newer) draftsRef.current.delete(pageId);
      config.onSavedPage(updated, newer);
      inFlightRef.current = false;
      if (draftsRef.current.size > 0) {
        // Edits made during this save, or other pages' drafts, go next.
        setState({ status: 'pending', pageId });
        await flush();
        return;
      }
      setState({ status: 'idle', pageId: updated.id });
    } catch (error) {
      inFlightRef.current = false;
      if (isDocsRevisionConflict(error)) {
        blockedRef.current.add(pageId);
        setState({ status: 'conflict', pageId });
        return;
      }
      setState({ status: 'failed', pageId });
    }
  }, [clearTimer]);

  const queueBlockSave = useCallback(
    (pageId: string, blocks: Record<string, unknown>[]) => {
      draftsRef.current.set(pageId, blocks);
      // An open conflict keeps the draft and sends nothing for that page.
      if (blockedRef.current.has(pageId)) return;
      if (!inFlightRef.current) setState({ status: 'pending', pageId });
      clearTimer();
      timerRef.current = setTimeout(() => {
        void flush();
      }, debounceMs);
    },
    [clearTimer, debounceMs, flush],
  );

  /** Drop the local draft (after a reload or a full replacement). */
  const cancelQueuedSave = useCallback(
    (pageId?: string) => {
      clearTimer();
      if (pageId) {
        draftsRef.current.delete(pageId);
        blockedRef.current.delete(pageId);
      } else {
        draftsRef.current.clear();
        blockedRef.current.clear();
      }
      setState((current) =>
        !pageId || current.pageId === pageId
          ? { status: draftsRef.current.size ? 'pending' : 'idle', pageId: null }
          : current,
      );
      if (draftsRef.current.size) void flush();
    },
    [clearTimer, flush],
  );

  /** "Keep mine": save the local draft over the server's newer revision. */
  const resolveConflictKeepMine = useCallback(
    async (pageId: string, latestRevision: number) => {
      if (!blockedRef.current.has(pageId)) return;
      revisionsRef.current.set(pageId, latestRevision);
      blockedRef.current.delete(pageId);
      await flush();
    },
    [flush],
  );

  /** "Reload": forget the draft; the screen shows the server's page. */
  const resolveConflictReload = useCallback(
    (page: DocsPageItem) => {
      revisionsRef.current.set(page.id, page.revision);
      cancelQueuedSave(page.id);
    },
    [cancelQueuedSave],
  );

  /** Whether `pageId` holds an unsaved local draft. */
  const hasDraft = useCallback(
    (pageId: string) => draftsRef.current.has(pageId),
    [],
  );

  /** The revision the editor of `pageId` is based on now. */
  const baseRevision = useCallback(
    (pageId: string) => revisionsRef.current.get(pageId),
    [],
  );

  /** No draft of `pageId` waits and no save runs (safe to replace the page). */
  const isSettled = useCallback(
    (pageId: string) => !draftsRef.current.has(pageId) && !inFlightRef.current,
    [],
  );

  const retry = useCallback(() => flush(), [flush]);

  // Leaving the editor sends the last draft right away (AI-DO flushed text
  // saves on unmount); the navigation guard covers leaving the app.
  useEffect(
    () => () => {
      clearTimer();
      if (draftsRef.current.size) void flush();
    },
    [clearTimer, flush],
  );

  return {
    baseRevision,
    cancelQueuedSave,
    flush,
    hasDraft,
    isSettled,
    queueBlockSave,
    resolveConflictKeepMine,
    resolveConflictReload,
    retry,
    setBaseRevision,
    saveState: state,
    /** Unsaved or in-flight work exists (reported to the portal). */
    dirty: state.status !== 'idle',
  };
}
