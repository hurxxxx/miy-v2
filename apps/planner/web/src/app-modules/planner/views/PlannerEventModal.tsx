import { useEffect, useMemo, useReducer, useState } from 'react';
import { Button, Dialog } from '@miy/ui';
import { useTranslation } from 'react-i18next';
import { CalendarDays, Clock3 } from 'lucide-react';

import { DateInput } from '../../../components/date/DateInput';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import { useAppApi } from '../../../platform/runtime/runtime-context';
import {
  createPlannerEvent,
  deletePlannerEvent,
  getPlannerEvent,
  updatePlannerEvent,
  type PlannerEvent,
} from '../api/planner-api';
import {
  buildPlannerEventSavePayload,
  buildPlannerEventUpdatePayload,
  canSave as canSavePlannerEventModal,
  initialPlannerEventModalState,
  isPlannerEventModalEdited,
  plannerEventModalEditableFields,
  plannerEventModalReducer,
  plannerEventModalSessionKey,
  type PlannerEventModalRange,
} from './planner-event-modal-model';

interface PlannerEventModalProps {
  contentClassName?: string;
  isOpen: boolean;
  onClose: () => void;
  eventId?: string | null;
  initialRange?: PlannerEventModalRange | null;
  onSaved?: (event: PlannerEvent) => void;
  onDeleted?: (eventId: string) => void;
  overlayClassName?: string;
  timeZone: string;
}

export function PlannerEventModal({
  isOpen,
  eventId,
  initialRange,
  ...props
}: PlannerEventModalProps) {
  if (!isOpen) {
    return null;
  }
  return (
    <PlannerEventModalContent
      key={plannerEventModalSessionKey(eventId, initialRange)}
      eventId={eventId}
      initialRange={initialRange}
      {...props}
    />
  );
}

type PlannerEventModalContentProps = Omit<PlannerEventModalProps, 'isOpen'>;

function PlannerEventModalContent(props: PlannerEventModalContentProps) {
  return usePlannerEventModalElement(props);
}

function usePlannerEventModalElement({
  onClose,
  contentClassName,
  eventId,
  initialRange,
  onSaved,
  onDeleted,
  overlayClassName,
  timeZone: initialTimeZone,
}: PlannerEventModalContentProps) {
  const { t } = useTranslation(['apps', 'common']);
  const [timeZone] = useState(initialTimeZone);
  // miy: the app's own API client replaces AI-DO's auth token.
  const token = useAppApi();
  const [state, dispatch] = useReducer(
    plannerEventModalReducer,
    initialRange,
    (range) => initialPlannerEventModalState(range, undefined, timeZone),
  );
  // miy: unsaved edits and in-flight saves are reported to the portal
  // (runtime.setNavigationState). The baseline is the form as opened, or as
  // loaded for an existing event.
  const [baseline, setBaseline] = useState(() =>
    plannerEventModalEditableFields(state),
  );
  useNavigationDirty(
    state.saving || state.deleting || isPlannerEventModalEdited(baseline, state),
  );
  const {
    loading,
    saving,
    deleting,
    error,
    title,
    description,
    location,
    allDay,
    startDateValue,
    startTimeValue,
    endDateValue,
    endTimeValue,
  } = state;

  const isEditMode = Boolean(eventId);

  useEffect(() => {
    if (!eventId || !token) {
      return;
    }
    let cancelled = false;
    dispatch({ type: 'loadStart' });
    getPlannerEvent(token, eventId)
      .then((event) => {
        if (cancelled) return;
        dispatch({ type: 'loadSuccess', event });
        setBaseline(
          plannerEventModalEditableFields(
            plannerEventModalReducer(initialPlannerEventModalState(null, undefined, timeZone), {
              type: 'loadSuccess',
              event,
            }),
          ),
        );
      })
      .catch((err: Error) => {
        if (cancelled) return;
        dispatch({
          type: 'loadFailure',
          error: err.message ?? t('apps:planner.loadFailed'),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [eventId, timeZone, token, t]);

  const canSave = useMemo(() => {
    return canSavePlannerEventModal(state);
  }, [state]);

  function toggleAllDay(nextAllDay: boolean) {
    dispatch({ type: 'setAllDay', allDay: nextAllDay });
  }

  async function handleSave() {
    if (!token || !canSave || saving || deleting) return;
    dispatch({ type: 'saveStart' });
    try {
      const event = eventId
        ? await updatePlannerEvent(token, eventId, buildPlannerEventUpdatePayload(state, baseline))
        : await createPlannerEvent(token, buildPlannerEventSavePayload(state), timeZone);
      setBaseline(plannerEventModalEditableFields(state));
      onSaved?.(event);
    } catch (err) {
      dispatch({
        type: 'setError',
        error:
          err instanceof Error ? err.message : t('apps:planner.saveFailed'),
      });
    } finally {
      dispatch({ type: 'saveDone' });
    }
  }

  async function handleDelete() {
    if (!token || !eventId || saving || deleting) return;
    dispatch({ type: 'deleteStart' });
    try {
      await deletePlannerEvent(token, eventId);
      setBaseline(plannerEventModalEditableFields(state));
      onDeleted?.(eventId);
    } catch (err) {
      dispatch({
        type: 'setError',
        error:
          err instanceof Error ? err.message : t('apps:planner.deleteFailed'),
      });
    } finally {
      dispatch({ type: 'deleteDone' });
    }
  }

  return (
    <Dialog
      closeLabel={t('common:actions.close')}
      open
      onOpenChange={(open) => {
        if (!open && !saving && !deleting) onClose();
      }}
      title={
        isEditMode ? t('apps:planner.editEvent') : t('apps:planner.newEvent')
      }
      description={t('apps:planner.eventDescription')}
      maxWidth="max-w-xl"
      dismissOnInteractOutside={false}
      contentClassName={contentClassName}
      overlayClassName={overlayClassName}
      actions={
        <div className="flex w-full items-center justify-between gap-3">
          <div>
            {isEditMode ? (
              <Button
                variant="secondary"
                onClick={() => void handleDelete()}
                disabled={deleting || saving}
                className="text-[var(--ui-color-danger)]"
              >
                {deleting
                  ? t('apps:planner.deletePending')
                  : t('common:actions.delete')}
              </Button>
            ) : null}
          </div>
          <div className="flex items-center gap-3">
            <Button
              variant="secondary"
              onClick={onClose}
              disabled={saving || deleting}
            >
              {t('common:actions.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => void handleSave()}
              disabled={!canSave || saving || loading || deleting}
            >
              {saving
                ? t('apps:planner.savePending')
                : t('common:actions.save')}
            </Button>
          </div>
        </div>
      }
    >
      <div className="space-y-5 text-app-ink">
        {error ? (
          <div
            role="alert"
            className="app-text-body rounded-md border border-[var(--ui-color-warning)]/40 bg-[var(--ui-color-warning)]/10 px-3 py-2 text-[var(--ui-color-warning)]"
          >
            {error}
          </div>
        ) : null}

        {loading ? (
          <div className="rounded-md border border-app-border bg-app-surface px-4 py-6 text-center text-app-ink/50">
            {t('apps:planner.eventLoading')}
          </div>
        ) : (
          <fieldset disabled={saving || deleting} className="min-w-0 space-y-5">
            <div className="space-y-1">
              <label className="app-text-control-sm text-app-ink/70">
                {t('apps:planner.title')}{' '}
                <span className="text-[var(--ui-color-danger)]">*</span>
              </label>
              <input
                type="text"
                value={title}
                onChange={(e) =>
                  dispatch({ type: 'setTitle', title: e.target.value })
                }
                maxLength={200}
                aria-label={t('apps:planner.title')}
                placeholder={t('apps:planner.titlePlaceholder')}
                className="app-text-body w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink placeholder:text-app-ink/30 focus:border-app-accent focus:outline-none"
              />
            </div>

            <div className="space-y-2">
              <span className="app-text-control-sm text-app-ink/70">
                {t('apps:planner.eventTimeMode')}
              </span>
              <div
                aria-label={t('apps:planner.eventTimeMode')}
                className="grid grid-cols-2 gap-1 rounded-md border border-app-border bg-app-surface-sidebar p-1"
                role="radiogroup"
              >
                <button
                  aria-checked={!allDay}
                  className={`app-text-control-sm inline-flex h-9 items-center justify-center gap-2 rounded-[5px] px-3 transition-colors ${
                    !allDay
                      ? 'bg-app-accent text-app-accent-fg shadow-sm'
                      : 'text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink'
                  }`}
                  onClick={() => toggleAllDay(false)}
                  role="radio"
                  type="button"
                >
                  <Clock3 aria-hidden="true" size={15} />
                  <span>{t('apps:planner.timedEvent')}</span>
                </button>
                <button
                  aria-checked={allDay}
                  className={`app-text-control-sm inline-flex h-9 items-center justify-center gap-2 rounded-[5px] px-3 transition-colors ${
                    allDay
                      ? 'bg-app-accent text-app-accent-fg shadow-sm'
                      : 'text-app-ink/60 hover:bg-app-surface-hover hover:text-app-ink'
                  }`}
                  onClick={() => toggleAllDay(true)}
                  role="radio"
                  type="button"
                >
                  <CalendarDays aria-hidden="true" size={15} />
                  <span>{t('apps:planner.allDay')}</span>
                </button>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <label className="app-text-control-sm text-app-ink/70">
                  {t('apps:planner.startDate')}
                </label>
                <DateInput
                  value={startDateValue}
                  aria-label={t('apps:planner.startDate')}
                  onValueChange={(value) =>
                    dispatch({
                      type: 'setStartDateValue',
                      startDateValue: value,
                    })
                  }
                  className="app-text-body w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none"
                />
              </div>
              <div className="space-y-1">
                <label className="app-text-control-sm text-app-ink/70">
                  {t('apps:planner.endDate')}
                </label>
                <DateInput
                  value={endDateValue}
                  aria-label={t('apps:planner.endDate')}
                  onValueChange={(value) =>
                    dispatch({ type: 'setEndDateValue', endDateValue: value })
                  }
                  className="app-text-body w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none"
                />
              </div>
            </div>

            {allDay ? (
              <p className="app-text-caption text-app-ink/45">
                {t('apps:planner.endDateHint')}
              </p>
            ) : (
              <div className="space-y-2">
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1">
                    <label className="app-text-control-sm text-app-ink/70">
                      {t('apps:planner.startTime')}
                    </label>
                    <input
                      aria-label={t('apps:planner.startTime')}
                      className="app-text-body w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none"
                      onChange={(e) =>
                        dispatch({
                          type: 'setStartTimeValue',
                          startTimeValue: e.target.value,
                        })
                      }
                      type="time"
                      value={startTimeValue}
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="app-text-control-sm text-app-ink/70">
                      {t('apps:planner.endTime')}
                    </label>
                    <input
                      aria-label={t('apps:planner.endTime')}
                      className="app-text-body w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink focus:border-app-accent focus:outline-none"
                      onChange={(e) =>
                        dispatch({
                          type: 'setEndTimeValue',
                          endTimeValue: e.target.value,
                        })
                      }
                      type="time"
                      value={endTimeValue}
                    />
                  </div>
                </div>
                <p className="app-text-caption text-app-ink/45">
                  {t('apps:planner.optionalTimeHint')}
                </p>
              </div>
            )}

            <div>
              <div className="space-y-1">
                <label className="app-text-control-sm text-app-ink/70">
                  {t('apps:planner.location')}
                </label>
                <input
                  type="text"
                  value={location}
                  onChange={(e) =>
                    dispatch({ type: 'setLocation', location: e.target.value })
                  }
                  maxLength={240}
                  aria-label={t('apps:planner.location')}
                  placeholder={t('apps:planner.locationPlaceholder')}
                  className="app-text-body w-full rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink placeholder:text-app-ink/30 focus:border-app-accent focus:outline-none"
                />
              </div>
            </div>

            <div className="space-y-1">
              <label className="app-text-control-sm text-app-ink/70">
                {t('apps:planner.description')}
              </label>
              <textarea
                value={description}
                onChange={(e) =>
                  dispatch({
                    type: 'setDescription',
                    description: e.target.value,
                  })
                }
                rows={4}
                aria-label={t('apps:planner.description')}
                placeholder={t('apps:planner.descriptionPlaceholder')}
                className="app-text-body w-full resize-none rounded-md border border-app-border bg-app-surface-sidebar px-3 py-2 text-app-ink placeholder:text-app-ink/30 focus:border-app-accent focus:outline-none"
              />
            </div>
          </fieldset>
        )}
      </div>
    </Dialog>
  );
}
