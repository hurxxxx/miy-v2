import { useId, useReducer } from 'react';
import { FormMessage } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import {
  FORM_FIELD_CONTROL_CLASS_NAME,
  FORM_TEXTAREA_CONTROL_CLASS_NAME,
  FormDialog,
  FormFieldRow,
} from '../../../components/form/FormDialog';
import { createPmsTaskList, type PmsTaskList } from '../api/pms-api';
import {
  canSubmitCreateName,
  createTaskListPayload,
  INITIAL_PMS_CREATE_MODAL_STATE,
  pmsCreateModalReducer,
} from './pms-create-modal-model';

export const CreateTaskListModal = ({
  workspaceSlug,
  isOpen,
  onClose,
  teamId = null,
  folderId = null,
  onCreated,
}: {
  workspaceSlug?: string | null;
  isOpen: boolean;
  onClose: () => void;
  teamId?: string | null;
  folderId?: string | null;
  onCreated?: (taskList: PmsTaskList) => void;
}) => {
  const { t } = useTranslation(['apps', 'common']);
  const api = useAppApi();
  const nameInputId = useId();
  const descriptionInputId = useId();
  const [{ description, error, name, submitting }, dispatch] = useReducer(
    pmsCreateModalReducer,
    INITIAL_PMS_CREATE_MODAL_STATE,
  );
  // miy: a typed name or a create in flight is unsaved work.
  useNavigationDirty(isOpen && (submitting || name.trim() !== ''));

  function handleClose() {
    dispatch({ type: 'reset' });
    onClose();
  }

  async function handleCreate() {
    if (!api || !canSubmitCreateName(name)) return;
    dispatch({ type: 'submit' });
    try {
      const taskList = await createPmsTaskList(
        api,
        createTaskListPayload({ name, description, teamId, folderId }),
        workspaceSlug,
      );
      onCreated?.(taskList);
      handleClose();
    } catch (err) {
      dispatch({
        type: 'failed',
        message:
          err instanceof Error ? err.message : t('apps:pms.createListFailed'),
      });
    } finally {
      dispatch({ type: 'finished' });
    }
  }

  return (
    <FormDialog
      cancelLabel={t('common:actions.cancel')}
      closeLabel={t('common:actions.close')}
      open={isOpen}
      onCancel={handleClose}
      onPrimary={() => void handleCreate()}
      title={t('apps:pms.createList')}
      maxWidth="max-w-lg"
      primaryDisabled={!canSubmitCreateName(name)}
      primaryLabel={t('apps:pms.create')}
      primaryPendingLabel={t('apps:pms.creating')}
      submitting={submitting}
    >
      <div className="space-y-5 text-app-ink">
        <div className="app-text-body text-app-ink/60">
          {t('apps:pms.createListDescription')}
        </div>

        <FormMessage
          id={`${nameInputId}-form-message`}
          message={error}
          tone="error"
          variant="form"
        />

        <FormFieldRow htmlFor={nameInputId} label={t('apps:pms.name')} required>
          <input
            id={nameInputId}
            aria-describedby={`${nameInputId}-form-message`}
            type="text"
            placeholder={t('apps:pms.listNamePlaceholder')}
            value={name}
            onChange={(event) =>
              dispatch({
                type: 'name',
                value: event.target.value,
              })
            }
            onKeyDown={(event) => {
              if (
                event.key === 'Enter' &&
                !event.nativeEvent.isComposing &&
                canSubmitCreateName(name) &&
                !submitting
              ) {
                void handleCreate();
              }
            }}
            className={FORM_FIELD_CONTROL_CLASS_NAME}
          />
        </FormFieldRow>

        <FormFieldRow
          htmlFor={descriptionInputId}
          label={t('apps:pms.description')}
          optionalLabel={t('apps:pms.optional')}
        >
          <textarea
            id={descriptionInputId}
            placeholder={t('apps:pms.listDescriptionPlaceholder')}
            value={description}
            onChange={(event) =>
              dispatch({
                type: 'description',
                value: event.target.value,
              })
            }
            rows={3}
            className={FORM_TEXTAREA_CONTROL_CLASS_NAME}
          />
        </FormFieldRow>
      </div>
    </FormDialog>
  );
};
