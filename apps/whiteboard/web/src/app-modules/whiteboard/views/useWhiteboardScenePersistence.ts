import {
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { getSceneVersion, serializeAsJSON } from '@excalidraw/excalidraw';
import type { ExcalidrawProps } from '@excalidraw/excalidraw/types';

import type { AppApi } from '../../../platform/runtime/runtime-context';
import { apiErrorMessage } from '../../../platform/api/api-error';
import {
  WhiteboardApiError,
  updateWhiteboard,
  type FileApi,
  type WhiteboardDetail,
  type WhiteboardScene,
} from '../api/whiteboard-api';
import {
  WhiteboardImageError,
  storeSceneFiles,
  type WhiteboardFileStore,
} from '../api/whiteboard-files';
import { sceneFromExcalidraw, sceneSignature } from './whiteboard-scene';
import {
  WHITEBOARD_CLOSE_FLUSH_RETRY_MS,
  WHITEBOARD_LOCAL_CHANGE_FLUSH_MS,
  WHITEBOARD_SCENE_SAVE_DEBOUNCE_MS,
  WHITEBOARD_SCENE_SAVE_RETRY_MS,
  describeWhiteboardLocalSceneChange,
  getWhiteboardSaveSettledDelay,
  resolveWhiteboardCloseFlushStep,
  resolveWhiteboardSaveSettledStatus,
  resolveWhiteboardSceneSaveSchedule,
} from './whiteboard-editor-session';
import { waitFor, type SaveStatus } from './whiteboard-editor-utils';

/**
 * AI-DO views/useWhiteboardScenePersistence.ts without the Yjs publishing:
 * local changes are batched (160 ms), debounced (800 ms) and saved with
 * `PATCH …/items/{id}` as before, and only one save is in flight.
 *
 * miy changes:
 * - every scene save names the revision it started from; a 409
 *   `whiteboard_revision_conflict` stops saving, keeps the unsaved scene and
 *   reports the conflict (the surface offers reload or keep mine);
 * - new images are stored in the core storage before the scene is saved
 *   (whiteboard-files.ts), resuming with the same request IDs after a
 *   failure;
 * - a failed save keeps its scene pending (it is sent again by the next change,
 *   the retry button or the close flush) instead of dropping it;
 * - a late answer for another board changes nothing on the current one.
 */
type ExcalidrawOnChange = NonNullable<ExcalidrawProps['onChange']>;

export type PendingWhiteboardEditorChange = {
  elements: Parameters<ExcalidrawOnChange>[0];
  appState: Parameters<ExcalidrawOnChange>[1];
  files: Parameters<ExcalidrawOnChange>[2];
};

export type UseWhiteboardScenePersistenceOptions = {
  activeBoard: WhiteboardDetail | null;
  activeBoardIdRef: MutableRefObject<string | null>;
  activeBoardRef: MutableRefObject<WhiteboardDetail | null>;
  api: AppApi | null | undefined;
  fileApi: FileApi;
  fileStoreRef: MutableRefObject<WhiteboardFileStore>;
  onBoardUpdated?: (board: WhiteboardDetail) => void;
  setActiveBoard: Dispatch<SetStateAction<WhiteboardDetail | null>>;
  setError: Dispatch<SetStateAction<string | null>>;
  spaceId: string;
  translate: (key: string) => string;
};

export type WhiteboardScenePersistenceRuntime = {
  conflict: boolean;
  flushPendingSceneBeforeClose: () => Promise<boolean>;
  hasUnsavedScene: () => boolean;
  /** Keep mine: continue from `revision` (the latest saved) and save again. */
  keepLocalSceneOver: (revision: number) => void;
  queueLocalChange: (change: PendingWhiteboardEditorChange) => void;
  /**
   * Take the editor's current scene as the saved one (no save). Excalidraw
   * normalizes a loaded scene (restored defaults, text measured with the
   * loaded fonts); without this, starting to edit would save that difference
   * at once and could conflict with another editor's newer save.
   */
  rebaseline: (change: PendingWhiteboardEditorChange) => void;
  resetLoadedScene: (scene: WhiteboardScene, revision: number) => void;
  retrySave: () => void;
  saveStatus: SaveStatus;
};

function clearWindowTimer(timerRef: MutableRefObject<number | null>): void {
  if (timerRef.current === null) return;
  window.clearTimeout(timerRef.current);
  timerRef.current = null;
}

function elementsVersion(elements: readonly unknown[]): number {
  return getSceneVersion(elements as never);
}

function saveErrorMessage(
  error: unknown,
  translate: (key: string) => string,
): string {
  if (error instanceof WhiteboardImageError) {
    return translate(
      error.reason === 'too-large'
        ? 'whiteboardApp.imageTooLarge'
        : error.reason === 'unsupported'
          ? 'whiteboardApp.imageUnsupported'
          : 'whiteboardApp.imageUploadFailed',
    );
  }
  if (error instanceof WhiteboardApiError) {
    if (error.code === 'media_upload_failed' || error.message.startsWith('upload failed')) {
      return translate('whiteboardApp.imageUploadFailed');
    }
    return error.message || translate('whiteboard.saveFailed');
  }
  return apiErrorMessage(error, translate('whiteboard.saveFailed'));
}

export function useWhiteboardScenePersistence({
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
  translate,
}: UseWhiteboardScenePersistenceOptions): WhiteboardScenePersistenceRuntime {
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle');
  const [conflict, setConflict] = useState(false);
  const conflictRef = useRef(false);
  const revisionRef = useRef<number | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  const saveInFlightRef = useRef(false);
  const pendingSceneRef = useRef<WhiteboardScene | null>(null);
  const pendingSceneSignatureRef = useRef<string | null>(null);
  const lastSavedSceneSignatureRef = useRef<string | null>(null);
  const pendingLocalChangeRef = useRef<PendingWhiteboardEditorChange | null>(
    null,
  );
  const localChangeFlushTimerRef = useRef<number | null>(null);
  const queuedLocalElementsVersionRef = useRef<number | null>(null);
  const saveSettledTimerRef = useRef<number | null>(null);
  const lastSaveFailedRef = useRef(false);

  useEffect(
    () => () => {
      clearWindowTimer(saveTimerRef);
      clearWindowTimer(localChangeFlushTimerRef);
      clearWindowTimer(saveSettledTimerRef);
    },
    [],
  );

  const setConflictState = useCallback((value: boolean) => {
    conflictRef.current = value;
    setConflict(value);
  }, []);

  const resetLoadedScene = useCallback(
    (scene: WhiteboardScene, revision: number) => {
      lastSavedSceneSignatureRef.current = sceneSignature(scene);
      revisionRef.current = revision;
      pendingSceneRef.current = null;
      pendingSceneSignatureRef.current = null;
      pendingLocalChangeRef.current = null;
      queuedLocalElementsVersionRef.current = null;
      lastSaveFailedRef.current = false;
      clearWindowTimer(saveTimerRef);
      clearWindowTimer(localChangeFlushTimerRef);
      clearWindowTimer(saveSettledTimerRef);
      setConflictState(false);
      setSaveStatus('idle');
    },
    [setConflictState],
  );

  const flushSceneSave = useCallback(
    async (resolvedBoardId: string): Promise<boolean> => {
      if (!api || saveInFlightRef.current || conflictRef.current) return false;
      const scene = pendingSceneRef.current;
      const signature = pendingSceneSignatureRef.current;
      if (!scene || !signature) return true;
      const baseRevision = revisionRef.current;
      if (baseRevision === null) return false;
      pendingSceneRef.current = null;
      pendingSceneSignatureRef.current = null;
      saveInFlightRef.current = true;
      lastSaveFailedRef.current = false;
      setSaveStatus('saving');
      let saved = false;
      let conflicted = false;
      try {
        const storedScene = await storeSceneFiles(scene, fileStoreRef.current, {
          api,
          fileApi,
          boardId: resolvedBoardId,
          spaceId,
        });
        const updated = await updateWhiteboard(
          api,
          resolvedBoardId,
          { scene: storedScene, base_revision: baseRevision },
          spaceId,
        );
        if (activeBoardIdRef.current !== resolvedBoardId) {
          // The editor moved to another board meanwhile.
          return true;
        }
        lastSavedSceneSignatureRef.current = signature;
        revisionRef.current = updated.revision;
        setActiveBoard((current) =>
          current && current.id === resolvedBoardId
            ? { ...current, ...updated, scene }
            : current,
        );
        const currentBoard = activeBoardRef.current;
        if (currentBoard && currentBoard.id === resolvedBoardId) {
          onBoardUpdated?.({ ...currentBoard, ...updated, scene });
        }
        saved = true;
      } catch (err) {
        if (activeBoardIdRef.current !== resolvedBoardId) {
          return false;
        }
        lastSaveFailedRef.current = true;
        // Keep the unsaved scene unless a newer one is already queued.
        if (pendingSceneRef.current === null) {
          pendingSceneRef.current = scene;
          pendingSceneSignatureRef.current = signature;
        }
        if (
          err instanceof WhiteboardApiError &&
          err.code === 'whiteboard_revision_conflict'
        ) {
          conflicted = true;
          setConflictState(true);
        } else {
          setError(saveErrorMessage(err, translate));
        }
        setSaveStatus('error');
      } finally {
        saveInFlightRef.current = false;
        if (
          !conflicted &&
          saved &&
          pendingSceneRef.current !== null &&
          activeBoardIdRef.current === resolvedBoardId &&
          saveTimerRef.current === null
        ) {
          saveTimerRef.current = window.setTimeout(() => {
            saveTimerRef.current = null;
            void flushSceneSave(resolvedBoardId);
          }, WHITEBOARD_SCENE_SAVE_RETRY_MS);
        } else if (saved) {
          setSaveStatus('saved');
          clearWindowTimer(saveSettledTimerRef);
          saveSettledTimerRef.current = window.setTimeout(
            () => {
              saveSettledTimerRef.current = null;
              const settledStatus = resolveWhiteboardSaveSettledStatus({
                localChangeTimerActive:
                  localChangeFlushTimerRef.current !== null,
                pendingLocalChange: pendingLocalChangeRef.current !== null,
                pendingScene: pendingSceneRef.current !== null,
                saveInFlight: saveInFlightRef.current,
              });
              if (settledStatus) {
                setSaveStatus(settledStatus);
              }
            },
            getWhiteboardSaveSettledDelay({
              localChangeFlushMs: WHITEBOARD_LOCAL_CHANGE_FLUSH_MS,
            }),
          );
        }
      }
      return saved;
    },
    [
      activeBoardIdRef,
      activeBoardRef,
      api,
      fileApi,
      fileStoreRef,
      onBoardUpdated,
      setActiveBoard,
      setConflictState,
      setError,
      spaceId,
      translate,
    ],
  );

  const scheduleSceneSave = useCallback(
    (scene: WhiteboardScene, knownSignature?: string) => {
      const signature = knownSignature ?? sceneSignature(scene);
      const decision = resolveWhiteboardSceneSaveSchedule({
        boardId: activeBoard?.id ?? null,
        canEdit: Boolean(activeBoard?.can_edit),
        lastSavedSignature: lastSavedSceneSignatureRef.current,
        pendingSignature: pendingSceneSignatureRef.current,
        saveInFlight: saveInFlightRef.current,
        saveTimerActive: saveTimerRef.current !== null,
        signature,
      });
      if (!decision.shouldQueue) {
        return;
      }
      pendingSceneRef.current = scene;
      pendingSceneSignatureRef.current = signature;
      const nextStatus = decision.nextStatus;
      if (nextStatus && !conflictRef.current) {
        setSaveStatus((current) =>
          current === 'saving' ? current : nextStatus,
        );
      }
      if (!decision.shouldSchedule || !activeBoard?.id || conflictRef.current) {
        return;
      }

      const resolvedBoardId = activeBoard.id;
      saveTimerRef.current = window.setTimeout(() => {
        saveTimerRef.current = null;
        void flushSceneSave(resolvedBoardId);
      }, WHITEBOARD_SCENE_SAVE_DEBOUNCE_MS);
    },
    [activeBoard?.can_edit, activeBoard?.id, flushSceneSave],
  );

  const flushQueuedLocalChange = useCallback(() => {
    localChangeFlushTimerRef.current = null;
    const pending = pendingLocalChangeRef.current;
    pendingLocalChangeRef.current = null;
    queuedLocalElementsVersionRef.current = null;
    if (!pending) {
      return;
    }

    const scene = sceneFromExcalidraw(
      pending.elements,
      pending.appState,
      pending.files,
      (elements, appState, files) =>
        serializeAsJSON(elements, appState, files, 'local'),
    );
    const signature = sceneSignature(scene);
    const decision = describeWhiteboardLocalSceneChange({
      lastAppliedRemoteSignature: null,
      lastPublishedCollabSignature: null,
      lastSavedSignature: lastSavedSceneSignatureRef.current,
      pendingScene: pendingSceneRef.current !== null,
      pendingSceneSignature: pendingSceneSignatureRef.current,
      saveInFlight: saveInFlightRef.current,
      signature,
    });
    if (decision.type === 'ignore-pending-duplicate') {
      return;
    }
    if (decision.type !== 'queue-local-scene') {
      if (decision.nextStatus === 'idle') {
        setSaveStatus((current) => (current === 'dirty' ? 'idle' : current));
      } else if (decision.nextStatus) {
        setSaveStatus(decision.nextStatus);
      }
      return;
    }
    scheduleSceneSave(scene, signature);
  }, [scheduleSceneSave]);

  const queueLocalChange = useCallback(
    (change: PendingWhiteboardEditorChange) => {
      const nextVersion = elementsVersion(change.elements);
      if (
        localChangeFlushTimerRef.current !== null &&
        queuedLocalElementsVersionRef.current === nextVersion
      ) {
        pendingLocalChangeRef.current = change;
        return;
      }
      queuedLocalElementsVersionRef.current = nextVersion;
      pendingLocalChangeRef.current = change;
      if (localChangeFlushTimerRef.current !== null) return;
      localChangeFlushTimerRef.current = window.setTimeout(
        flushQueuedLocalChange,
        WHITEBOARD_LOCAL_CHANGE_FLUSH_MS,
      );
    },
    [flushQueuedLocalChange],
  );

  const rebaseline = useCallback((change: PendingWhiteboardEditorChange) => {
    if (
      saveInFlightRef.current ||
      pendingSceneRef.current !== null ||
      conflictRef.current
    ) {
      return;
    }
    clearWindowTimer(localChangeFlushTimerRef);
    pendingLocalChangeRef.current = null;
    queuedLocalElementsVersionRef.current = null;
    const scene = sceneFromExcalidraw(
      change.elements,
      change.appState,
      change.files,
      (elements, appState, files) =>
        serializeAsJSON(elements, appState, files, 'local'),
    );
    lastSavedSceneSignatureRef.current = sceneSignature(scene);
  }, []);

  const retrySave = useCallback(() => {
    const boardId = activeBoardIdRef.current;
    if (!boardId || conflictRef.current) return;
    clearWindowTimer(saveTimerRef);
    void flushSceneSave(boardId);
  }, [activeBoardIdRef, flushSceneSave]);

  const keepLocalSceneOver = useCallback(
    (revision: number) => {
      const boardId = activeBoardIdRef.current;
      revisionRef.current = revision;
      setConflictState(false);
      if (!boardId) return;
      clearWindowTimer(saveTimerRef);
      void flushSceneSave(boardId);
    },
    [activeBoardIdRef, flushSceneSave, setConflictState],
  );

  const hasUnsavedScene = useCallback(
    () =>
      conflictRef.current ||
      saveInFlightRef.current ||
      pendingSceneRef.current !== null ||
      pendingLocalChangeRef.current !== null ||
      localChangeFlushTimerRef.current !== null,
    [],
  );

  const flushPendingSceneBeforeClose =
    useCallback(async (): Promise<boolean> => {
      const boardId = activeBoardIdRef.current;
      if (!boardId) return true;
      if (conflictRef.current) return false;

      const flushQueuedLocalChangeNow = () => {
        clearWindowTimer(localChangeFlushTimerRef);
        if (pendingLocalChangeRef.current !== null) {
          flushQueuedLocalChange();
        }
      };

      lastSaveFailedRef.current = false;
      flushQueuedLocalChangeNow();
      clearWindowTimer(saveTimerRef);

      const flushUntilSettled = async (attempt: number): Promise<boolean> => {
        for (;;) {
          if (conflictRef.current) return false;
          const step = resolveWhiteboardCloseFlushStep({
            attempt,
            lastSaveFailed: lastSaveFailedRef.current,
            localChangeTimerActive: localChangeFlushTimerRef.current !== null,
            pendingLocalChange: pendingLocalChangeRef.current !== null,
            pendingScene:
              pendingSceneRef.current !== null && !lastSaveFailedRef.current,
            saveInFlight: saveInFlightRef.current,
          });

          if (step === 'timed-out') {
            setError(translate('whiteboard.closeSaveIncomplete'));
            return false;
          }
          if (step === 'flush-local-change') {
            flushQueuedLocalChangeNow();
            clearWindowTimer(saveTimerRef);
            return flushUntilSettled(attempt + 1);
          }
          if (step === 'flush-scene-save') {
            clearWindowTimer(saveTimerRef);
            const saved = await flushSceneSave(boardId);
            if (!saved && lastSaveFailedRef.current) {
              return false;
            }
            return flushUntilSettled(attempt + 1);
          }
          if (step === 'settled-succeeded') {
            return true;
          }
          if (step === 'settled-failed') {
            return false;
          }
          break;
        }

        await waitFor(WHITEBOARD_CLOSE_FLUSH_RETRY_MS);
        return flushUntilSettled(attempt + 1);
      };

      return flushUntilSettled(0);
    }, [
      activeBoardIdRef,
      flushQueuedLocalChange,
      flushSceneSave,
      setError,
      translate,
    ]);

  return {
    conflict,
    flushPendingSceneBeforeClose,
    hasUnsavedScene,
    keepLocalSceneOver,
    queueLocalChange,
    rebaseline,
    resetLoadedScene,
    retrySave,
    saveStatus,
  };
}
