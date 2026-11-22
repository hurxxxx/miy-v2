import { useCallback, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Button, StatusSlot } from '@miy/ui';
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  UnifiedCalendar,
  type UnifiedCalendarHandle,
} from '../../../components/calendar/UnifiedCalendar';
import type { CalendarEvent } from '../../../platform/calendar/calendar-types';
import type {
  PmsMilestone,
  PmsTask,
  PmsTaskListStatus,
} from '../api/pms-api';
import {
  addCalendarMonths,
  buildPmsCalendarEvents,
  buildPmsMilestoneCalendarEvents,
  formatPmsCalendarTitle,
  getPmsCalendarPeriod,
  hasCalendarEventInPeriod,
  isTodayInPmsCalendarPeriod,
  PMS_CALENDAR_RANGE_VIEWS,
  PMS_CALENDAR_RANGES,
  pmsCalendarRangeForView,
  resolvePmsCalendarRangeAnchor,
  startOfCalendarMonth,
  type PmsCalendarRange,
} from './calendar-view-model';
import { formatDate, getStatusLabel } from './pms-constants';

const RANGE_LABEL_KEYS = {
  month: 'pms.calendar.rangeMonth',
  week: 'pms.calendar.rangeWeek',
  day: 'pms.calendar.rangeDay',
} as const satisfies Record<PmsCalendarRange, string>;

const STEP_LABEL_KEYS = {
  month: {
    next: 'pms.calendar.nextMonth',
    previous: 'pms.calendar.previousMonth',
  },
  week: {
    next: 'pms.calendar.nextWeek',
    previous: 'pms.calendar.previousWeek',
  },
  day: { next: 'pms.calendar.nextDay', previous: 'pms.calendar.previousDay' },
} as const satisfies Record<
  PmsCalendarRange,
  { next: string; previous: string }
>;

type ShownCalendarDays = {
  currentDate: Date;
  rangeEnd: Date;
  rangeStart: Date;
  view: string;
};

type HoveredCalendarTask = {
  eventId: string;
  rect: {
    bottom: number;
    left: number;
    top: number;
  };
  task: PmsTask;
};

export const CalendarView = ({
  milestones = [],
  tasks,
  taskListStatuses,
  taskSourceLabels,
}: {
  /** miy: milestones with a due date show on that day. */
  milestones?: readonly PmsMilestone[];
  tasks: PmsTask[];
  taskListStatuses?: PmsTaskListStatus[];
  /** Small label after each task title, such as the list it comes from. */
  taskSourceLabels?: ReadonlyMap<string, string>;
}) => {
  const { i18n, t } = useTranslation('apps');
  const calendarRef = useRef<UnifiedCalendarHandle | null>(null);
  const [initialMonth] = useState(() => startOfCalendarMonth(new Date()));
  const [shown, setShown] = useState<ShownCalendarDays>(() => ({
    currentDate: initialMonth,
    rangeEnd: addCalendarMonths(initialMonth, 1),
    rangeStart: initialMonth,
    view: PMS_CALENDAR_RANGE_VIEWS.month,
  }));
  const [hoveredTask, setHoveredTask] = useState<HoveredCalendarTask | null>(
    null,
  );

  const events = useMemo(
    () => [
      ...buildPmsMilestoneCalendarEvents(milestones, (milestone) =>
        t('pms.milestoneSettings.calendarTitle', { title: milestone.title }),
      ),
      ...buildPmsCalendarEvents(
        tasks,
        taskListStatuses,
        taskSourceLabels
          ? (task) => {
              const list = taskSourceLabels.get(task.id);
              return list
                ? t('pms.allTasks.taskWithList', { title: task.title, list })
                : task.title;
            }
          : undefined,
      ),
    ],
    [milestones, t, taskListStatuses, taskSourceLabels, tasks],
  );
  const tasksById = useMemo(
    () => new Map(tasks.map((task) => [task.id, task])),
    [tasks],
  );
  const range = pmsCalendarRangeForView(shown.view);
  const period = useMemo(
    () => getPmsCalendarPeriod(range, shown),
    [range, shown],
  );
  const periodTitle = useMemo(
    () => formatPmsCalendarTitle(range, period, i18n.language),
    [i18n.language, period, range],
  );
  const periodEmpty =
    range !== 'month' && !hasCalendarEventInPeriod(events, period);
  // Like FullCalendar's own toolbar: nothing to return to while today is already shown.
  const showingToday = isTodayInPmsCalendarPeriod(period);
  const handleEventMouseEnter = useCallback(
    (event: CalendarEvent, anchorEl: HTMLElement) => {
      const task = tasksById.get(event.sourceId);
      if (!task) return;
      const rect = anchorEl.getBoundingClientRect();
      setHoveredTask({
        eventId: event.id,
        rect: {
          bottom: rect.bottom,
          left: rect.left,
          top: rect.top,
        },
        task,
      });
    },
    [tasksById],
  );
  const handleEventMouseLeave = useCallback((event: CalendarEvent) => {
    setHoveredTask((current) =>
      current?.eventId === event.id ? null : current,
    );
  }, []);

  const handleDatesSet = useCallback((next: ShownCalendarDays) => {
    setShown({
      currentDate: next.currentDate,
      rangeEnd: next.rangeEnd,
      rangeStart: next.rangeStart,
      view: next.view,
    });
    setHoveredTask(null);
  }, []);
  const shiftVisibleMonth = useCallback(
    (months: number) => {
      calendarRef.current?.gotoDate(addCalendarMonths(period.start, months));
      setHoveredTask(null);
    },
    [period.start],
  );
  const shiftPeriod = useCallback(
    (direction: -1 | 1) => {
      if (range === 'month') {
        shiftVisibleMonth(direction);
        return;
      }
      if (direction < 0) calendarRef.current?.prev();
      else calendarRef.current?.next();
      setHoveredTask(null);
    },
    [range, shiftVisibleMonth],
  );
  const changeRange = useCallback(
    (next: PmsCalendarRange) => {
      if (next === range) return;
      calendarRef.current?.changeView(
        PMS_CALENDAR_RANGE_VIEWS[next],
        resolvePmsCalendarRangeAnchor(period),
      );
      setHoveredTask(null);
    },
    [period, range],
  );

  return (
    <div className="h-full flex flex-col card p-0 overflow-hidden">
      <div className="flex flex-col gap-2 border-b border-app-border bg-app-surface-sidebar/30 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 items-center gap-1">
          <CalendarDays size={16} className="mr-1 shrink-0 text-app-accent" />
          {range === 'month' ? (
            <Button
              aria-label={t('pms.calendar.previousYear')}
              onClick={() => shiftVisibleMonth(-12)}
              size="icon"
              title={t('pms.calendar.previousYear')}
              variant="ghost"
            >
              <ChevronsLeft aria-hidden="true" size={16} />
            </Button>
          ) : null}
          <Button
            aria-label={t(STEP_LABEL_KEYS[range].previous)}
            onClick={() => shiftPeriod(-1)}
            size="icon"
            title={t(STEP_LABEL_KEYS[range].previous)}
            variant="ghost"
          >
            <ChevronLeft aria-hidden="true" size={16} />
          </Button>
          <h3 className="app-text-title-md mx-2 min-w-0 break-keep text-app-ink">
            {periodTitle}
          </h3>
          <Button
            aria-label={t(STEP_LABEL_KEYS[range].next)}
            onClick={() => shiftPeriod(1)}
            size="icon"
            title={t(STEP_LABEL_KEYS[range].next)}
            variant="ghost"
          >
            <ChevronRight aria-hidden="true" size={16} />
          </Button>
          {range === 'month' ? (
            <Button
              aria-label={t('pms.calendar.nextYear')}
              onClick={() => shiftVisibleMonth(12)}
              size="icon"
              title={t('pms.calendar.nextYear')}
              variant="ghost"
            >
              <ChevronsRight aria-hidden="true" size={16} />
            </Button>
          ) : null}
        </div>
        <StatusSlot
          className="sm:mx-3 sm:min-w-0 sm:flex-1"
          message={periodEmpty ? t('pms.calendar.emptyPeriod') : null}
        />
        <div className="flex shrink-0 items-center gap-2">
          <div
            aria-label={t('pms.calendar.rangeLabel')}
            className="flex rounded-md border border-app-border p-0.5"
            role="group"
          >
            {PMS_CALENDAR_RANGES.map((option) => (
              <Button
                key={option}
                aria-pressed={range === option}
                className="h-6 px-2"
                onClick={() => changeRange(option)}
                size="dense"
                variant={range === option ? 'primary' : 'ghost'}
              >
                {t(RANGE_LABEL_KEYS[option])}
              </Button>
            ))}
          </div>
          <Button
            disabled={showingToday}
            onClick={() => calendarRef.current?.today()}
            size="dense"
            variant="secondary"
          >
            {t('pms.calendar.today')}
          </Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <UnifiedCalendar
          ref={calendarRef}
          className="min-h-full"
          // miy: FullCalendar follows the portal language, as in the planner
          // port (AI-DO always used the Korean locale).
          locale={i18n.language}
          dayMaxEvents={false}
          eventClassNames={(event) =>
            event.sourceType === 'pms_due' ? ['fc-pms-task-event'] : []
          }
          events={events}
          height="auto"
          initialDate={initialMonth}
          initialView="dayGridMonth"
          layout="content"
          onDatesSet={handleDatesSet}
          onEventMouseEnter={handleEventMouseEnter}
          onEventMouseLeave={handleEventMouseLeave}
        />
      </div>
      <CalendarTaskHoverCard
        hoveredTask={hoveredTask}
        sourceLabel={
          hoveredTask ? taskSourceLabels?.get(hoveredTask.task.id) : undefined
        }
        taskListStatuses={taskListStatuses}
      />
    </div>
  );
};

function CalendarTaskHoverCard({
  hoveredTask,
  sourceLabel,
  taskListStatuses,
}: {
  hoveredTask: HoveredCalendarTask | null;
  sourceLabel?: string;
  taskListStatuses?: PmsTaskListStatus[];
}) {
  if (!hoveredTask || typeof document === 'undefined') return null;

  return createPortal(
    <div
      className="pointer-events-none fixed z-[1000] w-72 rounded-lg border border-app-border bg-app-bg p-3 text-left shadow-xl"
      role="tooltip"
      style={getHoverCardStyle(hoveredTask.rect)}
    >
      <CalendarTaskDetailCard
        sourceLabel={sourceLabel}
        task={hoveredTask.task}
        taskListStatuses={taskListStatuses}
      />
    </div>,
    document.body,
  );
}

function CalendarTaskDetailCard({
  sourceLabel,
  task,
  taskListStatuses,
}: {
  sourceLabel?: string;
  task: PmsTask;
  taskListStatuses?: PmsTaskListStatus[];
}) {
  const { t } = useTranslation('apps');
  const assignee = resolveCalendarTaskAssignee(task);
  const notSet = t('pms.list.notSet');
  const detailRows = [
    ...(sourceLabel
      ? [{ label: t('pms.allTasks.sourceList'), value: sourceLabel }]
      : []),
    {
      label: t('pms.list.status'),
      value: task.status_label || getStatusLabel(task.status, taskListStatuses),
    },
    {
      label: t('pms.filter.priorityLabel'),
      value: task.priority_label || task.priority || notSet,
    },
    {
      label: t('pms.list.assignee'),
      value: assignee || t('pms.taskDetail.unassigned'),
    },
    {
      label: t('pms.filter.startDateLabel'),
      value: formatDate(task.start_date) || notSet,
    },
    {
      label: t('pms.list.dueDate'),
      value: formatDate(task.due_date) || notSet,
    },
  ];

  return (
    <>
      <div className="app-text-micro mb-1 text-app-ink/45">
        {task.reference}
      </div>
      <div className="app-text-body-sm mb-2 line-clamp-2 font-semibold text-app-ink">
        {task.title}
      </div>
      <dl className="space-y-1.5">
        {detailRows.map((row) => (
          <div
            key={row.label}
            className="flex items-start justify-between gap-3"
          >
            <dt className="app-text-micro shrink-0 text-app-ink/50">
              {row.label}
            </dt>
            <dd className="app-text-micro min-w-0 truncate text-right text-app-ink">
              {row.value}
            </dd>
          </div>
        ))}
      </dl>
    </>
  );
}

function getHoverCardStyle(rect: HoveredCalendarTask['rect']): {
  left: number;
  top: number;
} {
  const margin = 8;
  const width = 288;
  const estimatedHeight = 180;
  const viewportWidth =
    typeof window === 'undefined' ? 1024 : window.innerWidth;
  const viewportHeight =
    typeof window === 'undefined' ? 768 : window.innerHeight;
  const left = Math.min(
    Math.max(rect.left, margin),
    Math.max(margin, viewportWidth - width - margin),
  );
  const below = rect.bottom + margin;
  const top =
    below + estimatedHeight > viewportHeight
      ? Math.max(margin, rect.top - estimatedHeight - margin)
      : below;

  return { left, top };
}

function resolveCalendarTaskAssignee(task: PmsTask): string {
  if (task.assignee_names?.length) {
    return task.assignee_names.join(', ');
  }
  return task.assignee_name ?? '';
}
