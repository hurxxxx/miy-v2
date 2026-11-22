// Port of AI-DO apps/web/src/app-modules/planner/views/PlannerView.tsx
// (0002af062). Markup, classes and interactions are AI-DO's. What miy changed:
// - Only the planner's own events are shown (source `planner_event`). The
//   timeline surface, meetings (choice popover, create/preview modals), PMS
//   task opening and Google Calendar sync are not part of this release, so
//   their controls are not rendered; the pure session/controller models keep
//   those branches unchanged.
// - The `?event=` deep link reads runtime.location (useAppSearchParams)
//   instead of react-router, and the time zone comes from the portal display
//   settings instead of the user profile.
// - A year without bundled holiday data shows "휴일 정보 없음".
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { LazyMotion, domAnimation, m } from 'motion/react';
import {
  CalendarDays,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  Menu,
  Plus,
  Search,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../../lib/utils';
import { getKoreanHolidayYearRange } from '../../../lib/korean-holidays';
import { useCalendarEvents } from '../../../platform/calendar/use-calendar-events';
import {
  CALENDAR_SOURCE_COLORS,
  type CalendarEvent,
  type CalendarSourceFilter,
  type CalendarSourceType,
} from '../../../platform/calendar/calendar-types';
import { useAppSearchParams } from '../../../platform/runtime/app-location';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import { normalizeTimeZone } from '../../../platform/time/time-utils';
import { updatePlannerEvent } from '../api/planner-api';
import {
  UnifiedCalendar,
  type UnifiedCalendarHandle,
  type UnifiedCalendarView,
} from '../../../components/calendar/UnifiedCalendar';
import { PlannerEventModal } from './PlannerEventModal';
import {
  PlannerMiniMonth,
  type PlannerMiniMonthProps,
} from './PlannerMiniMonth';
import { PlannerInsightsPanel } from './PlannerInsightsPanel';
import {
  PlannerSidebar,
  type PlannerSidebarSource,
} from './PlannerSidebar';
import {
  buildPlannerMonthDayGroups,
  countPlannerEventsBySource,
  getPlannerWeekRange,
  groupPlannerEventsBySource,
  isPlannerDayGroupId,
  listPlannerUpcomingEvents,
  parsePlannerDate,
  rankPlannerBusyDays,
  type PlannerDayGroup,
} from './planner-month-insights-model';
import { createPlannerCalendarScheduleWorkflow } from './planner-calendar-schedule-workflow';
import {
  movePlannerVisiblePeriod,
  pickPlannerDate,
  resolvePlannerCalendarEventClick,
  runPlannerCalendarCommand,
} from './planner-calendar-controller';
import {
  formatPlannerHeading,
  readTimelineRangeDays,
  type PlannerSurfaceMode,
  type PlannerViewMode,
  type TimelineRangeDays,
} from './planner-calendar-view-model';
import {
  applyCalendarDatesSet,
  closeCreateMenu,
  closePicker,
  closePlannerEventModal as closePlannerEventModalSession,
  initializePlannerCalendarSession,
  movePickerMonth,
  openPicker as openPlannerPicker,
  openPlannerEventCreate as openPlannerEventCreateSession,
  openPlannerEventEdit as openPlannerEventEditSession,
  toggleCreateMenu,
  type PlannerEventDraftRange,
} from './planner-calendar-session';

const VIEW_MODE_LABEL_KEYS: Record<PlannerViewMode, string> = {
  Month: 'planner.viewModes.month',
  Week: 'planner.viewModes.week',
  Day: 'planner.viewModes.day',
  Agenda: 'planner.viewModes.agenda',
};

const VIEW_MODE_TO_FC: Record<PlannerViewMode, UnifiedCalendarView> = {
  Month: 'dayGridMonth',
  Week: 'timeGridWeek',
  Day: 'timeGridDay',
  Agenda: 'listWeek',
};

// miy: the planner's own events only (AI-DO also listed meeting, pms_due,
// pms_block and google_calendar from other apps).
const PLANNER_SOURCES = [
  'planner_event',
] as const satisfies CalendarSourceFilter;

// miy: the timeline surface is not part of this release.
const SURFACE_MODE: PlannerSurfaceMode = 'calendar';

const SOURCE_LABEL_KEYS: Record<CalendarSourceType, string> = {
  meeting: 'planner.timeline.sources.meeting',
  pms_due: 'planner.timeline.sources.pmsDue',
  pms_block: 'planner.timeline.sources.pmsBlock',
  planner_event: 'planner.timeline.sources.plannerEvent',
  google_calendar: 'planner.timeline.sources.googleCalendar',
};

// Day-group chips summarise several tasks, so they cannot be dragged.
function isPlannerEventEditable(event: CalendarEvent): boolean {
  // Imported Google Calendar events are read-only copies.
  return (
    !isPlannerDayGroupId(event.id) && event.sourceType !== 'google_calendar'
  );
}

/** miy: years in [start, end) that the bundled holiday dataset does not cover. */
function yearsWithoutHolidayData(start: Date, end: Date): number[] {
  const { min, max } = getKoreanHolidayYearRange();
  const last = new Date(end.getTime() - 1).getFullYear();
  const years: number[] = [];
  for (let year = start.getFullYear(); year <= last; year += 1) {
    if (year < min || year > max) years.push(year);
  }
  return years;
}

function withWorkspacePrefix(event: CalendarEvent): CalendarEvent {
  return event.workspace
    ? { ...event, title: `[${event.workspace.name}] ${event.title}` }
    : event;
}

function tint(color: string, percent: number): string {
  return `color-mix(in srgb, ${color} ${percent}%, transparent)`;
}

interface DayGroupPopoverProps {
  group: PlannerDayGroup;
  anchor: DOMRect;
  locale: string;
  onPick: (event: CalendarEvent) => void;
  onClose: () => void;
}

function DayGroupPopover({
  group,
  anchor,
  locale,
  onPick,
  onClose,
}: DayGroupPopoverProps) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    function onMouseDown(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        onClose();
      }
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') onClose();
    }
    document.addEventListener('mousedown', onMouseDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onMouseDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose]);
  const width = 288;
  const left = Math.max(
    8,
    Math.min(anchor.left, window.innerWidth - width - 8),
  );
  const top = Math.min(anchor.bottom + 4, window.innerHeight - 240);
  return (
    <dialog
      ref={ref}
      open
      className="fixed z-50 m-0 max-h-80 overflow-y-auto rounded-xl border border-app-border bg-app-surface p-2 shadow-xl"
      style={{ left, top, width }}
    >
      <div className="app-text-caption px-2 pb-1 pt-0.5 text-app-ink/55">
        {new Intl.DateTimeFormat(locale, {
          month: 'long',
          day: 'numeric',
          weekday: 'short',
        }).format(group.date)}
      </div>
      <ul>
        {group.items.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              onClick={() => onPick(item)}
              className="app-text-control-sm flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-app-ink hover:bg-app-surface-hover"
            >
              <span
                className="size-2 shrink-0 rounded-full"
                style={{ backgroundColor: item.color }}
              />
              <span className="truncate">
                {withWorkspacePrefix(item).title}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </dialog>
  );
}

interface DatePickerPopoverProps extends PlannerMiniMonthProps {
  onPickToday: () => void;
}

function DatePickerPopover({
  onPickToday,
  ...miniMonth
}: DatePickerPopoverProps) {
  const { t } = useTranslation('apps');
  return (
    <dialog
      open
      aria-label={t('planner.datePicker.chooseDate')}
      className="absolute left-0 top-full z-40 m-0 mt-2 w-72 rounded-lg border border-app-border bg-app-surface p-3 shadow-xl"
    >
      <PlannerMiniMonth {...miniMonth} />
      <div className="mt-2 flex justify-between border-t border-app-border pt-2">
        <button
          type="button"
          onClick={onPickToday}
          className="app-text-control-sm rounded px-2 py-1 text-app-accent hover:bg-app-surface-hover"
        >
          {t('planner.today')}
        </button>
      </div>
    </dialog>
  );
}

export const PlannerView = () => <>{usePlannerViewElement()}</>;

function usePlannerViewElement(): ReactNode {
  const { t, i18n } = useTranslation('apps');
  const today = new Date();
  // miy: the API client and display settings come from the frame runtime.
  const { api: token, settings } = useRuntime();
  const timeZone = normalizeTimeZone(settings.timeZone);
  const [searchParams, setSearchParams] = useAppSearchParams();
  const surfaceMode = SURFACE_MODE;
  const [timelineRangeDays] = useState<TimelineRangeDays>(() =>
    readTimelineRangeDays(),
  );
  const [viewMode, setViewMode] = useState<PlannerViewMode>('Month');
  const [session, setSession] = useState(() =>
    initializePlannerCalendarSession({
      today,
      viewMode: 'Month',
      surfaceMode,
      timelineRangeDays,
    }),
  );
  const calendarState = session.calendarState;
  const requestedPlannerEventId = searchParams.get('event');
  const plannerEventModalOpen =
    session.plannerEventModalOpen || Boolean(requestedPlannerEventId);
  const plannerEventId = requestedPlannerEventId ?? session.plannerEventId;
  const plannerEventRange = requestedPlannerEventId
    ? null
    : session.plannerEventRange;

  const calendarRef = useRef<UnifiedCalendarHandle | null>(null);

  const { events, loading, error, refresh } = useCalendarEvents({
    from: calendarState.rangeStart,
    to: calendarState.rangeEnd,
    sources: PLANNER_SOURCES,
    useMockData: false,
  });
  const [hiddenSources, setHiddenSources] = useState<
    ReadonlySet<CalendarSourceType>
  >(() => new Set());
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const normalizedQuery = searchQuery.trim().toLowerCase();
  const visibleEvents = useMemo(
    () =>
      events.filter(
        (event) =>
          !hiddenSources.has(event.sourceType) &&
          (!normalizedQuery ||
            event.title.toLowerCase().includes(normalizedQuery) ||
            Boolean(
              event.workspace?.name.toLowerCase().includes(normalizedQuery),
            )),
      ),
    [events, hiddenSources, normalizedQuery],
  );
  const isMonthView = surfaceMode === 'calendar' && viewMode === 'Month';
  const rangeStart = useMemo(
    () => parsePlannerDate(calendarState.rangeStart),
    [calendarState.rangeStart],
  );
  const rangeEnd = useMemo(
    () => parsePlannerDate(calendarState.rangeEnd),
    [calendarState.rangeEnd],
  );
  const holidayGapYears = useMemo(
    () => yearsWithoutHolidayData(rangeStart, rangeEnd),
    [rangeEnd, rangeStart],
  );
  const monthDayGroups = useMemo(
    () =>
      isMonthView
        ? buildPlannerMonthDayGroups(visibleEvents, rangeStart, rangeEnd)
        : null,
    [isMonthView, rangeEnd, rangeStart, visibleEvents],
  );
  const calendarEvents = useMemo(
    () =>
      monthDayGroups
        ? monthDayGroups.events
        : visibleEvents.map((event) => withWorkspacePrefix(event)),
    [monthDayGroups, visibleEvents],
  );
  // Filter counts cover the browsed month only, not the leading/trailing
  // days of other months that the 6-week grid also fetches.
  const browsedYear = calendarState.currentDate.getFullYear();
  const browsedMonth = calendarState.currentDate.getMonth();
  const sourceCounts = useMemo(
    () =>
      countPlannerEventsBySource(
        events,
        new Date(browsedYear, browsedMonth, 1),
        new Date(browsedYear, browsedMonth + 1, 1),
      ),
    [browsedMonth, browsedYear, events],
  );
  const sidebarSources = useMemo<PlannerSidebarSource[]>(
    () =>
      PLANNER_SOURCES.map((type) => ({
        type,
        label: t(SOURCE_LABEL_KEYS[type]),
        color: CALENDAR_SOURCE_COLORS[type],
        count: sourceCounts[type],
        visible: !hiddenSources.has(type),
      })),
    [hiddenSources, sourceCounts, t],
  );
  const toggleAllSources = useCallback((visible: boolean) => {
    setHiddenSources(visible ? new Set() : new Set(PLANNER_SOURCES));
  }, []);
  const [openDayGroup, setOpenDayGroup] = useState<{
    group: PlannerDayGroup;
    anchor: DOMRect;
  } | null>(null);
  const toggleSource = useCallback((type: CalendarSourceType) => {
    setHiddenSources((current) => {
      const next = new Set(current);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return next;
    });
  }, []);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  // The sidebar mini month browses independently and follows the main view
  // whenever the visible month changes.
  const viewYear = calendarState.currentDate.getFullYear();
  const viewMonth = calendarState.currentDate.getMonth();
  const [sidebarCursor, setSidebarCursor] = useState({
    year: viewYear,
    month: viewMonth,
  });
  useEffect(() => {
    setSidebarCursor({ year: viewYear, month: viewMonth });
  }, [viewMonth, viewYear]);
  const moveSidebarCursor = (delta: number) => {
    setSidebarCursor((current) => {
      const next = new Date(current.year, current.month + delta, 1);
      return { year: next.getFullYear(), month: next.getMonth() };
    });
  };

  // Mini date-picker popover. The picker has its own (year, month) cursor so
  // the user can browse without committing — the main view only updates when
  // they actually click a day.
  const pickerRef = useRef<HTMLDivElement>(null);
  const createMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!session.pickerOpen) return;
    function onMouseDown(event: MouseEvent) {
      if (
        pickerRef.current &&
        !pickerRef.current.contains(event.target as Node)
      ) {
        setSession(closePicker);
      }
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') setSession(closePicker);
    }
    document.addEventListener('mousedown', onMouseDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onMouseDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [session.pickerOpen]);

  useEffect(() => {
    if (!session.createMenuOpen) return;
    function onMouseDown(event: MouseEvent) {
      if (
        createMenuRef.current &&
        !createMenuRef.current.contains(event.target as Node)
      ) {
        setSession(closeCreateMenu);
      }
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') setSession(closeCreateMenu);
    }
    document.addEventListener('mousedown', onMouseDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onMouseDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [session.createMenuOpen]);

  // When the popover opens, sync its cursor to whatever the main view is showing.
  const openPicker = () => {
    setSession(openPlannerPicker);
  };

  const setMode = (mode: PlannerViewMode) => {
    setViewMode(mode);
    calendarRef.current?.changeView(VIEW_MODE_TO_FC[mode]);
  };

  const goToPreviousPeriod = () => {
    const result = movePlannerVisiblePeriod(session, {
      direction: 'previous',
      viewMode,
      surfaceMode,
      timelineRangeDays,
    });
    setSession(result.session);
    runPlannerCalendarCommand(calendarRef.current, result.command);
  };

  const goToNextPeriod = () => {
    const result = movePlannerVisiblePeriod(session, {
      direction: 'next',
      viewMode,
      surfaceMode,
      timelineRangeDays,
    });
    setSession(result.session);
    runPlannerCalendarCommand(calendarRef.current, result.command);
  };

  const goToToday = () => {
    const result = movePlannerVisiblePeriod(session, {
      direction: 'today',
      viewMode,
      surfaceMode,
      timelineRangeDays,
    });
    setSession(result.session);
    runPlannerCalendarCommand(calendarRef.current, result.command);
  };

  const goToDate = (year: number, month: number, day: number) => {
    const result = pickPlannerDate(session, {
      year,
      month,
      day,
      viewMode,
      surfaceMode,
    });
    setSession(result.session);
    runPlannerCalendarCommand(calendarRef.current, result.command);
  };
  const openPlannerEventCreate = useCallback(
    (range?: PlannerEventDraftRange) => {
      setSession((current) => openPlannerEventCreateSession(current, range));
    },
    [],
  );
  const openPlannerEventEdit = useCallback((eventId: string) => {
    setSession((current) => openPlannerEventEditSession(current, eventId));
  }, []);
  const closePlannerEventModal = useCallback(() => {
    const hasEventQueryIntent = Boolean(searchParams.get('event'));
    setSession((current) => {
      const result = closePlannerEventModalSession(current, {
        hasEventQueryIntent,
      });
      return result.session;
    });
    if (hasEventQueryIntent) {
      const nextParams = new URLSearchParams(searchParams);
      nextParams.delete('event');
      setSearchParams(nextParams, { replace: true });
    }
  }, [searchParams, setSearchParams]);

  const [actionError, setActionError] = useState<string | null>(null);
  useEffect(() => {
    if (!actionError) return;
    const id = window.setTimeout(() => setActionError(null), 4000);
    return () => window.clearTimeout(id);
  }, [actionError]);

  const handleDatesSet = useCallback(
    (nextState: {
      view: UnifiedCalendarView;
      currentDate: Date;
      rangeStart: Date;
      rangeEnd: Date;
    }) => {
      const result = applyCalendarDatesSet(session, nextState);
      setViewMode((currentMode) =>
        currentMode === result.viewMode ? currentMode : result.viewMode,
      );
      setSession(result.session);
    },
    [session],
  );

  const handleEventClick = (event: CalendarEvent) => {
    // miy: only planner events are listed; meeting previews and PMS task
    // panels belong to other apps.
    const action = resolvePlannerCalendarEventClick(event);
    if (action.type === 'openPlannerEvent') {
      openPlannerEventEdit(action.eventId);
    } else if (action.type === 'missingTaskList') {
      setActionError(t('planner.taskLocationMissing'));
    }
  };

  const handleCalendarEventClick = (
    event: CalendarEvent,
    anchorEl: HTMLElement,
  ) => {
    const group = isPlannerDayGroupId(event.id)
      ? monthDayGroups?.groups.get(event.id)
      : undefined;
    if (!group) {
      handleEventClick(event);
      return;
    }
    if (group.items.length === 1) {
      handleEventClick(group.items[0]);
      return;
    }
    setOpenDayGroup({ group, anchor: anchorEl.getBoundingClientRect() });
  };
  const closeDayGroup = useCallback(() => setOpenDayGroup(null), []);

  const timeFormatter = new Intl.DateTimeFormat(i18n.language, {
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
    timeZone,
  });
  const renderMonthChip = (event: CalendarEvent): ReactNode => {
    const group = isPlannerDayGroupId(event.id)
      ? monthDayGroups?.groups.get(event.id)
      : undefined;
    if (!event.allDay) {
      return (
        <span className="planner-chip planner-chip--timed">
          <span
            className="planner-chip__dot"
            style={{ backgroundColor: event.color }}
          />
          <span className="planner-chip__time">
            {timeFormatter.format(new Date(event.start))}
          </span>
          <span className="planner-chip__title">{event.title}</span>
        </span>
      );
    }
    const isDue = event.sourceType === 'pms_due';
    const count = group?.items.length ?? 1;
    return (
      <span
        className={cn('planner-chip', isDue && 'planner-chip--due')}
        style={{
          backgroundColor: tint(event.color, isDue ? 22 : 16),
        }}
      >
        {isDue ? (
          <CircleAlert
            size={14}
            className="planner-chip__icon"
            style={{ color: event.color }}
          />
        ) : (
          <span
            className="planner-chip__dot"
            style={{ backgroundColor: event.color }}
          />
        )}
        {group ? (
          <>
            <span className="planner-chip__label">
              {t(
                isDue ? 'planner.summary.dueCount' : 'planner.summary.pmsCount',
                { count },
              )}
            </span>
            {count === 1 ? (
              <span className="planner-chip__title planner-chip__title--muted">
                {group.items[0].title}
              </span>
            ) : null}
          </>
        ) : (
          <span className="planner-chip__title">{event.title}</span>
        )}
      </span>
    );
  };

  // Insights follow today's week while it is on screen, otherwise the week
  // of the browsed date, so the panel never reads outside the fetched range.
  const insightAnchor =
    today >= rangeStart && today < rangeEnd ? today : calendarState.currentDate;
  const insightWeek = getPlannerWeekRange(insightAnchor);
  const weekItems = groupPlannerEventsBySource(
    visibleEvents.map(withWorkspacePrefix),
    insightWeek.start,
    insightWeek.end,
  );
  const busyDays = rankPlannerBusyDays(
    visibleEvents,
    new Date(viewYear, viewMonth, 1),
    new Date(viewYear, viewMonth + 1, 1),
    3,
  );
  // Insights follow the sidebar filter and search like the calendar does.
  const upcomingEvents = listPlannerUpcomingEvents(visibleEvents, today, 4).map(
    withWorkspacePrefix,
  );
  const showDate = (date: Date, mode: PlannerViewMode) => {
    setMode(mode);
    goToDate(date.getFullYear(), date.getMonth(), date.getDate());
  };

  const scheduleWorkflow = createPlannerCalendarScheduleWorkflow({
    // miy: the workflow keeps AI-DO's token check; runtime.api carries the
    // credential (always present in a frame), so the adapters close over it.
    token: 'runtime',
    adapters: {
      updatePlannerEvent: (_token, eventId, payload) =>
        updatePlannerEvent(token, eventId, payload),
      // Meetings and PMS tasks are other apps' data: never listed here.
      updateMeeting: () =>
        Promise.reject(new Error(t('planner.meetingMoveFailed'))),
      updateTask: () =>
        Promise.reject(new Error(t('planner.taskScheduleMoveFailed'))),
    },
    messages: {
      meetingAllDayDisallowed: t('planner.meetingAllDayDisallowed'),
      taskAllDayOnly: t('planner.taskAllDayOnly'),
      eventMoveFailed: t('planner.eventMoveFailed'),
      meetingMoveFailed: t('planner.meetingMoveFailed'),
      taskScheduleMoveFailed: t('planner.taskScheduleMoveFailed'),
    },
    refresh,
    setActionError,
  });

  const handleEventDrop = async (
    event: CalendarEvent,
    newStartIso: string,
    newEndIso: string,
    newAllDay: boolean,
    revert: () => void,
  ) => {
    await scheduleWorkflow.drop(
      event,
      newStartIso,
      newEndIso,
      newAllDay,
      revert,
    );
  };

  const handleEventResize = async (
    event: CalendarEvent,
    newEndIso: string,
    revert: () => void,
  ) => {
    await scheduleWorkflow.resize(event, newEndIso, revert);
  };
  // miy: without the meeting app a selection always creates a planner event
  // (AI-DO's path when meetings were disabled).
  const handleDateSelect = useCallback(
    (range: {
      start: Date;
      end: Date;
      allDay: boolean;
      anchor: { x: number; y: number } | null;
    }) => {
      openPlannerEventCreate({
        start: range.start,
        end: range.end,
        allDay: range.allDay,
      });
    },
    [openPlannerEventCreate],
  );
  const handlePlannerEventSaved = useCallback(() => {
    closePlannerEventModal();
    refresh();
  }, [closePlannerEventModal, refresh]);
  const handlePlannerEventDeleted = useCallback(() => {
    closePlannerEventModal();
    refresh();
  }, [closePlannerEventModal, refresh]);
  // Only one placement renders at a time (sidebar or collapsed header), so the
  // shared outside-click ref stays unambiguous.
  const renderCreateMenu = (compact: boolean) => (
    <div ref={createMenuRef} className="relative w-full">
      <button
        type="button"
        onClick={() => setSession(toggleCreateMenu)}
        aria-haspopup="menu"
        aria-expanded={session.createMenuOpen}
        aria-label={compact ? t('planner.sidebar.create') : undefined}
        className={cn(
          'flex items-center bg-app-accent text-app-accent-fg shadow-sm transition-colors hover:bg-app-accent-hover',
          compact
            ? 'size-10 justify-center rounded-full'
            : 'app-text-control h-12 w-full gap-3 rounded-xl pl-4 pr-3',
        )}
      >
        <Plus size={20} />
        {compact ? null : (
          <>
            <span className="flex-1 text-left font-semibold">
              {t('planner.sidebar.create')}
            </span>
            <span className="flex h-6 items-center border-l border-app-accent-fg/25 pl-3">
              <ChevronDown size={16} />
            </span>
          </>
        )}
      </button>
      {session.createMenuOpen ? (
        <div className="absolute left-0 top-full z-30 mt-2 w-52 rounded-lg border border-app-border bg-app-surface py-1 shadow-xl">
          <button
            type="button"
            onClick={() => openPlannerEventCreate()}
            className="app-text-control-sm flex w-full items-center gap-2 px-3 py-2 text-left text-app-ink transition-colors hover:bg-app-surface-hover"
          >
            <Plus size={14} className="text-app-ink/45" />
            <span>{t('planner.event')}</span>
          </button>
        </div>
      ) : null}
    </div>
  );

  return (
    <LazyMotion features={domAnimation}>
      <m.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        className="relative flex h-full flex-col bg-app-surface-sidebar"
      >
        <header className="flex flex-wrap items-center gap-x-2 gap-y-2 px-3 py-2">
          <button
            type="button"
            onClick={() => setSidebarOpen((open) => !open)}
            aria-label={t('planner.sidebar.toggle')}
            aria-expanded={sidebarOpen}
            className="flex size-10 items-center justify-center rounded-full text-app-ink/70 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
          >
            <Menu size={20} />
          </button>
          <div className="mr-4 flex items-center gap-2 pr-2 lg:w-[200px]">
            <span className="flex size-9 items-center justify-center rounded-lg bg-app-accent text-app-accent-fg">
              <CalendarDays size={20} />
            </span>
            <h1 className="text-[22px] leading-none text-app-ink">
              {t('planner.planner')}
            </h1>
          </div>
          {sidebarOpen ? null : renderCreateMenu(true)}
          <button
            type="button"
            onClick={goToToday}
            className="app-text-control h-9 rounded-full border border-app-border bg-app-surface px-5 text-app-ink transition-colors hover:bg-app-surface-hover"
          >
            {t('planner.today')}
          </button>
          <div ref={pickerRef} className="relative flex items-center">
            <button
              type="button"
              onClick={goToPreviousPeriod}
              aria-label={t('planner.previousPeriod')}
              className="flex size-9 items-center justify-center rounded-full text-app-ink/70 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
            >
              <ChevronLeft size={20} />
            </button>
            <button
              type="button"
              onClick={goToNextPeriod}
              aria-label={t('planner.nextPeriod')}
              className="flex size-9 items-center justify-center rounded-full text-app-ink/70 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
            >
              <ChevronRight size={20} />
            </button>
            <button
              type="button"
              onClick={() =>
                session.pickerOpen ? setSession(closePicker) : openPicker()
              }
              aria-haspopup="dialog"
              aria-expanded={session.pickerOpen}
              className="ml-2 flex h-10 items-center gap-1.5 rounded-md px-2 text-[22px] font-semibold text-app-ink tabular-nums transition-colors hover:bg-app-surface-hover"
            >
              {formatPlannerHeading(
                viewMode,
                calendarState.currentDate,
                i18n.language,
                surfaceMode,
                calendarState.rangeStart,
                calendarState.rangeEnd,
              )}
              <ChevronDown size={18} className="text-app-ink/55" />
            </button>
            {holidayGapYears.length > 0 ? (
              <span
                role="note"
                title={t('planner.holidays.unavailableDetail', {
                  years: holidayGapYears.join(', '),
                })}
                className="app-text-caption ml-2 inline-flex items-center gap-1 rounded-full border border-app-border bg-app-surface px-2.5 py-1 text-app-ink/55"
              >
                <CircleAlert size={13} aria-hidden="true" />
                {t('planner.holidays.unavailable')}
              </span>
            ) : null}
            {session.pickerOpen ? (
              <DatePickerPopover
                pickerYear={session.pickerYear}
                pickerMonth={session.pickerMonth}
                viewYear={viewYear}
                viewMonth={viewMonth}
                selectedDate={calendarState.currentDate.getDate()}
                today={today}
                onPrev={() =>
                  setSession((current) => movePickerMonth(current, -1))
                }
                onNext={() =>
                  setSession((current) => movePickerMonth(current, 1))
                }
                onPickToday={() => {
                  goToToday();
                  setSession(closePicker);
                }}
                onPickDate={goToDate}
              />
            ) : null}
          </div>
          <div className="ml-auto flex flex-wrap items-center gap-3">
            {searchOpen ? (
              <div className="flex h-9 items-center gap-1 rounded-full border border-app-border bg-app-surface pl-3 pr-1">
                <Search size={16} className="text-app-ink/45" />
                <input
                  autoFocus
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Escape') {
                      setSearchQuery('');
                      setSearchOpen(false);
                    }
                  }}
                  aria-label={t('planner.search.label')}
                  placeholder={t('planner.search.placeholder')}
                  className="app-text-control-sm w-44 bg-transparent text-app-ink outline-none placeholder:text-app-ink/45"
                />
                <button
                  type="button"
                  onClick={() => {
                    setSearchQuery('');
                    setSearchOpen(false);
                  }}
                  aria-label={t('planner.search.close')}
                  className="flex size-7 items-center justify-center rounded-full text-app-ink/55 hover:bg-app-surface-hover hover:text-app-ink"
                >
                  <X size={14} />
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={() => setSearchOpen(true)}
                aria-label={t('planner.search.label')}
                className="flex size-9 items-center justify-center rounded-full text-app-ink/70 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
              >
                <Search size={18} />
              </button>
            )}
            <div className="flex items-center rounded-md border border-app-border bg-app-surface-sidebar p-1">
                {(['Month', 'Week', 'Day', 'Agenda'] as const).map((mode) => (
                  <button
                    key={mode}
                    type="button"
                    onClick={() => setMode(mode)}
                    aria-pressed={viewMode === mode}
                    className={cn(
                      'app-text-control-sm rounded px-3 py-1 transition-all',
                      viewMode === mode
                        ? 'bg-app-surface-hover text-app-ink shadow-sm'
                        : 'text-app-ink/55 hover:text-app-ink',
                    )}
                  >
                    {t(VIEW_MODE_LABEL_KEYS[mode])}
                  </button>
                ))}
            </div>
          </div>
        </header>

        {actionError ? (
          <div
            role="alert"
            className="mx-4 mb-2 rounded-md border border-app-danger/40 bg-app-danger/10 px-4 py-2 app-text-caption text-app-danger"
          >
            {actionError}
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 pb-3 pl-3 pr-3">
          {sidebarOpen ? (
            <PlannerSidebar
              createSlot={renderCreateMenu(false)}
              miniMonth={{
                pickerYear: sidebarCursor.year,
                pickerMonth: sidebarCursor.month,
                viewYear,
                viewMonth,
                selectedDate: calendarState.currentDate.getDate(),
                today,
                onPrev: () => moveSidebarCursor(-1),
                onNext: () => moveSidebarCursor(1),
                onPickDate: goToDate,
              }}
              sources={sidebarSources}
              onToggleSource={toggleSource}
              onToggleAll={toggleAllSources}
            />
          ) : null}
          <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-2xl bg-app-surface">
            {error ? (
              <div className="flex flex-1 items-center justify-center p-8">
                <div className="space-y-2 text-center">
                  <p className="app-text-body text-app-danger">{error}</p>
                  <p className="app-text-caption text-app-ink/55">
                    {t('planner.loadRetry')}
                  </p>
                </div>
              </div>
            ) : (
              <div className="relative flex-1">
                {loading ? (
                  <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center bg-app-bg/40">
                    <div className="app-text-caption text-app-ink/55">
                      {t('common:feedback.loading')}
                    </div>
                  </div>
                ) : null}
                <UnifiedCalendar
                    ref={calendarRef}
                    events={calendarEvents}
                    initialView={VIEW_MODE_TO_FC[viewMode]}
                    initialDate={calendarState.currentDate}
                    onDatesSet={handleDatesSet}
                    onDateSelect={handleDateSelect}
                    onEventClick={handleCalendarEventClick}
                    onEventDrop={handleEventDrop}
                    onEventResize={handleEventResize}
                    timeZone={timeZone}
                    locale={i18n.language}
                    dayMaxEvents
                    weekNumbers={false}
                    plainDayNumbers
                    eventContent={isMonthView ? renderMonthChip : undefined}
                    moreLinkContent={(count) => `+${count}`}
                    isEventEditable={isPlannerEventEditable}
                    className="unified-calendar--google"
                  />
              </div>
            )}
          </div>
          <PlannerInsightsPanel
            locale={i18n.language}
            weekStart={insightWeek.start}
            weekEnd={insightWeek.end}
            weekItems={weekItems}
            busyDays={busyDays}
            upcoming={upcomingEvents}
            onPickDay={(date) => showDate(date, 'Day')}
            onPickWeek={(date) => showDate(date, 'Week')}
            onOpenEvent={handleEventClick}
            onViewAll={() => showDate(today, 'Agenda')}
            sources={PLANNER_SOURCES}
          />
        </div>

        {openDayGroup ? (
          <DayGroupPopover
            group={openDayGroup.group}
            anchor={openDayGroup.anchor}
            locale={i18n.language}
            onPick={(item) => {
              setOpenDayGroup(null);
              handleEventClick(item);
            }}
            onClose={closeDayGroup}
          />
        ) : null}

        <PlannerEventModal
          isOpen={plannerEventModalOpen}
          onClose={closePlannerEventModal}
          eventId={plannerEventId}
          initialRange={plannerEventRange}
          onSaved={handlePlannerEventSaved}
          onDeleted={handlePlannerEventDeleted}
        />
      </m.div>
    </LazyMotion>
  );
}
