import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FormMessage } from '@miy/ui';
import { DateInput } from '../../../components/date/DateInput';
import {
  buildBulkDatesAction,
  isBulkDateRangeInverted,
  type BulkDateField,
  type BulkUpdateAction,
} from './bulk-action-bar-model';

const EMPTY_FIELD: BulkDateField = { clear: false, value: '' };
const RANGE_MESSAGE_ID = 'pms-bulk-dates-range-message';

/** Start/due dates for every selected task; a blank field is left alone. */
export function BulkDatesForm({
  disabled,
  onApply,
}: {
  disabled: boolean;
  onApply: (action: Extract<BulkUpdateAction, { type: 'dates' }>) => void;
}) {
  const { t } = useTranslation('apps');
  const [start, setStart] = useState<BulkDateField>(EMPTY_FIELD);
  const [due, setDue] = useState<BulkDateField>(EMPTY_FIELD);
  const action = buildBulkDatesAction(start, due);
  const rangeInverted = isBulkDateRangeInverted(start, due);

  const fields = [
    {
      field: start,
      id: 'pms-bulk-start-date',
      label: t('pms.bulk.dates.startDate'),
      setField: setStart,
    },
    {
      field: due,
      id: 'pms-bulk-due-date',
      label: t('pms.bulk.dates.dueDate'),
      setField: setDue,
    },
  ];

  return (
    <form
      className="flex w-64 flex-col gap-2 p-3"
      onSubmit={(event) => {
        event.preventDefault();
        if (action && !rangeInverted) onApply(action);
      }}
    >
      {fields.map(({ field, id, label, setField }) => (
        <div key={id} className="flex flex-col gap-1">
          <label className="app-text-overline text-app-ink/55" htmlFor={id}>
            {label}
          </label>
          <div className="flex items-center gap-2">
            <DateInput
              aria-describedby={RANGE_MESSAGE_ID}
              aria-label={label}
              className="app-field-input-sm min-w-0 flex-1"
              disabled={field.clear}
              id={id}
              onValueChange={(value) => setField({ ...field, value })}
              value={field.value}
            />
            <label className="app-text-caption flex shrink-0 cursor-pointer items-center gap-1 text-app-ink/70">
              <input
                checked={field.clear}
                className="size-3.5 rounded accent-app-accent"
                onChange={(event) =>
                  setField({ ...field, clear: event.target.checked })
                }
                type="checkbox"
              />
              {t('pms.bulk.dates.clear')}
            </label>
          </div>
        </div>
      ))}
      <p className="app-text-caption text-app-ink/50">
        {t('pms.bulk.dates.hint')}
      </p>
      <FormMessage
        id={RANGE_MESSAGE_ID}
        message={rangeInverted ? t('pms.bulk.dates.invalidRange') : null}
      />
      <button
        className="app-text-control-sm rounded bg-app-accent px-3 py-1.5 font-medium text-app-accent-fg transition-colors hover:bg-app-accent/90 disabled:cursor-not-allowed disabled:opacity-50"
        disabled={disabled || !action || rangeInverted}
        type="submit"
      >
        {t('pms.bulk.dates.apply')}
      </button>
    </form>
  );
}
