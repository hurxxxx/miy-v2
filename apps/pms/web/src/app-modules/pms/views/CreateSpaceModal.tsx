import { useEffect, useMemo, useReducer, useRef } from 'react';
import { Layout, UserPlus } from 'lucide-react';
import { ContentState, FormMessage, useFeedback } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import { usePmsCurrentUser } from '../../../platform/auth/current-user';
import { hasWorkspaceMembership } from '../../../platform/auth/auth-api';
import { useOptionalAppRuntime } from '../../../platform/app-runtime';
import {
  FORM_FIELD_CONTROL_CLASS_NAME,
  FORM_TEXTAREA_CONTROL_CLASS_NAME,
  FormDialog,
  FormFieldRow,
} from '../../../components/form/FormDialog';
import { UserSearchMultiSelect } from '../../../platform/users/UserSearchMultiSelect';
import { selectUserOptionsForPicker } from '../../../platform/users/user-option-picker-model';
import {
  addSpaceMember,
  createSpace,
  listPmsUsers,
  type PmsSpace,
  type PmsUserSummary,
} from '../api/pms-api';
import { dispatchPmsSpaceChanged } from './pms-events';

interface CreateSpaceModalState {
  name: string;
  description: string;
  submitting: boolean;
  error: string;
  allUsers: PmsUserSummary[];
  picked: PmsUserSummary[];
  query: string;
  queryFocused: boolean;
}

type CreateSpaceModalAction =
  | { type: 'reset' }
  | { type: 'setName'; value: string }
  | { type: 'setDescription'; value: string }
  | { type: 'setQuery'; value: string }
  | { type: 'setQueryFocused'; value: boolean }
  | { type: 'usersLoaded'; users: PmsUserSummary[] }
  | { type: 'usersLoadFailed' }
  | { type: 'addMember'; user: PmsUserSummary }
  | { type: 'removeMember'; userId: string }
  | { type: 'createStarted' }
  | { type: 'createFailed'; message: string }
  | { type: 'createFinished' };

const INITIAL_CREATE_SPACE_MODAL_STATE: CreateSpaceModalState = {
  name: '',
  description: '',
  submitting: false,
  error: '',
  allUsers: [],
  picked: [],
  query: '',
  queryFocused: false,
};

function createSpaceModalReducer(
  state: CreateSpaceModalState,
  action: CreateSpaceModalAction,
): CreateSpaceModalState {
  switch (action.type) {
    case 'reset':
      return {
        ...INITIAL_CREATE_SPACE_MODAL_STATE,
        submitting: state.submitting,
      };
    case 'setName':
      return { ...state, name: action.value };
    case 'setDescription':
      return { ...state, description: action.value };
    case 'setQuery':
      return { ...state, query: action.value };
    case 'setQueryFocused':
      return { ...state, queryFocused: action.value };
    case 'usersLoaded':
      return { ...state, allUsers: action.users };
    case 'usersLoadFailed':
      return { ...state, allUsers: [] };
    case 'addMember':
      return {
        ...state,
        picked: state.picked.some((item) => item.id === action.user.id)
          ? state.picked
          : [...state.picked, action.user],
        query: '',
      };
    case 'removeMember':
      return {
        ...state,
        picked: state.picked.filter((user) => user.id !== action.userId),
      };
    case 'createStarted':
      return { ...state, submitting: true, error: '' };
    case 'createFailed':
      return { ...state, submitting: false, error: action.message };
    case 'createFinished':
      return { ...state, submitting: false };
    default:
      return state;
  }
}

export const CreateSpaceModal = ({
  isOpen,
  onClose,
  onCreated,
  workspaceSlug,
}: {
  isOpen: boolean;
  onClose: () => void;
  onCreated?: (space: PmsSpace) => void;
  workspaceSlug?: string | null;
}) => {
  const { t } = useTranslation(['apps', 'common']);
  const feedback = useFeedback();
  const api = useAppApi();
  const user = usePmsCurrentUser();
  const runtime = useOptionalAppRuntime();
  const pendingCreate = useRef<{ dismissed: boolean } | null>(null);
  const [
    {
      name,
      description,
      submitting,
      error,
      allUsers,
      picked,
      query,
      queryFocused,
    },
    dispatch,
  ] = useReducer(createSpaceModalReducer, INITIAL_CREATE_SPACE_MODAL_STATE);
  // miy: typed details, picked members or a create in flight are unsaved work.
  useNavigationDirty(
    isOpen &&
      (submitting ||
        name.trim() !== '' ||
        description.trim() !== '' ||
        picked.length > 0),
  );

  const canCreateSpace = runtime
    ? runtime.app.app_id === 'pms' && runtime.app.can_create_space
    : hasWorkspaceMembership(user);

  useEffect(() => {
    if (!isOpen) return;
    dispatch({ type: 'reset' });
    return () => {
      if (pendingCreate.current) pendingCreate.current.dismissed = true;
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen || !api || !canCreateSpace) return;
    let cancelled = false;
    listPmsUsers(api, workspaceSlug)
      .then((users) => {
        if (!cancelled) dispatch({ type: 'usersLoaded', users });
      })
      .catch(() => {
        if (!cancelled) dispatch({ type: 'usersLoadFailed' });
      });
    return () => {
      cancelled = true;
    };
  }, [isOpen, api, workspaceSlug, canCreateSpace]);

  const excludedCandidateIds = useMemo(
    () =>
      new Set([
        ...picked.map((pickedUser) => pickedUser.id),
        ...(user?.id ? [user.id] : []),
      ]),
    [picked, user?.id],
  );

  const candidates = useMemo(
    () =>
      selectUserOptionsForPicker<PmsUserSummary>({
        users: allUsers,
        query,
        excludeIds: excludedCandidateIds,
        limit: 6,
      }),
    [allUsers, excludedCandidateIds, query],
  );

  function addMember(member: PmsUserSummary) {
    dispatch({ type: 'addMember', user: member });
  }

  function removeMember(userId: string) {
    dispatch({ type: 'removeMember', userId });
  }

  function handleClose() {
    if (pendingCreate.current) pendingCreate.current.dismissed = true;
    dispatch({ type: 'reset' });
    onClose();
  }

  async function handleCreate() {
    if (!api || !name.trim() || !canCreateSpace || pendingCreate.current)
      return;
    const request = { dismissed: false };
    pendingCreate.current = request;
    dispatch({ type: 'createStarted' });
    let space: PmsSpace;
    try {
      space = await createSpace(
        api,
        {
          name: name.trim(),
          description: description.trim(),
        },
        workspaceSlug,
      );
    } catch (err) {
      pendingCreate.current = null;
      const message =
        err instanceof Error ? err.message : t('apps:pms.createSpaceFailed');
      if (request.dismissed) {
        dispatch({ type: 'createFinished' });
        feedback.error(message);
      } else {
        dispatch({ type: 'createFailed', message });
      }
      return;
    }

    if (user?.id) {
      dispatchPmsSpaceChanged({ type: 'created', space, creatorId: user.id });
    }
    const failures = (
      await Promise.all(
        picked.map(async (member) => {
          try {
            await addSpaceMember(
              api,
              space.id,
              {
                user_id: member.id,
                role: 'member',
              },
              runtime ? space.workspace_key : workspaceSlug,
            );
            return null;
          } catch (err) {
            const message =
              err instanceof Error
                ? err.message
                : t('apps:pms.memberInviteFailed');
            return `${member.full_name}: ${message}`;
          }
        }),
      )
    ).filter((failure): failure is string => failure !== null);

    pendingCreate.current = null;
    dispatch({ type: 'createFinished' });
    if (failures.length > 0) {
      feedback.warning(
        t('apps:pms.createSpacePartialFailure', {
          failures: failures.join(', '),
        }),
      );
    } else if (request.dismissed) {
      feedback.success(
        t('apps:admin.console.workspaces.created', { name: space.name }),
      );
    }
    if (!request.dismissed) {
      handleClose();
      onCreated?.(space);
    }
  }

  return (
    <FormDialog
      cancelLabel={t('common:actions.cancel')}
      closeLabel={t('common:actions.close')}
      open={isOpen}
      onCancel={handleClose}
      onPrimary={() => void handleCreate()}
      title={t('apps:pms.createSpace')}
      maxWidth="max-w-xl"
      dismissOnInteractOutside={false}
      primaryDisabled={!name.trim() || !canCreateSpace}
      primaryLabel={t('apps:pms.createSpace')}
      primaryPendingLabel={t('apps:pms.creating')}
      submitting={submitting}
    >
      <div className="space-y-5 text-app-ink">
        {!canCreateSpace ? (
          <ContentState
            kind="unavailable"
            size="compact"
            title={t('apps:pms.createSpaceNoAccess')}
          />
        ) : (
          <>
            <div className="flex items-center gap-3 p-4 rounded-lg bg-app-surface-sidebar border border-app-border">
              <div className="size-10 bg-app-accent/20 rounded-lg flex items-center justify-center">
                <Layout size={20} className="text-app-accent" />
              </div>
              <div className="app-text-body text-app-ink/60">
                {t('apps:pms.createSpaceDescription')}
              </div>
            </div>

            <FormMessage
              id="create-space-form-message"
              message={error}
              tone="error"
              variant="form"
            />

            <FormFieldRow
              htmlFor="create-space-name"
              label={t('apps:pms.spaceName')}
              required
            >
              <input
                id="create-space-name"
                aria-describedby="create-space-form-message"
                type="text"
                placeholder={t('apps:pms.spacePlaceholder')}
                value={name}
                onChange={(e) =>
                  dispatch({ type: 'setName', value: e.target.value })
                }
                onKeyDown={(e) => {
                  if (
                    e.key === 'Enter' &&
                    !e.nativeEvent.isComposing &&
                    name.trim() &&
                    !submitting
                  ) {
                    void handleCreate();
                  }
                }}
                className={FORM_FIELD_CONTROL_CLASS_NAME}
              />
            </FormFieldRow>

            <FormFieldRow
              htmlFor="create-space-description"
              label={t('apps:pms.description')}
              optionalLabel={t('apps:pms.optional')}
            >
              <textarea
                id="create-space-description"
                placeholder={t('apps:pms.spaceDescriptionPlaceholder')}
                value={description}
                onChange={(e) =>
                  dispatch({ type: 'setDescription', value: e.target.value })
                }
                rows={3}
                className={FORM_TEXTAREA_CONTROL_CLASS_NAME}
              />
            </FormFieldRow>

            <FormFieldRow
              htmlFor="create-space-member-query"
              label={t('apps:pms.inviteMembers')}
              optionalLabel={t('apps:pms.optional')}
            >
              <p className="app-text-caption text-app-ink/40">
                {t('apps:pms.inviteMembersHint')}
              </p>
              <UserSearchMultiSelect
                candidates={candidates}
                inputId="create-space-member-query"
                labels={{
                  noUserMatch: t('apps:pms.noMatchingUsers'),
                  removeItem: (memberName) =>
                    t('apps:pms.removeMember', { name: memberName }),
                  searchPlaceholder: t('apps:pms.searchUser'),
                  searchPrompt: t('apps:pms.noUsersToAdd'),
                  searching: t('apps:pms.searchUser'),
                }}
                onAddUser={addMember}
                onQueryChange={(value) => dispatch({ type: 'setQuery', value })}
                onQueryFocusChange={(value) =>
                  dispatch({ type: 'setQueryFocused', value })
                }
                onRemoveUser={removeMember}
                query={query}
                queryFocused={
                  queryFocused &&
                  (Boolean(query.trim()) || candidates.length > 0)
                }
                renderCandidateTrailing={() => (
                  <UserPlus size={14} className="shrink-0 text-app-accent" />
                )}
                selectedUsers={picked}
              />
            </FormFieldRow>
          </>
        )}
      </div>
    </FormDialog>
  );
};
