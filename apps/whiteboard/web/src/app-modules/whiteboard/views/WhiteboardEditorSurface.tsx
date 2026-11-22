import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import {
  CaptureUpdateAction,
  Excalidraw,
  exportToBlob,
  exportToSvg,
  newElementWith,
  restoreLibraryItems,
  serializeAsJSON,
  THEME,
} from '@excalidraw/excalidraw';
import type {
  BinaryFileData,
  ExcalidrawImperativeAPI,
  ExcalidrawInitialDataState,
  ExcalidrawProps,
  LibraryItems,
} from '@excalidraw/excalidraw/types';
import '@excalidraw/excalidraw/index.css';
import {
  ArchiveRestore,
  Download,
  FileJson,
  Grid3X3,
  Image,
  Loader2,
  MoreHorizontal,
  Pencil,
  RefreshCcw,
  Share2,
  Star,
  Trash2,
  X,
} from 'lucide-react';
import { Button, Dialog, DropdownMenu } from '@miy/ui';
import { useTranslation } from 'react-i18next';

import { cn } from '../../../lib/utils';
import { downloadBlobAsFile } from '../../../platform/browser/browser-download';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import {
  deleteWhiteboard,
  deleteWhiteboardTarget,
  deleteWhiteboardUserShare,
  getWhiteboard,
  getWhiteboardSharing,
  listWhiteboardShareableUsers,
  normalizeWhiteboardScene,
  permanentlyDeleteWhiteboard,
  recordWhiteboardView,
  restoreWhiteboard,
  toggleWhiteboardFavorite,
  updateWhiteboard,
  updateWhiteboardTarget,
  upsertWhiteboardUserShare,
  type ShareableUserItem,
  type WhiteboardDetail,
  type WhiteboardScene,
  type WhiteboardSharingResponse,
  type WhiteboardVisibility,
} from '../api/whiteboard-api';
import {
  createWhiteboardFileStore,
  hydrateSceneFiles,
  type WhiteboardFileStore,
} from '../api/whiteboard-files';
import architectureDiagramComponentsLibrary from '../libraries/architecture-diagram-components.excalidrawlib.json';
import awesomeIconsLibrary from '../libraries/awesome-icons.excalidrawlib.json';
import drwnioLibrary from '../libraries/drwnio.excalidrawlib.json';
import softwareArchitectureLibrary from '../libraries/software-architecture.excalidrawlib.json';
import systemDesignLibrary from '../libraries/system-design.excalidrawlib.json';
import {
  elementId,
  selectedElementCount,
  type WhiteboardElement,
} from './whiteboard-scene';
import { safeFilename, saveLabel } from './whiteboard-editor-utils';
import { buildWhiteboardSharePresenter } from './whiteboard-share-model';
import { useWhiteboardScenePersistence } from './useWhiteboardScenePersistence';
import { buildWhiteboardVisibilityMenuItems } from './WhiteboardViewParts';

/**
 * AI-DO views/WhiteboardEditorSurface.tsx.
 *
 * Not ported: the Yjs collaboration (live status, collaborators, pointers,
 * remote scene merging), link shares and the shared-link editor, and the
 * detach action of other apps' context slots. "편집 시작" now turns on
 * single-editor editing on the latest saved scene; saves carry the revision
 * and a conflict shows reload / keep mine (useWhiteboardScenePersistence).
 * Images are stored apart from the scene (whiteboard-files.ts). Web embeds
 * are disabled: the frame CSP blocks every iframe (`frame-src 'none'`).
 */
type ExcalidrawOnChange = NonNullable<ExcalidrawProps['onChange']>;
type ExcalidrawLibraryFile = {
  library?: unknown;
  libraryItems?: unknown;
};
type ImportedLibraryItems = Parameters<typeof restoreLibraryItems>[0];

function libraryItemsFromFile(
  libraryFile: ExcalidrawLibraryFile,
): LibraryItems {
  const rawItems = Array.isArray(libraryFile.libraryItems)
    ? libraryFile.libraryItems
    : Array.isArray(libraryFile.library)
      ? libraryFile.library
      : [];
  return restoreLibraryItems(rawItems as ImportedLibraryItems, 'published');
}

const WHITEBOARD_LIBRARY_ITEMS: LibraryItems = [
  ...libraryItemsFromFile(softwareArchitectureLibrary as ExcalidrawLibraryFile),
  ...libraryItemsFromFile(systemDesignLibrary as ExcalidrawLibraryFile),
  ...libraryItemsFromFile(
    architectureDiagramComponentsLibrary as ExcalidrawLibraryFile,
  ),
  ...libraryItemsFromFile(drwnioLibrary as ExcalidrawLibraryFile),
  ...libraryItemsFromFile(awesomeIconsLibrary as ExcalidrawLibraryFile),
];

export interface WhiteboardEditorSurfaceProps {
  boardId: string;
  spaceId: string;
  className?: string;
  showArchive?: boolean;
  onBoardLoaded?: (board: WhiteboardDetail) => void;
  onBoardUpdated?: (board: WhiteboardDetail) => void;
  onArchived?: (boardId: string) => void;
  onDeleted?: (boardId: string) => void;
  onRestored?: (board: WhiteboardDetail) => void;
  onClose?: () => void;
}

function readExcalidrawTheme() {
  return typeof document !== 'undefined' &&
    document.documentElement.classList.contains('dark')
    ? THEME.DARK
    : THEME.LIGHT;
}

function subscribeExcalidrawTheme(onStoreChange: () => void) {
  if (typeof document === 'undefined') return () => undefined;
  const observer = new MutationObserver(onStoreChange);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  });
  return () => observer.disconnect();
}

function markElementDeleted(
  element: Parameters<ExcalidrawOnChange>[0][number],
): Parameters<ExcalidrawOnChange>[0][number] {
  return newElementWith(
    element as never,
    { isDeleted: true } as never,
  ) as Parameters<ExcalidrawOnChange>[0][number];
}

/** A loaded board with the editor's images (data URLs) beside its scene. */
interface LoadedBoard {
  board: WhiteboardDetail;
  files: Record<string, unknown>;
  failedImages: number;
}

type WhiteboardShareDialogProps = {
  board: WhiteboardDetail;
  spaceId: string;
  open: boolean;
  onClose: () => void;
};

function WhiteboardShareDialog(props: WhiteboardShareDialogProps) {
  return <>{useWhiteboardShareDialogElement(props)}</>;
}

/** AI-DO's share dialog without the link share section. */
function useWhiteboardShareDialogElement({
  board,
  spaceId,
  open,
  onClose,
}: WhiteboardShareDialogProps): ReactNode {
  const { api } = useRuntime();
  const { t } = useTranslation('apps');
  const [sharing, setSharing] = useState<WhiteboardSharingResponse | null>(
    null,
  );
  const [users, setUsers] = useState<ShareableUserItem[]>([]);
  const [query, setQuery] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const loadSeqRef = useRef(0);

  const load = useCallback(async () => {
    if (!open) return;
    const seq = ++loadSeqRef.current;
    setBusy(true);
    setError(null);
    try {
      const [sharingResponse, usersResponse] = await Promise.all([
        getWhiteboardSharing(api, board.id, spaceId),
        listWhiteboardShareableUsers(api, query, spaceId),
      ]);
      if (seq !== loadSeqRef.current) return;
      setSharing(sharingResponse);
      setUsers(usersResponse);
    } catch (err) {
      if (seq !== loadSeqRef.current) return;
      setError(
        err instanceof Error ? err.message : t('whiteboard.shareLoadFailed'),
      );
    } finally {
      if (seq === loadSeqRef.current) setBusy(false);
    }
  }, [api, board.id, open, query, spaceId, t]);

  useEffect(() => {
    void load();
  }, [load]);

  async function run(action: () => Promise<WhiteboardSharingResponse>) {
    setBusy(true);
    setError(null);
    try {
      setSharing(await action());
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t('whiteboard.shareSaveFailed'),
      );
    } finally {
      setBusy(false);
    }
  }

  const sharePresenter = useMemo(
    () =>
      buildWhiteboardSharePresenter({
        sharing,
        users,
      }),
    [sharing, users],
  );

  return (
    <Dialog
      closeLabel={t('common:actions.close')}
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
      title={t('whiteboard.shareTitle')}
      description={t('whiteboard.shareDescription')}
      maxWidth="max-w-2xl"
      actions={
        <Button variant="secondary" onClick={onClose}>
          {t('common:actions.close')}
        </Button>
      }
    >
      <div className="space-y-5 text-app-ink">
        {error ? (
          <div className="rounded-md border border-rose-500/30 bg-app-danger/10 px-3 py-2 text-sm text-rose-500">
            {error}
          </div>
        ) : null}

        <section className="space-y-3">
          <div className="space-y-1">
            <label className="app-text-control-sm text-app-ink/70">
              {t('ai.search.metadataPeople')}
            </label>
            <input
              aria-label={t('pms.searchUser')}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('pms.searchUser')}
              className="app-text-body-sm w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 outline-none focus:border-app-accent"
            />
          </div>

          {busy && !sharing ? (
            <div className="flex h-20 items-center justify-center text-app-ink/40">
              <Loader2 size={16} className="animate-spin" />
            </div>
          ) : (
            <div className="max-h-72 overflow-y-auto rounded-md border border-app-border">
              {users.length === 0 ? (
                <div className="px-4 py-6 text-center text-sm text-app-ink/45">
                  {t('whiteboard.noShareableUsers')}
                </div>
              ) : (
                sharePresenter.userRows.map(
                  ({ currentShare, shared, user }) => {
                    return (
                      <div
                        key={user.id}
                        className="flex items-center justify-between gap-3 border-b border-app-border px-3 py-2 last:border-b-0"
                      >
                        <div className="min-w-0">
                          <p className="app-text-body-sm truncate text-app-ink">
                            {user.full_name}
                          </p>
                          <p className="app-text-caption truncate text-app-ink/45">
                            {user.email || user.primary_group || ''}
                          </p>
                        </div>
                        {shared && currentShare ? (
                          <div className="flex shrink-0 items-center gap-2">
                            <select
                              value={currentShare.access_level}
                              disabled={busy}
                              onChange={(event) => {
                                void run(() =>
                                  upsertWhiteboardUserShare(
                                    api,
                                    board.id,
                                    user.id,
                                    event.target.value as 'read' | 'edit',
                                    spaceId,
                                  ),
                                );
                              }}
                              className="app-field-input-sm w-auto"
                            >
                              <option value="read">
                                {t('docs.share.access.read')}
                              </option>
                              <option value="edit">
                                {t('docs.share.access.edit')}
                              </option>
                            </select>
                            <Button
                              variant="ghost"
                              size="dense"
                              disabled={busy}
                              onClick={() => {
                                void run(() =>
                                  deleteWhiteboardUserShare(
                                    api,
                                    board.id,
                                    user.id,
                                    spaceId,
                                  ),
                                );
                              }}
                            >
                              {t('pms.bulk.remove')}
                            </Button>
                          </div>
                        ) : (
                          <Button
                            variant="secondary"
                            size="dense"
                            disabled={busy}
                            onClick={() => {
                              void run(() =>
                                upsertWhiteboardUserShare(
                                  api,
                                  board.id,
                                  user.id,
                                  'read',
                                  spaceId,
                                ),
                              );
                            }}
                          >
                            {t('common:actions.add')}
                          </Button>
                        )}
                      </div>
                    );
                  },
                )
              )}
            </div>
          )}
        </section>
      </div>
    </Dialog>
  );
}

export function WhiteboardEditorSurface(props: WhiteboardEditorSurfaceProps) {
  return <>{useWhiteboardEditorSurfaceElement(props)}</>;
}

function useWhiteboardEditorSurfaceElement({
  boardId,
  spaceId,
  className,
  showArchive = true,
  onBoardLoaded,
  onBoardUpdated,
  onArchived,
  onDeleted,
  onRestored,
  onClose,
}: WhiteboardEditorSurfaceProps): ReactNode {
  const { api, fileApi } = useRuntime();
  const { t } = useTranslation('apps');
  const [activeBoard, setActiveBoard] = useState<WhiteboardDetail | null>(null);
  const [editorFiles, setEditorFiles] = useState<Record<string, unknown>>({});
  const [titleDraft, setTitleDraft] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingBoardId, setEditingBoardId] = useState<string | null>(null);
  const [startingEdit, setStartingEdit] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const excalidrawTheme = useSyncExternalStore(
    subscribeExcalidrawTheme,
    readExcalidrawTheme,
    () => THEME.LIGHT,
  );
  const [gridModeEnabled, setGridModeEnabled] = useState(false);
  const [selectedElementsCount, setSelectedElementsCount] = useState(0);
  const [closePending, setClosePending] = useState(false);
  const apiRef = useRef<ExcalidrawImperativeAPI | null>(null);
  const activeBoardRef = useRef<WhiteboardDetail | null>(null);
  const activeBoardIdRef = useRef<string | null>(null);
  const fileStoreRef = useRef<WhiteboardFileStore>(createWhiteboardFileStore());
  const loadSeqRef = useRef(0);
  const canEditCanvas = Boolean(
    activeBoard?.can_edit &&
      !activeBoard.trashed_at &&
      editingBoardId === activeBoard.id,
  );
  const currentSpaceId = spaceId;

  const {
    conflict,
    flushPendingSceneBeforeClose,
    hasUnsavedScene,
    keepLocalSceneOver,
    queueLocalChange,
    rebaseline,
    resetLoadedScene,
    retrySave,
    saveStatus,
  } = useWhiteboardScenePersistence({
    activeBoard,
    activeBoardIdRef,
    activeBoardRef,
    api,
    fileApi,
    fileStoreRef,
    onBoardUpdated,
    setActiveBoard,
    setError,
    spaceId,
    translate: t,
  });

  const titleDirty = Boolean(
    activeBoard?.can_manage &&
      titleDraft.trim() &&
      titleDraft.trim() !== activeBoard.title,
  );
  useNavigationDirty(
    titleDirty ||
      conflict ||
      saveStatus === 'dirty' ||
      saveStatus === 'saving' ||
      saveStatus === 'error',
  );

  useEffect(() => {
    activeBoardIdRef.current = activeBoard?.id ?? null;
    activeBoardRef.current = activeBoard;
  }, [activeBoard]);

  /** The board with its images downloaded (late answers are dropped). */
  const fetchBoard = useCallback(async (): Promise<LoadedBoard | null> => {
    const seq = ++loadSeqRef.current;
    const board = await getWhiteboard(api, boardId, spaceId);
    const scene = normalizeWhiteboardScene(board.scene);
    const hydrated = await hydrateSceneFiles(scene, { fileApi, boardId: board.id });
    if (seq !== loadSeqRef.current) return null;
    return {
      board: { ...board, scene },
      files: hydrated.files,
      failedImages: hydrated.failed,
    };
  }, [api, boardId, fileApi, spaceId]);

  const applyLoaded = useCallback(
    (loaded: LoadedBoard) => {
      const { board } = loaded;
      fileStoreRef.current = createWhiteboardFileStore(board.scene);
      resetLoadedScene(board.scene, board.revision);
      setSelectedElementsCount(0);
      setEditorFiles(loaded.files);
      setActiveBoard(board);
      setTitleDraft(board.title);
      setGridModeEnabled(Boolean(board.scene.appState.gridModeEnabled));
      if (loaded.failedImages > 0) {
        setError(t('whiteboardApp.imageLoadFailed'));
      }
      onBoardLoaded?.(board);
    },
    [onBoardLoaded, resetLoadedScene, t],
  );

  const loadBoard = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const loaded = await fetchBoard();
      if (!loaded) return;
      applyLoaded(loaded);
      void recordWhiteboardView(api, boardId, spaceId).catch(() => undefined);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('whiteboard.loadFailed'));
      setActiveBoard(null);
      setGridModeEnabled(false);
      setSelectedElementsCount(0);
    } finally {
      setLoading(false);
    }
  }, [api, applyLoaded, boardId, fetchBoard, spaceId, t]);

  useEffect(() => {
    void loadBoard();
    return () => {
      // A board switch drops answers still on their way.
      loadSeqRef.current += 1;
    };
  }, [loadBoard]);

  /** The editor's current scene is the saved one (see `rebaseline`). */
  const rebaselineFromEditor = useCallback(() => {
    const editor = apiRef.current;
    if (!editor) return;
    rebaseline({
      elements:
        editor.getSceneElementsIncludingDeleted() as Parameters<ExcalidrawOnChange>[0],
      appState: editor.getAppState(),
      files: editor.getFiles(),
    });
  }, [rebaseline]);

  /** Put a freshly loaded scene into the mounted editor. */
  const applySceneToEditor = useCallback(
    (loaded: LoadedBoard) => {
      const editor = apiRef.current;
      if (!editor) return;
      const files = Object.values(loaded.files);
      if (files.length > 0) {
        editor.addFiles(files as BinaryFileData[]);
      }
      editor.updateScene({
        elements: loaded.board.scene.elements as never,
        appState: {
          ...loaded.board.scene.appState,
          name: loaded.board.title,
        } as never,
        captureUpdate: CaptureUpdateAction.NEVER,
      });
      rebaselineFromEditor();
    },
    [rebaselineFromEditor],
  );

  /** Reload in place: the latest saved scene replaces the editor's. */
  const reloadLatest = useCallback(async () => {
    setError(null);
    try {
      const loaded = await fetchBoard();
      if (!loaded || loaded.board.id !== activeBoardIdRef.current) return;
      applyLoaded(loaded);
      applySceneToEditor(loaded);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('whiteboard.loadFailed'));
    }
  }, [applyLoaded, applySceneToEditor, fetchBoard, t]);

  const startEditing = useCallback(async () => {
    const board = activeBoardRef.current;
    if (!board?.can_edit || startingEdit) return;
    setStartingEdit(true);
    try {
      // Edit the latest saved scene: someone may have saved since loading.
      const loaded = await fetchBoard();
      if (!loaded || loaded.board.id !== board.id) return;
      if (loaded.board.revision !== board.revision) {
        applyLoaded(loaded);
        applySceneToEditor(loaded);
      }
      if (loaded.board.can_edit && !loaded.board.trashed_at) {
        rebaselineFromEditor();
        setEditingBoardId(loaded.board.id);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : t('whiteboard.loadFailed'));
    } finally {
      setStartingEdit(false);
    }
  }, [applyLoaded, applySceneToEditor, fetchBoard, rebaselineFromEditor, startingEdit, t]);

  /** Keep mine: save the editor's scene over the latest saved revision. */
  const keepMine = useCallback(async () => {
    try {
      const latest = await getWhiteboard(api, boardId, spaceId);
      if (latest.id !== activeBoardIdRef.current) return;
      keepLocalSceneOver(latest.revision);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('whiteboard.loadFailed'));
    }
  }, [api, boardId, keepLocalSceneOver, spaceId, t]);

  const saveTitle = useCallback(async (): Promise<boolean> => {
    const board = activeBoardRef.current;
    if (!board || !board.can_manage) return true;
    const nextTitle = titleDraft.trim();
    if (!nextTitle || nextTitle === board.title) return true;
    try {
      const updated = await updateWhiteboard(
        api,
        board.id,
        { title: nextTitle },
        spaceId,
      );
      if (activeBoardIdRef.current !== board.id) return true;
      // The title answer carries the stored scene; keep the editor's.
      setActiveBoard((current) =>
        current && current.id === board.id
          ? { ...current, title: updated.title, updated_at: updated.updated_at }
          : current,
      );
      onBoardUpdated?.({ ...board, title: updated.title, updated_at: updated.updated_at });
      return true;
    } catch (err) {
      if (activeBoardIdRef.current !== board.id) return false;
      setError(
        err instanceof Error ? err.message : t('whiteboard.renameFailed'),
      );
      return false;
    }
  }, [api, onBoardUpdated, spaceId, titleDraft, t]);

  const handleArchive = useCallback(async () => {
    const board = activeBoardRef.current;
    if (!board || !board.can_manage) return;
    // Unsaved work is saved first; trashing a board mid-save would lose it.
    if (hasUnsavedScene() && !(await flushPendingSceneBeforeClose())) return;
    try {
      await deleteWhiteboard(api, board.id, spaceId);
      if (activeBoardIdRef.current === board.id) onArchived?.(board.id);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t('whiteboard.archiveFailed'),
      );
    }
  }, [api, flushPendingSceneBeforeClose, hasUnsavedScene, onArchived, spaceId, t]);

  const handlePermanentDelete = useCallback(async () => {
    const board = activeBoardRef.current;
    if (!board || !board.can_manage || !board.trashed_at) return;
    if (
      !window.confirm(
        t('whiteboard.deletePermanentConfirm', { title: board.title }),
      )
    )
      return;
    try {
      await permanentlyDeleteWhiteboard(api, board.id, spaceId);
      if (activeBoardIdRef.current === board.id) onDeleted?.(board.id);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t('whiteboard.deleteFailed'),
      );
    }
  }, [api, onDeleted, spaceId, t]);

  const handleRestore = useCallback(async () => {
    const board = activeBoardRef.current;
    if (!board || !board.can_manage || !board.trashed_at) return;
    try {
      const restored = await restoreWhiteboard(api, board.id, spaceId);
      if (activeBoardIdRef.current !== board.id) return;
      const merged = {
        ...board,
        ...restored,
        scene: board.scene,
      };
      setActiveBoard(merged);
      onBoardUpdated?.(merged);
      onRestored?.(merged);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t('whiteboard.restoreFailed'),
      );
    }
  }, [api, onBoardUpdated, onRestored, spaceId, t]);

  const handleFavorite = useCallback(async () => {
    const board = activeBoardRef.current;
    if (!board) return;
    try {
      const response = await toggleWhiteboardFavorite(api, board.id, spaceId);
      if (activeBoardIdRef.current !== board.id) return;
      setActiveBoard((current) =>
        current && current.id === board.id
          ? { ...current, is_favorite: response.is_favorite }
          : current,
      );
      onBoardUpdated?.({ ...board, is_favorite: response.is_favorite });
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t('whiteboard.favoriteFailed'),
      );
    }
  }, [api, onBoardUpdated, spaceId, t]);

  const handleVisibilityChange = useCallback(
    async (visibility: WhiteboardVisibility) => {
      const board = activeBoardRef.current;
      if (!board || !board.can_manage) return;
      if ((visibility === 'personal') === board.is_private) return;
      try {
        const updated =
          visibility === 'personal'
            ? await deleteWhiteboardTarget(api, board.id, spaceId)
            : await updateWhiteboardTarget(
                api,
                board.id,
                {
                  app: 'whiteboard',
                  type: 'workspace_sidebar',
                  id: currentSpaceId,
                  sort_order: 0,
                },
                spaceId,
              );
        if (activeBoardIdRef.current !== board.id) return;
        // Visibility changes leave the scene alone; keep the editor's.
        const merged = { ...board, ...updated, scene: board.scene };
        setActiveBoard((current) =>
          current && current.id === board.id
            ? { ...current, ...updated, scene: current.scene }
            : current,
        );
        onBoardUpdated?.(merged);
      } catch (err) {
        setError(
          err instanceof Error
            ? err.message
            : t('whiteboard.visibilityUpdateFailed'),
        );
      }
    },
    [api, currentSpaceId, onBoardUpdated, spaceId, t],
  );

  const currentApi = () => {
    const editor = apiRef.current;
    if (!editor || !activeBoard) return null;
    return editor;
  };

  async function exportPng() {
    const editor = currentApi();
    if (!editor || !activeBoard) return;
    const blob = await exportToBlob({
      elements: editor.getSceneElements(),
      appState: editor.getAppState(),
      files: editor.getFiles(),
      mimeType: 'image/png',
      exportPadding: 16,
    });
    downloadBlobAsFile(blob, safeFilename(activeBoard.title, 'png'));
  }

  async function exportSvg() {
    const editor = currentApi();
    if (!editor || !activeBoard) return;
    const svg = await exportToSvg({
      elements: editor.getSceneElements(),
      appState: editor.getAppState(),
      files: editor.getFiles(),
      exportPadding: 16,
    });
    downloadBlobAsFile(
      new Blob([svg.outerHTML], { type: 'image/svg+xml' }),
      safeFilename(activeBoard.title, 'svg'),
    );
  }

  function exportJson() {
    const editor = currentApi();
    if (!editor || !activeBoard) return;
    const json = serializeAsJSON(
      editor.getSceneElementsIncludingDeleted(),
      editor.getAppState(),
      editor.getFiles(),
      'local',
    );
    downloadBlobAsFile(
      new Blob([json], { type: 'application/json' }),
      safeFilename(activeBoard.title, 'excalidraw'),
    );
  }

  const toggleGridMode = useCallback(() => {
    const editor = apiRef.current;
    if (!editor || !activeBoard) return;
    const appState = editor.getAppState();
    const nextGridMode = !appState.gridModeEnabled;
    const nextAppState = {
      ...appState,
      gridModeEnabled: nextGridMode,
    } as Parameters<ExcalidrawOnChange>[1];
    editor.updateScene({
      appState: nextAppState as never,
      captureUpdate: CaptureUpdateAction.IMMEDIATELY,
    });
    setGridModeEnabled(nextGridMode);
    if (canEditCanvas) {
      queueLocalChange({
        elements:
          editor.getSceneElementsIncludingDeleted() as Parameters<ExcalidrawOnChange>[0],
        appState: nextAppState,
        files: editor.getFiles(),
      });
    }
  }, [activeBoard, canEditCanvas, queueLocalChange]);

  const deleteSelectedElements = useCallback(() => {
    const editor = apiRef.current;
    if (!editor || !canEditCanvas) return;
    const appState = editor.getAppState();
    const selectedElementIds = appState.selectedElementIds;
    const elements =
      editor.getSceneElementsIncludingDeleted() as Parameters<ExcalidrawOnChange>[0];
    let deletedCount = 0;
    const nextElements = elements.map((element) => {
      const id = elementId(element);
      if (
        id &&
        selectedElementIds[id] === true &&
        !(element as WhiteboardElement).isDeleted
      ) {
        deletedCount += 1;
        return markElementDeleted(element);
      }
      return element;
    }) as Parameters<ExcalidrawOnChange>[0];
    if (deletedCount === 0) return;
    const nextAppState = {
      ...appState,
      selectedElementIds: {},
      selectedGroupIds: {},
      editingGroupId: null,
    } as Parameters<ExcalidrawOnChange>[1];
    editor.updateScene({
      elements: nextElements as never,
      appState: nextAppState as never,
      captureUpdate: CaptureUpdateAction.IMMEDIATELY,
    });
    setSelectedElementsCount(0);
    queueLocalChange({
      elements: nextElements,
      appState: nextAppState,
      files: editor.getFiles(),
    });
  }, [canEditCanvas, queueLocalChange]);

  const handleClose = useCallback(async () => {
    if (!onClose || closePending) return;
    if (!activeBoard?.can_edit && !activeBoard?.can_manage) {
      onClose();
      return;
    }

    setClosePending(true);
    const titleSaved = await saveTitle();
    const sceneSaved = titleSaved
      ? await flushPendingSceneBeforeClose()
      : false;
    if (titleSaved && sceneSaved) {
      onClose();
      return;
    }
    setClosePending(false);
  }, [
    activeBoard?.can_edit,
    activeBoard?.can_manage,
    closePending,
    flushPendingSceneBeforeClose,
    onClose,
    saveTitle,
  ]);

  // Built once per board: Excalidraw reads initialData only when it mounts.
  const initialData = useMemo<ExcalidrawInitialDataState | null>(() => {
    if (!activeBoard) return null;
    const scene: WhiteboardScene = normalizeWhiteboardScene(activeBoard.scene);
    return {
      elements: scene.elements as ExcalidrawInitialDataState['elements'],
      appState: {
        ...scene.appState,
        name: activeBoard.title,
      } as ExcalidrawInitialDataState['appState'],
      files: editorFiles as ExcalidrawInitialDataState['files'],
      libraryItems: WHITEBOARD_LIBRARY_ITEMS,
    };
  }, [activeBoard?.id]);

  const label = saveLabel(saveStatus, t);
  const activeBoardVisibility: WhiteboardVisibility = activeBoard?.is_private
    ? 'personal'
    : 'workspace';

  return (
    <div
      className={cn(
        'flex min-h-0 flex-1 flex-col bg-app-bg text-app-ink',
        className,
      )}
    >
      {error ? (
        <div className="border-b border-app-danger-border bg-app-danger-bg px-4 py-2 text-sm text-app-danger-text dark:border-app-danger-border dark:bg-app-danger-bg dark:text-app-danger-text">
          {error}
        </div>
      ) : null}

      {conflict ? (
        <div
          role="alert"
          className="flex flex-wrap items-center justify-between gap-3 border-b border-app-warning/40 bg-app-warning/10 px-4 py-2"
        >
          <div className="min-w-0">
            <p className="app-text-body-sm font-semibold text-app-ink">
              {t('whiteboardApp.conflict.title')}
            </p>
            <p className="app-text-caption text-app-ink/60">
              {t('whiteboardApp.conflict.description')}
            </p>
          </div>
          <div className="flex shrink-0 gap-2">
            <Button size="dense" variant="secondary" onClick={() => void reloadLatest()}>
              {t('whiteboardApp.conflict.reload')}
            </Button>
            <Button size="dense" variant="primary" onClick={() => void keepMine()}>
              {t('whiteboardApp.conflict.keep')}
            </Button>
          </div>
        </div>
      ) : null}

      {!activeBoard ? (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          {loading ? (
            <Loader2 size={24} className="animate-spin text-app-ink/40" />
          ) : (
            <p className="app-text-body text-app-ink/45">
              {t('whiteboard.loadFailed')}
            </p>
          )}
        </div>
      ) : (
        <>
          <div className="flex min-h-[56px] items-center gap-2 border-b border-app-border bg-app-surface px-4">
            <input
              aria-label={t('whiteboard.titleInput')}
              value={titleDraft}
              onChange={(event) => setTitleDraft(event.target.value)}
              onBlur={() => void saveTitle()}
              onKeyDown={(event) => {
                if (event.key === 'Enter') event.currentTarget.blur();
              }}
              disabled={!activeBoard.can_manage}
              maxLength={200}
              className="app-text-title-md min-w-0 flex-1 rounded-md bg-transparent px-2 py-1 text-app-ink outline-none transition-colors enabled:hover:bg-app-surface-hover enabled:focus:bg-app-bg"
            />
            {label ? (
              saveStatus === 'error' && !conflict ? (
                <button
                  type="button"
                  onClick={retrySave}
                  className="app-text-caption shrink-0 rounded px-1 text-rose-500 underline-offset-2 hover:underline"
                  title={t('common:actions.retry')}
                >
                  {label}
                </button>
              ) : (
                <span
                  className={cn(
                    'app-text-caption shrink-0',
                    saveStatus === 'error' ? 'text-rose-500' : 'text-app-ink/45',
                  )}
                >
                  {label}
                </span>
              )
            ) : null}
            {activeBoard.can_edit &&
            !activeBoard.trashed_at &&
            !canEditCanvas ? (
              <button
                type="button"
                onClick={() => void startEditing()}
                disabled={startingEdit}
                className="inline-flex items-center gap-2 rounded-md bg-app-accent px-3 py-1.5 text-[length:var(--ui-text-control)] font-medium text-app-accent-fg hover:opacity-90 disabled:opacity-60"
              >
                {startingEdit ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Pencil size={14} />
                )}
                {t('whiteboard.startEditing')}
              </button>
            ) : null}
            <button
              type="button"
              onClick={handleFavorite}
              className={cn(
                'inline-flex size-8 items-center justify-center rounded-md transition-colors hover:bg-app-surface-hover',
                activeBoard.is_favorite
                  ? 'text-app-warning'
                  : 'text-app-ink/60 hover:text-app-ink',
              )}
              title={
                activeBoard.is_favorite
                  ? t('whiteboard.unfavorite')
                  : t('pms.actions.favorite')
              }
            >
              <Star
                size={15}
                fill={activeBoard.is_favorite ? 'currentColor' : 'none'}
              />
            </button>
            <button
              type="button"
              onClick={toggleGridMode}
              aria-pressed={gridModeEnabled}
              className={cn(
                'inline-flex size-8 items-center justify-center rounded-md transition-colors hover:bg-app-surface-hover',
                gridModeEnabled
                  ? 'bg-app-surface-hover text-app-ink'
                  : 'text-app-ink/60 hover:text-app-ink',
              )}
              title={
                gridModeEnabled
                  ? t('whiteboard.hideGrid')
                  : t('whiteboard.showGrid')
              }
            >
              <Grid3X3 size={15} />
            </button>
            {canEditCanvas ? (
              <button
                type="button"
                onClick={deleteSelectedElements}
                disabled={selectedElementsCount === 0}
                className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 transition-colors hover:bg-app-surface-hover hover:text-rose-500 disabled:cursor-not-allowed disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-app-ink/60"
                title={
                  selectedElementsCount > 0
                    ? t('whiteboard.deleteSelectedElements', {
                        count: selectedElementsCount,
                      })
                    : t('whiteboard.deleteSelectedElementsDisabled')
                }
              >
                <Trash2 size={15} />
              </button>
            ) : null}
            <button
              type="button"
              onClick={() => void exportPng()}
              className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink"
              title={t('whiteboard.downloadPng')}
            >
              <Image size={15} />
            </button>
            <button
              type="button"
              onClick={() => void exportSvg()}
              className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink"
              title={t('whiteboard.downloadSvg')}
            >
              <Download size={15} />
            </button>
            <button
              type="button"
              onClick={exportJson}
              className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink"
              title={t('whiteboard.downloadJson')}
            >
              <FileJson size={15} />
            </button>
            {activeBoard.can_share ? (
              <button
                type="button"
                onClick={() => setShareOpen(true)}
                className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink"
                title={t('common:actions.share')}
              >
                <Share2 size={15} />
              </button>
            ) : null}
            <DropdownMenu
              align="end"
              side="bottom"
              trigger={
                <button
                  type="button"
                  aria-label={t('whiteboard.actionsMenu')}
                  className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink"
                >
                  <MoreHorizontal size={16} />
                </button>
              }
              items={buildWhiteboardVisibilityMenuItems({
                canManage: activeBoard.can_manage,
                currentVisibility: activeBoardVisibility,
                onChange: (visibility) =>
                  void handleVisibilityChange(visibility),
                t,
              })}
            />
            <button
              type="button"
              onClick={() => void (conflict || saveStatus === 'idle' || saveStatus === 'saved' ? reloadLatest() : undefined)}
              disabled={!(conflict || saveStatus === 'idle' || saveStatus === 'saved')}
              className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink disabled:opacity-35"
              title={t('common:actions.reload')}
            >
              <RefreshCcw size={15} />
            </button>
            {showArchive &&
            activeBoard.can_manage &&
            !activeBoard.trashed_at ? (
              <button
                type="button"
                onClick={() => void handleArchive()}
                className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-rose-500"
                title={t('whiteboard.moveToTrash')}
              >
                <Trash2 size={15} />
              </button>
            ) : null}
            {showArchive &&
            activeBoard.can_manage &&
            activeBoard.trashed_at ? (
              <button
                type="button"
                onClick={() => void handleRestore()}
                className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-success-text"
                title={t('whiteboard.restore')}
              >
                <ArchiveRestore size={15} />
              </button>
            ) : null}
            {showArchive &&
            activeBoard.can_manage &&
            activeBoard.trashed_at ? (
              <button
                type="button"
                onClick={() => void handlePermanentDelete()}
                className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-rose-500"
                title={t('common:actions.deletePermanently')}
              >
                <Trash2 size={15} />
              </button>
            ) : null}
            {onClose ? (
              <>
                <div className="mx-1 h-6 w-px bg-app-border" />
                <button
                  type="button"
                  onClick={() => void handleClose()}
                  disabled={closePending}
                  className="inline-flex size-8 items-center justify-center rounded-md text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink disabled:cursor-wait disabled:opacity-60"
                  title={
                    closePending
                      ? t('whiteboard.savingBeforeClose')
                      : t('whiteboard.closeWhiteboard')
                  }
                >
                  {closePending ? (
                    <Loader2 size={15} className="animate-spin" />
                  ) : (
                    <X size={16} />
                  )}
                </button>
              </>
            ) : null}
          </div>

          <div className="relative min-h-0 flex-1 bg-white">
            {loading ? (
              <div className="absolute inset-0 z-10 flex items-center justify-center bg-app-bg/60">
                <Loader2 size={24} className="animate-spin text-app-ink/50" />
              </div>
            ) : null}
            {initialData ? (
              <Excalidraw
                key={activeBoard.id}
                excalidrawAPI={(editor) => {
                  apiRef.current = editor;
                }}
                initialData={initialData}
                onChange={(elements, appState, files) => {
                  setGridModeEnabled(Boolean(appState.gridModeEnabled));
                  setSelectedElementsCount(
                    selectedElementCount(
                      apiRef.current?.getSceneElementsIncludingDeleted() ??
                        elements,
                      appState.selectedElementIds,
                    ),
                  );
                  if (!canEditCanvas) return;
                  queueLocalChange({
                    elements: (apiRef.current?.getSceneElementsIncludingDeleted() ??
                      elements) as Parameters<ExcalidrawOnChange>[0],
                    appState,
                    files,
                  });
                }}
                viewModeEnabled={!canEditCanvas}
                theme={excalidrawTheme}
                aiEnabled={false}
                validateEmbeddable={false}
                UIOptions={{
                  canvasActions: {
                    loadScene: false,
                    saveToActiveFile: false,
                    export: false,
                    toggleTheme: false,
                  },
                }}
              />
            ) : null}
          </div>

          {shareOpen ? (
            <WhiteboardShareDialog
              board={activeBoard}
              spaceId={spaceId}
              open={shareOpen}
              onClose={() => setShareOpen(false)}
            />
          ) : null}
        </>
      )}
    </div>
  );
}
