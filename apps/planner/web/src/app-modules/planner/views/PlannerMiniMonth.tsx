import { useMemo } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../../lib/utils';
import { getKoreanHolidayNames } from '../../../lib/korean-holidays';
import {
  buildPlannerDatePickerGrid,
  getPlannerDateFormatter,
} from './planner-calendar-view-model';

export interface PlannerMiniMonthProps {
  pickerYear: number;
  pickerMonth: number;
  viewYear: number;
  viewMonth: number;
  selectedDate: number;
  today: Date;
  onPrev: () => void;
  onNext: () => void;
  onPickDate: (year: number, month: number, day: number) => void;
}

// Google Calendar style compact month grid shared by the sidebar and the
// heading date-picker popover.
export function PlannerMiniMonth({
  pickerYear,
  pickerMonth,
  viewYear,
  viewMonth,
  selectedDate,
  today,
  onPrev,
  onNext,
  onPickDate,
}: PlannerMiniMonthProps) {
  const { i18n, t } = useTranslation('apps');
  const cells = useMemo(
    () =>
      buildPlannerDatePickerGrid({
        pickerYear,
        pickerMonth,
        viewYear,
        viewMonth,
        selectedDate,
        today,
        getHolidayNames: getKoreanHolidayNames,
      }),
    [pickerMonth, pickerYear, selectedDate, today, viewMonth, viewYear],
  );

  return (
    <div>
      <div className="mb-1 flex items-center justify-between pl-2">
        <div className="app-text-control text-app-ink tabular-nums">
          {getPlannerDateFormatter(i18n.language, 'monthYear').format(
            new Date(pickerYear, pickerMonth, 1),
          )}
        </div>
        <div className="flex items-center">
          <button
            type="button"
            onClick={onPrev}
            aria-label={t('planner.datePicker.previousMonth')}
            className="flex size-7 items-center justify-center rounded-full text-app-ink/55 hover:bg-app-surface-hover hover:text-app-ink"
          >
            <ChevronLeft size={16} />
          </button>
          <button
            type="button"
            onClick={onNext}
            aria-label={t('planner.datePicker.nextMonth')}
            className="flex size-7 items-center justify-center rounded-full text-app-ink/55 hover:bg-app-surface-hover hover:text-app-ink"
          >
            <ChevronRight size={16} />
          </button>
        </div>
      </div>

      <div className="grid grid-cols-7">
        {Array.from({ length: 7 }, (_, i) => (
          <div
            key={`weekday-${i}`}
            className={cn(
              'py-1 text-center text-[11px] font-medium',
              // Sun-start: index 0 is Sunday (red).
              i === 0 ? 'text-app-danger' : 'text-app-ink/55',
            )}
          >
            {getPlannerDateFormatter(i18n.language, 'weekdayNarrow').format(
              new Date(2026, 1, i + 1),
            )}
          </div>
        ))}
        {cells.map((cell) => {
          if (cell.kind === 'blank') {
            return <div key={cell.key} className="h-7" />;
          }
          return (
            <div
              key={cell.key}
              className="flex h-7 items-center justify-center"
            >
              <button
                type="button"
                onClick={() => onPickDate(pickerYear, pickerMonth, cell.day)}
                title={
                  cell.holidayNames ? cell.holidayNames.join(', ') : undefined
                }
                aria-current={cell.isToday ? 'date' : undefined}
                className={cn(
                  'flex size-6 items-center justify-center rounded-full text-[11px] transition-colors tabular-nums',
                  cell.isToday
                    ? 'bg-app-accent font-semibold text-app-accent-fg'
                    : cell.isSelected
                      ? 'bg-app-accent/15 font-semibold text-app-accent'
                      : cell.holidayNames || cell.isSunday
                        ? 'text-app-danger hover:bg-app-surface-hover'
                        : 'text-app-ink hover:bg-app-surface-hover',
                )}
              >
                {cell.day}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
