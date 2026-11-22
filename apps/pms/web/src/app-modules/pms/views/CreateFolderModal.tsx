import { useId, useReducer } from 'react';
import { FolderOpen } from 'lucide-react';
import { FormMessage } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import {
  FORM_FIELD_CONTROL_CLASS_NAME,
  FormDialog,
  FormFieldRow,
} from '../../../components/form/FormDialog';
import { createFolder, type PmsFolder } from '../api/pms-api';
import {
  canSubmitCreateName,
  createFolderPayload,
  INITIAL_PMS_CREATE_MODAL_STATE,
  pmsCreateModalReducer,
} from './pms-create-modal-model';

export const CreateFolderModal = ({
  workspaceSlug,
  isOpen,
  onClose,
  teamId,
  onCreated,
}: {
  workspaceSlug?: string | null;
  isOpen: boolean;
  onClose: () => void;
  teamId: string;
  onCreated?: (folder: PmsFolder) => void;
}) => {
  const { t } = useTranslation(['apps', 'common']);
  const api = useAppApi();
  const nameInputId = useId();
  const [{ error, name, submitting }, dispatch] = useReducer(
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
      const folder = await createFolder(
        api,
        createFolderPayload({ name, teamId }),
        workspaceSlug,
      );
      onCreated?.(folder);
      handleClose();
    } catch (err) {
      dispatch({
        type: 'failed',
        message:
          err instanceof Error ? err.message : t('apps:pms.createFolderFailed'),
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
      title={t('apps:pms.createFolder')}
      maxWidth="max-w-lg"
      primaryDisabled={!canSubmitCreateName(name)}
      primaryLabel={t('apps:pms.createFolder')}
      primaryPendingLabel={t('apps:pms.creating')}
      submitting={submitting}
    >
      <div className="space-y-5 text-app-ink">
        <div className="flex items-center gap-3 p-4 rounded-lg bg-app-surface-sidebar border border-app-border">
          <div className="flex size-10 items-center justify-center rounded-lg bg-app-warning/20">
            <FolderOpen size={20} className="text-app-warning-text" />
          </div>
          <div className="app-text-body text-app-ink/60">
            {t('apps:pms.createFolderDescription')}
          </div>
        </div>

        <FormMessage
          id={`${nameInputId}-form-message`}
          message={error}
          tone="error"
          variant="form"
        />

        <FormFieldRow htmlFor={nameInputId} label={t('apps:pms.folderName')}>
          <input
            id={nameInputId}
            aria-describedby={`${nameInputId}-form-message`}
            type="text"
            placeholder={t('apps:pms.folderPlaceholder')}
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
      </div>
    </FormDialog>
  );
};
