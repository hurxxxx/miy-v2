import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from 'react';
import { useNavigate } from '../../../platform/router';
import { AnimatePresence, LazyMotion, domAnimation, m } from 'motion/react';
import { ChevronDown, ChevronRight, Loader2, Plus } from 'lucide-react';
import { useFeedback } from '@miy/ui';
import { useConfirm } from '@miy/ui';
import { usePrompt } from '@miy/ui';
import { useTranslation } from 'react-i18next';

import { useAppApi, type AppApi } from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import { useOptionalAppRuntime } from '../../../platform/app-runtime';
import {
  hasWorkspaceMembership,
  teamRoleAllows,
} from '../../../platform/auth/auth-api';
import {
  listAllPmsTaskLists,
  listFolders,
  deletePmsTaskList,
  updateFolder,
  deleteFolder,
  listSpaces,
  updateSpace,
  deleteSpace,
  updatePmsTaskList,
  type PmsFolder,
  type PmsTaskList,
  type PmsSpace,
  type PmsSpacePreferenceChange,
} from '../api/pms-api';
import {
  applyFlatReorder,
  computeFlatDropTarget,
  type FlatDropZone,
} from '../api/pms-sidebar-reorder';
import {
  buildWorkspaceAppPath,
  resolveDefaultWorkspaceAppPath,
} from '../../../platform/workspaces/workspace-utils';
import { CreateTaskListModal } from '../views/CreateTaskListModal';
import { CreateSpaceModal } from '../views/CreateSpaceModal';
import { SpaceMembersModal } from '../views/SpaceMembersModal';
import { usePmsSpacePreferences } from '../views/usePmsSpacePreferences';
import { CreateFolderModal } from '../views/CreateFolderModal';
import {
  SPACE_COLORS,
  arrangeSidebarSpaces,
  buildPmsSidebarSpaceTree,
} from './space-tree-model';
import { SpaceItem } from './SpaceTree';
import { SpaceOrderEditorModal } from './SpaceOrderEditorModal';
import {
  buildSpaceOrderChanges,
  persistSpaceOrderChanges,
} from './space-order-persistence';
import {
  pmsSidebarSpaceKey,
  pmsSidebarSpacePath,
  pmsSidebarListPath,
} from './pms-sidebar-scope';
import {
  PMS_SPACE_CHANGED_EVENT,
  PMS_SPACE_ORDER_CHANGED_EVENT,
  PMS_TASK_LIST_CHANGED_EVENT,
  dispatchPmsSpaceChanged,
  dispatchPmsSpaceOrderChanged,
  dispatchPmsTaskListChanged,
  type PmsSpaceChangedDetail,
  type PmsSpaceOrderChangedDetail,
  type PmsTaskListChangedDetail,
} from '../views/pms-events';
import {
  hasArchivedPmsTaskListInFolder,
  listActivePmsTaskListsForSpace,
  reconcilePmsTaskListCatalog,
} from '../views/pms-task-list-catalog-model';

interface PmsSidebarSpacesProps {
  activeNavItemId: string;
  currentWorkspaceSlug: string | null;
  isExpanded: boolean;
  onToggle: () => void;
  onNavigate?: () => void;
}

function upsertList(lists: PmsTaskList[], item: PmsTaskList): PmsTaskList[] {
  return [item, ...lists.filter((current) => current.id !== item.id)].sort(
    (left, right) => right.updated_at.localeCompare(left.updated_at),
  );
}

function upsertSpace(
  spaces: PmsSpace[],
  space: PmsSpace,
  locale = 'ko-KR',
): PmsSpace[] {
  return [space, ...spaces.filter((item) => item.id !== space.id)].sort(
    (left, right) => left.name.localeCompare(right.name, locale),
  );
}

function getErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

async function listSidebarTaskLists(
  api: AppApi,
  workspaceSlug: string | null,
): Promise<PmsTaskList[]> {
  const [active, archived] = await Promise.all([
    listAllPmsTaskLists(api, undefined, workspaceSlug),
    listAllPmsTaskLists(api, undefined, workspaceSlug, {
      archived: true,
    }).catch(() => null),
  ]);
  return [...active.items, ...(archived?.items ?? [])];
}

export function PmsSidebarSpaces(props: PmsSidebarSpacesProps) {
  const runtime = useOptionalAppRuntime();
  const user = usePmsCurrentUser();
  return (
    <PmsSidebarSpacesContent
      key={`${user?.id ?? ''}:${runtime?.app.app_id ?? 'legacy'}:${runtime?.space.key ?? ''}:${props.currentWorkspaceSlug}:${runtime?.space.role ?? ''}`}
      {...props}
    />
  );
}

function PmsSidebarSpacesContent({
  activeNavItemId,
  currentWorkspaceSlug,
  isExpanded,
  onToggle,
  onNavigate,
}: PmsSidebarSpacesProps) {
  return usePmsSidebarSpacesElement({
    activeNavItemId,
    currentWorkspaceSlug,
    isExpanded,
    onToggle,
    onNavigate,
  });
}

interface PmsSidebarRuntimeState {
  pmsLists: PmsTaskList[];
  pmsFolders: PmsFolder[];
  pmsTeams: PmsSpace[];
  pmsLoading: boolean;
  collapsedSpaces: Set<string>;
}

type PmsSidebarRuntimeAction =
  | { type: 'loadStart' }
  | {
      type: 'loadComplete';
      lists: PmsTaskList[] | null;
      folders: PmsFolder[];
      teams: PmsSpace[];
    }
  | { type: 'setLists'; updater: (current: PmsTaskList[]) => PmsTaskList[] }
  | { type: 'setFolders'; updater: (current: PmsFolder[]) => PmsFolder[] }
  | { type: 'setTeams'; updater: (current: PmsSpace[]) => PmsSpace[] }
  | { type: 'toggleSpace'; spaceId: string }
  | { type: 'expandSpace'; spaceId: string }
  | { type: 'removeSpace'; spaceId: string };

const PMS_SIDEBAR_INITIAL_STATE: PmsSidebarRuntimeState = {
  pmsLists: [],
  pmsFolders: [],
  pmsTeams: [],
  pmsLoading: false,
  collapsedSpaces: new Set(),
};

function pmsSidebarRuntimeReducer(
  state: PmsSidebarRuntimeState,
  action: PmsSidebarRuntimeAction,
): PmsSidebarRuntimeState {
  switch (action.type) {
    case 'loadStart':
      return { ...PMS_SIDEBAR_INITIAL_STATE, pmsLoading: true };
    case 'loadComplete':
      return {
        ...state,
        pmsLists: action.lists ?? [],
        pmsFolders: action.folders,
        pmsTeams: action.teams,
        pmsLoading: false,
      };
    case 'setLists':
      return { ...state, pmsLists: action.updater(state.pmsLists) };
    case 'setFolders':
      return { ...state, pmsFolders: action.updater(state.pmsFolders) };
    case 'setTeams':
      return { ...state, pmsTeams: action.updater(state.pmsTeams) };
    case 'toggleSpace': {
      const collapsedSpaces = new Set(state.collapsedSpaces);
      if (collapsedSpaces.has(action.spaceId)) {
        collapsedSpaces.delete(action.spaceId);
      } else {
        collapsedSpaces.add(action.spaceId);
      }
      return { ...state, collapsedSpaces };
    }
    case 'expandSpace': {
      if (!state.collapsedSpaces.has(action.spaceId)) {
        return state;
      }
      const collapsedSpaces = new Set(state.collapsedSpaces);
      collapsedSpaces.delete(action.spaceId);
      return { ...state, collapsedSpaces };
    }
    case 'removeSpace': {
      const collapsedSpaces = new Set(state.collapsedSpaces);
      collapsedSpaces.delete(action.spaceId);
      return {
        ...state,
        pmsTeams: state.pmsTeams.filter((team) => team.id !== action.spaceId),
        pmsLists: state.pmsLists.filter(
          (list) => list.team_id !== action.spaceId,
        ),
        pmsFolders: state.pmsFolders.filter(
          (folder) => folder.team_id !== action.spaceId,
        ),
        collapsedSpaces,
      };
    }
    default:
      return state;
  }
}

function usePmsSidebarSpacesElement({
  activeNavItemId,
  currentWorkspaceSlug,
  isExpanded,
  onToggle,
  onNavigate,
}: PmsSidebarSpacesProps) {
  const routerNavigate = useNavigate();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const navigate = useCallback(
    (path: string) => {
      if (!mounted.current) return;
      routerNavigate(path);
      onNavigate?.();
    },
    [onNavigate, routerNavigate],
  );
  const { t, i18n } = useTranslation('apps');
  const locale = i18n.resolvedLanguage ?? i18n.language;
  const api = useAppApi();
  const user = usePmsCurrentUser();
  const runtime = useOptionalAppRuntime();
  const canonical = runtime !== null;
  const routeBase =
    runtime?.app.app_id === 'pms' ? runtime.app.route_base : null;
  const scopedSpaceId =
    runtime?.space.kind === 'app_space' ? runtime.space.id : null;
  const { confirm, confirmDialog } = useConfirm();
  const { prompt, promptDialog } = usePrompt();
  const feedback = useFeedback();
  const canReadTeams = Boolean(
    currentWorkspaceSlug &&
      (runtime
        ? runtime.app.app_id === 'pms' &&
          runtime.space.key === currentWorkspaceSlug
        : hasWorkspaceMembership(user, currentWorkspaceSlug)),
  );
  // The server decides who may create a space (any PMS user today); see can_create_space.
  const canWriteTeams =
    canReadTeams &&
    (runtime
      ? runtime.app.can_create_space
      : hasWorkspaceMembership(user, currentWorkspaceSlug));
  const pmsRootPath = routeBase ?? resolveDefaultWorkspaceAppPath(user, 'pms');
  const canManageSpace = useCallback(
    (team: PmsSpace) => teamRoleAllows(team.current_user_role, 'admin'),
    [],
  );

  const [state, dispatch] = useReducer(
    pmsSidebarRuntimeReducer,
    PMS_SIDEBAR_INITIAL_STATE,
  );
  const {
    pmsLists,
    pmsFolders,
    pmsTeams,
    pmsLoading,
    collapsedSpaces,
  } = state;
  const spaceWorkspaceKey = useCallback(
    (spaceId: string | null | undefined) =>
      pmsSidebarSpaceKey({
        canonical,
        currentWorkspaceSlug,
        spaceId,
        spaces: pmsTeams,
      }),
    [canonical, currentWorkspaceSlug, pmsTeams],
  );
  const listWorkspaceKey = useCallback(
    (listId: string) => {
      const list = pmsLists.find((item) => item.id === listId);
      const key = spaceWorkspaceKey(list?.team_id);
      return !canonical || list?.workspace_key === key ? key : null;
    },
    [canonical, pmsLists, spaceWorkspaceKey],
  );
  const folderWorkspaceKey = useCallback(
    (folderId: string) =>
      spaceWorkspaceKey(
        pmsFolders.find((item) => item.id === folderId)?.team_id,
      ),
    [pmsFolders, spaceWorkspaceKey],
  );
  const [createTaskListOpen, setCreateTaskListOpen] = useState(false);
  const [createTaskListTeamId, setCreateTaskListTeamId] = useState<
    string | null
  >(null);
  const [createTaskListFolderId, setCreateTaskListFolderId] = useState<
    string | null
  >(null);
  const [createSpaceOpen, setCreateSpaceOpen] = useState(false);
  const [manageMembersSpace, setManageMembersSpace] = useState<{
    id: string;
    name: string;
    canManage: boolean;
    currentUserRole: string | null;
  } | null>(null);
  const [orderEditorSpace, setOrderEditorSpace] = useState<{
    id: string;
    name: string;
  } | null>(null);
  const [createFolderOpen, setCreateFolderOpen] = useState(false);
  const [createFolderTeamId, setCreateFolderTeamId] = useState<string | null>(
    null,
  );

  useEffect(() => {
    let cancelled = false;
    if (!api || !canReadTeams) {
      dispatch({
        type: 'loadComplete',
        lists: [],
        folders: [],
        teams: [],
      });
      return undefined;
    }

    dispatch({ type: 'loadStart' });

    Promise.all([
      listSidebarTaskLists(api, currentWorkspaceSlug).catch(() => null),
      listFolders(api, undefined, currentWorkspaceSlug)
        .then((response) => response.items)
        .catch(() => []),
      canReadTeams
        ? listSpaces(api, currentWorkspaceSlug)
            .then((teams) => (Array.isArray(teams) ? teams : []))
            .catch(() => [])
        : Promise.resolve([]),
    ]).then(([lists, folders, teams]) => {
      if (cancelled) return;
      dispatch({
        type: 'loadComplete',
        lists,
        folders,
        teams,
      });
    });

    return () => {
      cancelled = true;
    };
  }, [canReadTeams, currentWorkspaceSlug, api]);

  const userId = user?.id;
  useEffect(() => {
    const handleSpaceChanged = (event: Event) => {
      const detail = (event as CustomEvent<PmsSpaceChangedDetail>).detail;
      if (!detail) return;
      if (detail.type === 'created') {
        if (!canReadTeams || scopedSpaceId || detail.creatorId !== userId)
          return;
      } else if (detail.type === 'updated') {
        if (
          canonical &&
          (scopedSpaceId
            ? detail.space.id !== scopedSpaceId
            : !pmsTeams.some((space) => space.id === detail.space.id))
        )
          return;
      } else {
        dispatch({ type: 'removeSpace', spaceId: detail.spaceId });
        return;
      }
      dispatch({
        type: 'setTeams',
        updater: (current) => upsertSpace(current, detail.space, locale),
      });
    };

    window.addEventListener(PMS_SPACE_CHANGED_EVENT, handleSpaceChanged);
    return () => {
      window.removeEventListener(PMS_SPACE_CHANGED_EVENT, handleSpaceChanged);
    };
  }, [canReadTeams, canonical, locale, pmsTeams, scopedSpaceId, userId]);

  useEffect(() => {
    const handleTaskListChanged = (event: Event) => {
      const detail = (event as CustomEvent<PmsTaskListChangedDetail>).detail;
      if (!detail) return;
      if (detail.type === 'updated') {
        if (
          canonical &&
          (!spaceWorkspaceKey(detail.taskList.team_id) ||
            detail.taskList.workspace_key !==
              spaceWorkspaceKey(detail.taskList.team_id))
        )
          return;
        dispatch({
          type: 'setLists',
          updater: (current) =>
            reconcilePmsTaskListCatalog(current, detail.taskList, 'all'),
        });
      } else {
        dispatch({
          type: 'setLists',
          updater: (current) =>
            current.filter((list) => list.id !== detail.taskListId),
        });
      }
    };

    window.addEventListener(PMS_TASK_LIST_CHANGED_EVENT, handleTaskListChanged);
    return () => {
      window.removeEventListener(
        PMS_TASK_LIST_CHANGED_EVENT,
        handleTaskListChanged,
      );
    };
  }, [canonical, spaceWorkspaceKey]);

  useEffect(() => {
    const handleSpaceOrderChanged = (event: Event) => {
      const detail = (event as CustomEvent<PmsSpaceOrderChangedDetail>).detail;
      if (!detail) return;
      const listChangeMap = new Map(
        detail.listChanges.map((item) => [item.id, item]),
      );

      dispatch({
        type: 'setLists',
        updater: (current) =>
          current.map((list) => {
            const change = listChangeMap.get(list.id);
            return change
              ? {
                  ...list,
                  folder_id: change.folder_id,
                  sort_order: change.sort_order,
                }
              : list;
          }),
      });
    };

    window.addEventListener(
      PMS_SPACE_ORDER_CHANGED_EVENT,
      handleSpaceOrderChanged,
    );
    return () => {
      window.removeEventListener(
        PMS_SPACE_ORDER_CHANGED_EVENT,
        handleSpaceOrderChanged,
      );
    };
  }, []);

  const openCreateTaskList = (teamId: string | null) => {
    setCreateTaskListTeamId(teamId);
    setCreateTaskListFolderId(null);
    setCreateTaskListOpen(true);
  };

  const openCreateFolder = (teamId: string) => {
    setCreateFolderTeamId(teamId);
    setCreateFolderOpen(true);
  };

  const toggleSpace = (spaceId: string) => {
    dispatch({ type: 'toggleSpace', spaceId });
  };

  const handleSaveSpaceOrder = useCallback(
    async (
      spaceId: string,
      payload: {
        lists: Array<{
          id: string;
          name: string;
          folder_id: string | null;
          sort_order: number;
          active_task_count: number;
        }>;
      },
    ) => {
      const workspaceKey = spaceWorkspaceKey(spaceId);
      if (!api || !workspaceKey) return;
      const currentLists = listActivePmsTaskListsForSpace(pmsLists, spaceId);
      const changes = buildSpaceOrderChanges({
        currentLists,
        payload,
      });
      const { listChanges } = changes;

      if (listChanges.length === 0) return;

      const nextListMap = new Map(payload.lists.map((item) => [item.id, item]));
      const listSnapshot = pmsLists;

      dispatch({
        type: 'setLists',
        updater: (current) =>
          current.map((list) => {
            const next = nextListMap.get(list.id);
            return next
              ? {
                  ...list,
                  folder_id: next.folder_id,
                  sort_order: next.sort_order,
                }
              : list;
          }),
      });

      try {
        await persistSpaceOrderChanges({
          workspaceSlug: workspaceKey,
          changes,
          spaceId,
          api,
        });
        dispatchPmsSpaceOrderChanged({ spaceId, ...changes });
      } catch (error) {
        dispatch({ type: 'setLists', updater: () => listSnapshot });
        throw new Error(
          getErrorMessage(error, t('pms.orderEditor.saveFailed')),
        );
      }
    },
    [pmsLists, api, t, spaceWorkspaceKey],
  );

  const handleRenameFolder = useCallback(
    async (folderId: string, currentName: string) => {
      const workspaceKey = folderWorkspaceKey(folderId);
      if (!api || !workspaceKey) return;
      const newName = await prompt({
        title: t('pms.sidebar.renameFolder'),
        defaultValue: currentName,
        placeholder: t('pms.folderName'),
        submitLabel: t('common:actions.save'),
        cancelLabel: t('common:actions.cancel'),
      });
      if (!newName || newName === currentName || !mounted.current) return;
      try {
        const updated = await updateFolder(
          api,
          folderId,
          { name: newName },
          workspaceKey,
        );
        if (!mounted.current) return;
        dispatch({
          type: 'setFolders',
          updater: (current) =>
            current.map((folder) =>
              folder.id === updated.id ? updated : folder,
            ),
        });
      } catch {
        /* ignore */
      }
    },
    [prompt, api, t, folderWorkspaceKey],
  );

  const handleRenameTaskList = useCallback(
    async (listId: string, currentName: string) => {
      const workspaceKey = listWorkspaceKey(listId);
      if (!api || !workspaceKey) return;
      const newName = await prompt({
        title: t('pms.sidebar.renameList'),
        defaultValue: currentName,
        placeholder: t('pms.listName'),
        submitLabel: t('common:actions.save'),
        cancelLabel: t('common:actions.cancel'),
      });
      const trimmedName = newName?.trim() ?? '';
      if (!trimmedName || trimmedName === currentName || !mounted.current)
        return;
      try {
        const updated = await updatePmsTaskList(
          api,
          listId,
          {
            name: trimmedName,
          },
          workspaceKey,
        );
        if (!mounted.current) return;
        dispatch({
          type: 'setLists',
          updater: (current) =>
            current.map((list) => (list.id === updated.id ? updated : list)),
        });
        dispatchPmsTaskListChanged({ type: 'updated', taskList: updated });
      } catch (error) {
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.renameListFailed')),
        );
      }
    },
    [feedback, prompt, api, t, listWorkspaceKey],
  );

  const handleDeleteTaskList = useCallback(
    async (listId: string, currentName: string) => {
      const workspaceKey = listWorkspaceKey(listId);
      if (!api || !workspaceKey) return;
      if (
        !(await confirm({
          title: t('pms.sidebar.deleteList'),
          description: t('pms.sidebar.deleteListDescription', {
            name: currentName,
          }),
          confirmLabel: t('common:actions.delete'),
          cancelLabel: t('common:actions.cancel'),
          variant: 'danger',
        }))
      )
        return;
      if (!mounted.current) return;
      try {
        await deletePmsTaskList(api, listId, workspaceKey);
        if (!mounted.current) return;
        dispatch({
          type: 'setLists',
          updater: (current) => current.filter((list) => list.id !== listId),
        });
        dispatchPmsTaskListChanged({ type: 'deleted', taskListId: listId });
        if (activeNavItemId === `pms-list-${listId}`) {
          navigate(
            routeBase
              ? pmsRootPath
              : buildWorkspaceAppPath(workspaceKey, 'pms'),
          );
        }
      } catch (error) {
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.deleteListFailed')),
        );
      }
    },
    [
      activeNavItemId,
      confirm,
      listWorkspaceKey,
      routeBase,
      feedback,
      navigate,
      pmsRootPath,
      api,
      t,
    ],
  );

  const handleArchiveTaskList = useCallback(
    async (listId: string, currentName: string, spaceId: string | null) => {
      const workspaceKey = listWorkspaceKey(listId);
      if (!api || !workspaceKey) return;
      const confirmed = await confirm({
        title: t('pms.archive.confirmTitle'),
        description: t('pms.archive.confirmDescription', {
          name: currentName,
        }),
        confirmLabel: t('pms.archive.action'),
        cancelLabel: t('common:actions.cancel'),
      });
      if (!confirmed || !mounted.current) return;
      try {
        const updated = await updatePmsTaskList(
          api,
          listId,
          {
            archived: true,
          },
          workspaceKey,
        );
        if (!mounted.current) return;
        dispatch({
          type: 'setLists',
          updater: (current) =>
            reconcilePmsTaskListCatalog(current, updated, 'all'),
        });
        dispatchPmsTaskListChanged({ type: 'updated', taskList: updated });
        feedback.success(
          t('pms.archive.archivedFeedback', { name: currentName }),
        );
        if (activeNavItemId === `pms-list-${listId}`) {
          navigate(
            spaceId
              ? pmsSidebarSpacePath(routeBase, workspaceKey, spaceId)
              : routeBase
                ? pmsRootPath
                : buildWorkspaceAppPath(workspaceKey, 'pms'),
          );
        }
      } catch (error) {
        feedback.error(getErrorMessage(error, t('pms.archive.archiveFailed')));
      }
    },
    [
      activeNavItemId,
      confirm,
      listWorkspaceKey,
      routeBase,
      feedback,
      navigate,
      pmsRootPath,
      t,
      api,
    ],
  );

  const handleRestoreTaskList = useCallback(
    async (listId: string, currentName: string) => {
      const workspaceKey = listWorkspaceKey(listId);
      if (!api || !workspaceKey) return;
      try {
        const updated = await updatePmsTaskList(
          api,
          listId,
          {
            archived: false,
          },
          workspaceKey,
        );
        if (!mounted.current) return;
        dispatch({
          type: 'setLists',
          updater: (current) =>
            reconcilePmsTaskListCatalog(current, updated, 'all'),
        });
        dispatchPmsTaskListChanged({ type: 'updated', taskList: updated });
        feedback.success(
          t('pms.archive.restoredFeedback', { name: currentName }),
        );
      } catch (error) {
        feedback.error(getErrorMessage(error, t('pms.archive.restoreFailed')));
      }
    },
    [feedback, t, api, listWorkspaceKey],
  );

  const handleReorderList = useCallback(
    async (
      spaceId: string,
      activeListId: string,
      overListId: string,
      zone: FlatDropZone,
    ) => {
      const workspaceKey = spaceWorkspaceKey(spaceId);
      if (!api || !workspaceKey) return;
      const listsInSpace = listActivePmsTaskListsForSpace(pmsLists, spaceId);
      const items = listsInSpace.map((list) => ({
        id: list.id,
        parent_id: list.folder_id ?? null,
        sort_order: list.sort_order,
        name: list.name,
      }));
      const target = computeFlatDropTarget(items, overListId, zone);
      if (!target) return;
      const result = applyFlatReorder(items, activeListId, target);
      if (!result) return;
      const patchMap = new Map(result.nextItems.map((item) => [item.id, item]));
      const snapshot = pmsLists;
      dispatch({
        type: 'setLists',
        updater: (current) =>
          current.map((list) => {
            const next = patchMap.get(list.id);
            if (!next) return list;
            return {
              ...list,
              sort_order: next.sort_order,
              folder_id: next.parent_id,
            };
          }),
      });
      try {
        await Promise.all(
          result.patches.map((patch) =>
            updatePmsTaskList(
              api,
              patch.id,
              {
                folder_id: patch.parent_id,
                sort_order: patch.sort_order,
              },
              workspaceKey,
            ),
          ),
        );
      } catch (error) {
        dispatch({ type: 'setLists', updater: () => snapshot });
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.listOrderFailed')),
        );
      }
    },
    [feedback, pmsLists, api, t, spaceWorkspaceKey],
  );

  const handleMoveFolder = useCallback(
    async (spaceId: string, folderId: string, direction: 'up' | 'down') => {
      const workspaceKey = spaceWorkspaceKey(spaceId);
      if (!api || !workspaceKey) return;
      const orderedFolders = pmsFolders.filter(
        (folder) => folder.team_id === spaceId,
      );
      orderedFolders.sort(
        (left, right) =>
          left.sort_order - right.sort_order ||
          left.name.localeCompare(right.name, locale),
      );
      const currentIndex = orderedFolders.findIndex(
        (folder) => folder.id === folderId,
      );
      if (currentIndex < 0) return;

      const targetIndex =
        direction === 'up' ? currentIndex - 1 : currentIndex + 1;
      if (targetIndex < 0 || targetIndex >= orderedFolders.length) return;

      const nextOrder = [...orderedFolders];
      const [movedFolder] = nextOrder.splice(currentIndex, 1);
      nextOrder.splice(targetIndex, 0, movedFolder);

      const updates = nextOrder.flatMap((folder, index) =>
        folder.sort_order === index ? [] : [{ folder, sort_order: index }],
      );

      if (updates.length === 0) return;

      try {
        const updatedFolders = new Map(
          (
            await Promise.all(
              updates.map(({ folder, sort_order }) =>
                updateFolder(api, folder.id, { sort_order }, workspaceKey),
              ),
            )
          ).map((updated) => [updated.id, updated] as const),
        );
        dispatch({
          type: 'setFolders',
          updater: (current) =>
            current.map((folder) => updatedFolders.get(folder.id) ?? folder),
        });
      } catch (error) {
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.folderOrderFailed')),
        );
      }
    },
    [feedback, locale, pmsFolders, api, t, spaceWorkspaceKey],
  );

  const handleDeleteFolder = useCallback(
    async (folderId: string) => {
      const workspaceKey = folderWorkspaceKey(folderId);
      if (!api || !workspaceKey) return;
      if (hasArchivedPmsTaskListInFolder(pmsLists, folderId)) {
        feedback.error(t('pms.archive.folderDeleteBlocked'));
        return;
      }
      if (
        !(await confirm({
          title: t('pms.sidebar.deleteFolder'),
          description: t('pms.sidebar.deleteFolderDescription'),
          confirmLabel: t('common:actions.delete'),
          cancelLabel: t('common:actions.cancel'),
          variant: 'danger',
        }))
      )
        return;
      if (!mounted.current) return;
      try {
        await deleteFolder(api, folderId, workspaceKey);
        dispatch({
          type: 'setFolders',
          updater: (current) =>
            current.filter((folder) => folder.id !== folderId),
        });
        listSidebarTaskLists(api, currentWorkspaceSlug)
          .then((lists) => dispatch({ type: 'setLists', updater: () => lists }))
          .catch(() => undefined);
      } catch (error) {
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.deleteFolderFailed')),
        );
      }
    },
    [
      confirm,
      currentWorkspaceSlug,
      feedback,
      folderWorkspaceKey,
      pmsLists,
      t,
      api,
    ],
  );

  const handleRenameSpace = useCallback(
    async (spaceId: string, newName: string) => {
      const workspaceKey = spaceWorkspaceKey(spaceId);
      if (!api || !workspaceKey) return;
      try {
        const updated = await updateSpace(
          api,
          spaceId,
          { name: newName },
          workspaceKey,
        );
        if (!mounted.current) return;
        dispatch({
          type: 'setTeams',
          updater: (current) =>
            current.map((team) => (team.id === updated.id ? updated : team)),
        });
        dispatchPmsSpaceChanged({ type: 'updated', space: updated });
      } catch {
        /* ignore */
      }
    },
    [api, spaceWorkspaceKey],
  );

  // A space the user deleted or left can no longer be shown; move off any
  // route inside it (the space or one of its lists).
  const navigateAwayFromSpace = useCallback(
    (spaceId: string) => {
      const activeListInSpace = pmsLists.some(
        (list) =>
          list.team_id === spaceId && activeNavItemId === `pms-list-${list.id}`,
      );
      const activeSpaceRoute = activeNavItemId === `pms-space-${spaceId}`;
      if (activeSpaceRoute || activeListInSpace) {
        navigate(
          routeBase || !currentWorkspaceSlug
            ? pmsRootPath
            : buildWorkspaceAppPath(currentWorkspaceSlug, 'pms'),
        );
      }
    },
    [
      activeNavItemId,
      currentWorkspaceSlug,
      navigate,
      pmsLists,
      pmsRootPath,
      routeBase,
    ],
  );

  const handleDeleteSpace = useCallback(
    async (spaceId: string) => {
      const workspaceKey = spaceWorkspaceKey(spaceId);
      if (!api || !workspaceKey) return;
      if (
        !(await confirm({
          title: t('pms.sidebar.deleteSpace'),
          description: t('pms.sidebar.deleteSpaceDescription'),
          confirmLabel: t('common:actions.archive'),
          cancelLabel: t('common:actions.cancel'),
          variant: 'danger',
        }))
      )
        return;
      if (!mounted.current) return;
      try {
        await deleteSpace(api, spaceId, workspaceKey);
        if (!mounted.current) return;
        dispatch({ type: 'removeSpace', spaceId });
        dispatchPmsSpaceChanged({ type: 'deleted', spaceId });
        navigateAwayFromSpace(spaceId);
      } catch (error) {
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.deleteSpaceFailed')),
        );
      }
    },
    [confirm, feedback, navigateAwayFromSpace, api, t, spaceWorkspaceKey],
  );

  const groupedSpaces = useMemo(() => {
    return buildPmsSidebarSpaceTree({
      folders: canonical
        ? pmsFolders.filter((folder) => spaceWorkspaceKey(folder.team_id))
        : pmsFolders,
      lists: canonical
        ? pmsLists.filter(
            (list) =>
              list.workspace_key &&
              list.workspace_key === spaceWorkspaceKey(list.team_id),
          )
        : pmsLists,
      locale,
      spaces: pmsTeams,
      untitledSpaceName: t('pms.sidebar.untitledSpace'),
    });
  }, [canonical, locale, pmsFolders, pmsLists, pmsTeams, spaceWorkspaceKey, t]);

  const { preferences: spacePreferences, setSpacePreference } =
    usePmsSpacePreferences(currentWorkspaceSlug);
  const [hiddenSpacesOpen, setHiddenSpacesOpen] = useState(false);
  const arrangedSpaces = useMemo(
    () => arrangeSidebarSpaces(groupedSpaces, spacePreferences),
    [groupedSpaces, spacePreferences],
  );
  const changeSpacePreference = useCallback(
    (spaceId: string, change: PmsSpacePreferenceChange) => {
      setSpacePreference(spaceId, change).catch((error: unknown) => {
        feedback.error(
          getErrorMessage(error, t('pms.sidebar.spacePreferenceFailed')),
        );
      });
    },
    [feedback, setSpacePreference, t],
  );

  const renderSpace = (space: (typeof groupedSpaces)[number]) => {
    const workspaceKey = spaceWorkspaceKey(space.id);
    if (!workspaceKey) return null;
    // Colors follow the name order, so pinning or hiding never repaints a space.
    const index = groupedSpaces.indexOf(space);
    const preference = spacePreferences.get(space.id);

    const spaceCanCreate = teamRoleAllows(space.current_user_role, 'member');
    const spaceCanManage = canManageSpace(space);
    return (
      <SpaceItem
        key={space.id}
        pinned={preference?.pinned ?? false}
        hidden={preference?.hidden ?? false}
        onTogglePinned={() =>
          changeSpacePreference(space.id, {
            pinned: !preference?.pinned,
          })
        }
        onToggleHidden={() =>
          changeSpacePreference(space.id, {
            hidden: !preference?.hidden,
          })
        }
        spaceId={space.id}
        name={space.name}
        iconColor={SPACE_COLORS[index % SPACE_COLORS.length]}
        rootLists={space.rootLists}
        archivedLists={space.archivedLists}
        folders={space.folders}
        expanded={!collapsedSpaces.has(space.id)}
        onToggle={() => toggleSpace(space.id)}
        onNavigate={() =>
          navigate(pmsSidebarSpacePath(routeBase, workspaceKey, space.id))
        }
        onAfterNavigate={onNavigate}
        onAddList={() => openCreateTaskList(space.id)}
        onAddListToFolder={(folderId) => {
          setCreateTaskListTeamId(space.id);
          setCreateTaskListFolderId(folderId);
          setCreateTaskListOpen(true);
        }}
        onAddFolder={() => openCreateFolder(space.id)}
        onMoveFolder={(folderId, direction) => {
          void handleMoveFolder(space.id, folderId, direction);
        }}
        onRenameFolder={(folderId, currentName) => {
          void handleRenameFolder(folderId, currentName);
        }}
        onDeleteFolder={(folderId) => {
          void handleDeleteFolder(folderId);
        }}
        onRenameList={(listId, currentName) => {
          void handleRenameTaskList(listId, currentName);
        }}
        onDeleteList={(listId, currentName) => {
          void handleDeleteTaskList(listId, currentName);
        }}
        onArchiveList={(listId, currentName) => {
          void handleArchiveTaskList(listId, currentName, space.id);
        }}
        onRestoreList={(listId, currentName) => {
          void handleRestoreTaskList(listId, currentName);
        }}
        onOpenListSettings={(listId) => {
          const listKey = listWorkspaceKey(listId);
          if (listKey)
            navigate(pmsSidebarListPath(routeBase, listKey, listId, true));
        }}
        onRenameSpace={(newName) => {
          void handleRenameSpace(space.id, newName);
        }}
        onDeleteSpace={() => {
          void handleDeleteSpace(space.id);
        }}
        onManageMembers={() => {
          setManageMembersSpace({
            id: space.id,
            name: space.name,
            canManage: spaceCanManage,
            currentUserRole: space.current_user_role,
          });
        }}
        onOpenOrderEditor={() => {
          setOrderEditorSpace({
            id: space.id,
            name: space.name,
          });
        }}
        onReorderList={(activeListId, overListId, zone) =>
          handleReorderList(space.id, activeListId, overListId, zone)
        }
        activeNavItemId={activeNavItemId}
        currentWorkspaceSlug={workspaceKey}
        canCreateSpaceContent={spaceCanCreate}
        canManageSpace={spaceCanManage}
      />
    );
  };

  return (
    <>
      {confirmDialog}
      {promptDialog}
      <SpaceOrderEditorModal
        isOpen={orderEditorSpace !== null}
        onClose={() => setOrderEditorSpace(null)}
        spaceName={orderEditorSpace?.name ?? ''}
        folders={pmsFolders.filter(
          (folder) => folder.team_id === orderEditorSpace?.id,
        )}
        lists={listActivePmsTaskListsForSpace(pmsLists, orderEditorSpace?.id)}
        onSave={async (payload) => {
          if (!orderEditorSpace) return;
          await handleSaveSpaceOrder(orderEditorSpace.id, payload);
        }}
      />
      <div className="space-y-1">
        <div className="w-full flex items-center justify-between px-3 py-1">
          <button
            type="button"
            onClick={onToggle}
            className="sidebar-section-label sidebar-section-header group/section flex items-center gap-1"
          >
            {isExpanded ? (
              <ChevronDown
                size={11}
                className="text-app-ink/55 dark:text-app-ink/65 transition-colors group-hover/section:text-app-ink dark:group-hover/section:text-white"
              />
            ) : (
              <ChevronRight
                size={11}
                className="text-app-ink/55 dark:text-app-ink/65 transition-colors group-hover/section:text-app-ink dark:group-hover/section:text-white"
              />
            )}
            <span>{t('pms.spaces')}</span>
          </button>
        </div>

        <LazyMotion features={domAnimation}>
          <AnimatePresence initial={false}>
            {isExpanded && (
              <m.div
                initial={{ height: 0, opacity: 0 }}
                animate={{ height: 'auto', opacity: 1 }}
                exit={{ height: 0, opacity: 0 }}
                className="overflow-hidden space-y-1"
              >
                {pmsLoading ? (
                  <div className="flex items-center justify-center py-4">
                    <Loader2
                      size={14}
                      className="animate-spin text-app-ink/55 dark:text-app-ink/65"
                    />
                  </div>
                ) : (
                  <>
                    {arrangedSpaces.visible.map(renderSpace)}
                    {arrangedSpaces.hidden.length > 0 ? (
                      <div>
                        <button
                          aria-expanded={hiddenSpacesOpen}
                          className="sidebar-submenu-item ml-1 w-full"
                          onClick={() => setHiddenSpacesOpen((open) => !open)}
                          type="button"
                        >
                          {hiddenSpacesOpen ? (
                            <ChevronDown size={13} />
                          ) : (
                            <ChevronRight size={13} />
                          )}
                          <span className="sidebar-submenu-label">
                            {t('pms.sidebar.hiddenSpaces', {
                              count: arrangedSpaces.hidden.length,
                            })}
                          </span>
                        </button>
                        {hiddenSpacesOpen
                          ? arrangedSpaces.hidden.map(renderSpace)
                          : null}
                      </div>
                    ) : null}

                    {canWriteTeams && (
                      <button
                        type="button"
                        onClick={() => setCreateSpaceOpen(true)}
                        className="sidebar-submenu-item ml-1 w-full"
                      >
                        <Plus size={13} />
                        <span className="sidebar-submenu-label">
                          {t('pms.sidebar.newSpace')}
                        </span>
                      </button>
                    )}
                  </>
                )}
              </m.div>
            )}
          </AnimatePresence>
        </LazyMotion>
      </div>

      <CreateTaskListModal
        workspaceSlug={spaceWorkspaceKey(createTaskListTeamId)}
        isOpen={createTaskListOpen}
        onClose={() => setCreateTaskListOpen(false)}
        teamId={createTaskListTeamId}
        folderId={createTaskListFolderId}
        onCreated={(list) => {
          if (!mounted.current) return;
          const workspaceKey = spaceWorkspaceKey(list.team_id);
          if (
            !workspaceKey ||
            (canonical && list.workspace_key !== workspaceKey)
          )
            return;
          dispatch({
            type: 'setLists',
            updater: (current) => upsertList(current, list),
          });
          const teamId = list.team_id;
          if (teamId) {
            dispatch({ type: 'expandSpace', spaceId: teamId });
          }
          navigate(pmsSidebarListPath(routeBase, workspaceKey, list.id));
        }}
      />

      <CreateSpaceModal
        isOpen={createSpaceOpen}
        onClose={() => setCreateSpaceOpen(false)}
        workspaceSlug={currentWorkspaceSlug}
        onCreated={(space) => {
          if (!mounted.current) return;
          if (routeBase) {
            const key = pmsSidebarSpaceKey({
              canonical: true,
              currentWorkspaceSlug: null,
              spaces: [space],
              spaceId: space.id,
            });
            if (key) navigate(pmsSidebarSpacePath(routeBase, key, space.id));
            return;
          }
          dispatch({
            type: 'setTeams',
            updater: (current) => upsertSpace(current, space, locale),
          });
          dispatch({ type: 'expandSpace', spaceId: space.id });
        }}
      />

      <SpaceMembersModal
        isOpen={manageMembersSpace !== null}
        onClose={() => setManageMembersSpace(null)}
        spaceId={manageMembersSpace?.id ?? null}
        spaceName={manageMembersSpace?.name ?? ''}
        workspaceSlug={spaceWorkspaceKey(manageMembersSpace?.id)}
        canManage={manageMembersSpace?.canManage ?? false}
        currentUserRole={manageMembersSpace?.currentUserRole ?? null}
        onChanged={() => {
          if (api) {
            listSpaces(api, currentWorkspaceSlug)
              .then((teams) => {
                if (mounted.current && Array.isArray(teams)) {
                  dispatch({ type: 'setTeams', updater: () => teams });
                }
              })
              .catch(() => undefined);
          }
        }}
        onLeft={() => {
          // The space-changed event already took it out of the tree.
          if (manageMembersSpace) navigateAwayFromSpace(manageMembersSpace.id);
        }}
      />

      {createFolderTeamId && (
        <CreateFolderModal
          workspaceSlug={spaceWorkspaceKey(createFolderTeamId)}
          isOpen={createFolderOpen}
          onClose={() => setCreateFolderOpen(false)}
          teamId={createFolderTeamId}
          onCreated={(folder) => {
            if (!mounted.current) return;
            dispatch({
              type: 'setFolders',
              updater: (current) => [...current, folder],
            });
            if (api) {
              listSidebarTaskLists(api, currentWorkspaceSlug)
                .then((lists) =>
                  dispatch({ type: 'setLists', updater: () => lists }),
                )
                .catch(() => undefined);
            }
          }}
        />
      )}
    </>
  );
}
