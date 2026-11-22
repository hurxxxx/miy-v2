// Ported from AI-DO apps/web/src/components/layout/useAppBarController.ts
// (0002af062): the launcher menu and pin editor state, closing on navigation
// and on a press outside, and the editor's reset and save flow. miy reads the
// categories from GET /api/v1/launcher/categories when the menu opens (AI-DO
// had them in its workspace bootstrap), saves the pins with
// PATCH /api/v1/me/preferences, and closes the menus on Escape.
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useSession, type User } from "../data/session";
import { launcherErrorKey, listLauncherCategories, savePinnedApps, type LauncherCategory } from "../data/launcher";
import {
  INITIAL_APP_BAR_STATE,
  appBarReducer,
  buildAppBarItemsProjection,
  isDefaultPinnedApps,
  resolveDefaultDraftPinnedAppIds,
  resolveEditorDraftPinnedAppIds,
  resolvePinnedAppIds,
  resolveSavablePinnedAppIds,
  type AppBarAppItem,
  type AppBarTranslator,
} from "./app-bar-model";

export type LauncherCategoriesState = {
  status: "idle" | "loading" | "ready" | "error";
  categories: LauncherCategory[] | null;
};

export function useAppBarController({
  currentPathname,
  user,
  visibleItems,
}: {
  currentPathname: string;
  user: User;
  visibleItems: AppBarAppItem[];
}) {
  const { t, i18n } = useTranslation(["shell", "launcher", "common", "portal"]);
  const translate = t as unknown as AppBarTranslator;
  const { updateUser } = useSession();
  const [state, dispatch] = useReducer(appBarReducer, INITIAL_APP_BAR_STATE);
  const [categories, setCategories] = useState<LauncherCategoriesState>({ status: "idle", categories: null });
  const launcherButtonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLDialogElement>(null);
  const previousPathnameRef = useRef(currentPathname);
  const savedPinnedApps = user.preferences?.pinnedApps;
  const anyOpen = state.launcherOpen || state.appBarEditorOpen;

  useEffect(() => {
    if (previousPathnameRef.current === currentPathname) {
      return;
    }
    previousPathnameRef.current = currentPathname;
    if (state.launcherOpen || state.appBarEditorOpen) {
      dispatch({ type: "closeLauncherMenus" });
    }
  }, [currentPathname, state.appBarEditorOpen, state.launcherOpen]);

  useEffect(() => {
    if (!anyOpen) {
      return;
    }

    function handlePointerDown(event: MouseEvent) {
      const target = event.target as Node;
      if (
        launcherButtonRef.current?.contains(target) ||
        menuRef.current?.contains(target) ||
        editorRef.current?.contains(target)
      ) {
        return;
      }
      dispatch({ type: "closeLauncherMenus" });
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      dispatch({ type: "closeLauncherMenus" });
      launcherButtonRef.current?.focus();
    }

    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [anyOpen]);

  // Each opening reads the current arrangement; the last good one stays shown
  // while it loads.
  useEffect(() => {
    if (!state.launcherOpen) return;
    const controller = new AbortController();
    setCategories((current) => ({ ...current, status: "loading" }));
    listLauncherCategories(controller.signal)
      .then((page) => {
        if (!controller.signal.aborted) setCategories({ status: "ready", categories: page.categories });
      })
      .catch(() => {
        if (!controller.signal.aborted) setCategories((current) => ({ ...current, status: "error" }));
      });
    return () => controller.abort();
  }, [state.launcherOpen]);

  const pinnedAppIds = useMemo(
    () => resolvePinnedAppIds(savedPinnedApps, visibleItems),
    [savedPinnedApps, visibleItems],
  );
  const itemProjection = useMemo(
    () =>
      buildAppBarItemsProjection({
        draftPinnedAppIds: state.draftPinnedAppIds,
        pinnedAppIds,
        visibleItems,
      }),
    [pinnedAppIds, state.draftPinnedAppIds, visibleItems],
  );
  const { draftItems, pinnedEligibleAppIds, pinnedItems } = itemProjection;

  const openAppBarEditor = useCallback(() => {
    const isDefault = isDefaultPinnedApps(savedPinnedApps);
    dispatch({
      type: "patch",
      patch: {
        appBarEditorOpen: true,
        appBarLayoutError: null,
        draftIsDefault: isDefault,
        draftPinnedAppIds: isDefault
          ? resolveDefaultDraftPinnedAppIds(visibleItems)
          : resolveEditorDraftPinnedAppIds(pinnedAppIds, pinnedEligibleAppIds),
        launcherOpen: false,
      },
    });
  }, [pinnedAppIds, pinnedEligibleAppIds, savedPinnedApps, visibleItems]);

  const resetDraftPinnedApps = useCallback(() => {
    dispatch({
      type: "patch",
      patch: {
        appBarLayoutError: null,
        draftIsDefault: true,
        draftPinnedAppIds: resolveDefaultDraftPinnedAppIds(visibleItems),
      },
    });
  }, [visibleItems]);

  const saveAppBarLayout = useCallback(async () => {
    // An empty list means "every app" to the server; unpinning everything is
    // refused here, and the default is chosen with reset.
    if (!state.draftIsDefault && !state.draftPinnedAppIds.some((appId) => pinnedEligibleAppIds.has(appId))) {
      dispatch({ type: "patch", patch: { appBarLayoutError: translate("shell:appBarEditor.requirePinned") } });
      return;
    }
    const nextPinnedAppIds = state.draftIsDefault
      ? []
      : resolveSavablePinnedAppIds(state.draftPinnedAppIds, pinnedEligibleAppIds, savedPinnedApps);

    dispatch({
      type: "patch",
      patch: { appBarLayoutError: null, appBarLayoutSaving: true },
    });
    try {
      updateUser(await savePinnedApps(user.csrf, nextPinnedAppIds));
      dispatch({
        type: "patch",
        patch: { appBarEditorOpen: false },
      });
      launcherButtonRef.current?.focus();
    } catch (caughtError) {
      const key = launcherErrorKey(caughtError);
      dispatch({
        type: "patch",
        patch: {
          appBarLayoutError:
            key && i18n.exists(key) ? translate(key) : translate("shell:appBarEditor.saveFailed"),
        },
      });
    } finally {
      dispatch({
        type: "patch",
        patch: { appBarLayoutSaving: false },
      });
    }
  }, [
    i18n,
    pinnedEligibleAppIds,
    savedPinnedApps,
    state.draftIsDefault,
    state.draftPinnedAppIds,
    translate,
    updateUser,
    user.csrf,
  ]);

  return {
    categories,
    draftItems,
    editorRef,
    launcherButtonRef,
    menuRef,
    onCloseEditor: () => {
      dispatch({ type: "patch", patch: { appBarEditorOpen: false } });
      launcherButtonRef.current?.focus();
    },
    onCloseLauncherMenus: () => dispatch({ type: "closeLauncherMenus" }),
    onMovePinnedApp: (appId: string, direction: -1 | 1) => dispatch({ type: "moveDraftPinned", appId, direction }),
    onOpenEditor: openAppBarEditor,
    onResetDraft: resetDraftPinnedApps,
    onSaveLayout: () => {
      void saveAppBarLayout();
    },
    onToggleLauncher: () => dispatch({ type: "toggleLauncher" }),
    onTogglePinnedApp: (appId: string, checked: boolean) => {
      dispatch({ type: "toggleDraftPinned", appId, checked });
    },
    pinnedEligibleAppIds,
    pinnedItems,
    state,
    t: translate,
  };
}
