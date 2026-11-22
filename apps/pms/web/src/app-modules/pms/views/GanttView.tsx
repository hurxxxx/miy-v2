import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ComponentRef,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import {
  ChevronLeft,
  ChevronRight,
  CalendarDays,
  ChevronDown,
  ChevronsDownUp,
  ChevronsLeft,
  ChevronsRight,
  ChevronsUpDown,
  CornerDownRight,
  GitBranch,
  Palette,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  Button,
  RangeSlider,
  RangeSliderRange,
  RangeSliderThumb,
  RangeSliderTrack,
} from '@miy/ui';
import { cn } from '../../../lib/utils';
import type {
  PmsMilestone,
  PmsTask,
  PmsTaskListMember,
  PmsTaskListStatus,
} from '../api/pms-api';
import {
  addGanttDays,
  applyGanttScheduleOverride,
  diffGanttDays,
  getGanttBarStyle,
  getGanttDragPreviewRange,
  getGanttStatusBarStyle,
  getGanttTaskDateRange,
  parseGanttDateOnly,
  toGanttDateOnly,
  type GanttDragMode,
  type GanttScheduleOverride,
} from './gantt-view-model';
import {
  GANTT_SCALES,
  GANTT_TODAY_LEAD_DAYS,
  buildGanttTimeline,
  getDefaultGanttScale,
  getGanttDayWidth,
  type GanttScale,
} from './gantt-timeline-model';
import { getStatusLabel } from './pms-constants';
import { StatusIconGlyph } from './StatusIcon';
import { TaskAssigneeStack } from './TaskAssigneeStack';
import { buildTaskHierarchy, sortTasksByHierarchy } from './pms-task-hierarchy';

// Sticky left panel: the task name column takes what the assignee column
// (w-24) leaves. The panel width includes its right border.
const TASK_PANEL_WIDTH = 384;
const MAX_GANTT_RANGE_MONTHS = 12;
// The range slider shows six months on each side of a maximum range.
const GANTT_SLIDER_LEAD_MONTHS = 6;
const GANTT_SLIDER_WINDOW_MONTHS =
  MAX_GANTT_RANGE_MONTHS + GANTT_SLIDER_LEAD_MONTHS * 2;
const GANTT_MONTH_PRESETS = [1, 2, 3, 6, 12] as const;
// Header cells narrower than these (a range edge cutting a week or month)
// stay blank instead of showing a clipped label.
const GANTT_GROUP_LABEL_MIN_WIDTH = 72;
const GANTT_UNIT_LABEL_MIN_WIDTH = 20;
const GANTT_SCALE_LABEL_KEYS: Record<GanttScale, string> = {
  day: 'pms.gantt.scaleDay',
  week: 'pms.gantt.scaleWeek',
  month: 'pms.gantt.scaleMonth',
};
// A thick, always-visible horizontal scrollbar for the chart only; the shared
// custom-scrollbar is 6px light gray and users missed it (#44).
const GANTT_SCROLLBAR_CLASS =
  '[&::-webkit-scrollbar]:size-3 [&::-webkit-scrollbar-track]:bg-app-surface-sidebar [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:border-2 [&::-webkit-scrollbar-thumb]:border-solid [&::-webkit-scrollbar-thumb]:border-transparent [&::-webkit-scrollbar-thumb]:bg-app-ink/30 [&::-webkit-scrollbar-thumb]:bg-clip-padding [&::-webkit-scrollbar-thumb:hover]:bg-app-ink/45';
const GANTT_DATE_INPUT_CLASS =
  'app-text-body-sm h-9 w-full rounded-md border border-app-border bg-app-surface px-2 text-app-ink outline-none transition focus:border-app-accent focus:ring-2 focus:ring-app-accent/20';

type GanttInputDateRange = {
  fromDate: string;
  toDate: string;
};

type GanttRangeOverflowAnchor = 'end' | 'start';

function getMonthDateRange(value = new Date()): GanttInputDateRange {
  return getMonthSpanDateRange(1, value);
}

function getMonthSpanDateRange(
  months: number,
  value = new Date(),
): GanttInputDateRange {
  const start = new Date(value.getFullYear(), value.getMonth(), 1);
  const end = new Date(value.getFullYear(), value.getMonth() + months, 0);
  return {
    fromDate: toGanttDateOnly(start),
    toDate: toGanttDateOnly(end),
  };
}

function getDaysInMonth(year: number, monthIndex: number): number {
  return new Date(year, monthIndex + 1, 0).getDate();
}

function startOfLocalMonth(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), 1);
}

function addCalendarMonths(value: Date, months: number): Date {
  const targetYear = value.getFullYear();
  const targetMonth = value.getMonth() + months;
  const firstOfTarget = new Date(targetYear, targetMonth, 1);
  const targetDay = Math.min(
    value.getDate(),
    getDaysInMonth(firstOfTarget.getFullYear(), firstOfTarget.getMonth()),
  );
  return new Date(
    firstOfTarget.getFullYear(),
    firstOfTarget.getMonth(),
    targetDay,
  );
}

function getMaxRangeEnd(start: Date): Date {
  return addGanttDays(addCalendarMonths(start, MAX_GANTT_RANGE_MONTHS), -1);
}

function getMaxRangeStart(end: Date): Date {
  return addCalendarMonths(addGanttDays(end, 1), -MAX_GANTT_RANGE_MONTHS);
}

function getGanttSliderWindowStart(anchor: Date): Date {
  return startOfLocalMonth(
    addCalendarMonths(anchor, -GANTT_SLIDER_LEAD_MONTHS),
  );
}

function getGanttSliderWindowEnd(start: Date): Date {
  return addGanttDays(addCalendarMonths(start, GANTT_SLIDER_WINDOW_MONTHS), -1);
}

function normalizeInputDateRange(
  range: GanttInputDateRange,
  fallback: GanttInputDateRange,
  overflowAnchor: GanttRangeOverflowAnchor = 'start',
): GanttInputDateRange {
  const fallbackStart = parseGanttDateOnly(fallback.fromDate) ?? new Date();
  const fallbackEnd = parseGanttDateOnly(fallback.toDate) ?? fallbackStart;
  const parsedStart = parseGanttDateOnly(range.fromDate) ?? fallbackStart;
  const parsedEnd = parseGanttDateOnly(range.toDate) ?? fallbackEnd;
  const start = parsedStart <= parsedEnd ? parsedStart : parsedEnd;
  const end = parsedStart <= parsedEnd ? parsedEnd : parsedStart;
  const maxEnd = getMaxRangeEnd(start);
  if (end > maxEnd && overflowAnchor === 'end') {
    return {
      fromDate: toGanttDateOnly(getMaxRangeStart(end)),
      toDate: toGanttDateOnly(end),
    };
  }

  return {
    fromDate: toGanttDateOnly(start),
    toDate: toGanttDateOnly(end > maxEnd ? maxEnd : end),
  };
}

function buildVisibleDates(start: Date, end: Date): Date[] {
  const dates: Date[] = [];
  for (
    let cursor = new Date(
      start.getFullYear(),
      start.getMonth(),
      start.getDate(),
    );
    cursor <= end;
    cursor = new Date(
      cursor.getFullYear(),
      cursor.getMonth(),
      cursor.getDate() + 1,
    )
  ) {
    dates.push(cursor);
  }
  return dates;
}

function clampGanttOffset(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function snapGanttSliderOffset(
  value: number,
  sliderStart: Date,
  sliderEnd: Date,
): number {
  const maxOffset = diffGanttDays(sliderEnd, sliderStart);
  let bestOffset = clampGanttOffset(value, 0, maxOffset);
  let bestDistance = Number.POSITIVE_INFINITY;

  for (const date of buildVisibleDates(sliderStart, sliderEnd)) {
    const snapTarget = date.getDate() === 1 || date.getDay() === 1;
    if (!snapTarget) continue;
    const offset = diffGanttDays(date, sliderStart);
    const distance = Math.abs(offset - value);
    if (distance < bestDistance) {
      bestDistance = distance;
      bestOffset = offset;
    }
  }

  return bestDistance <= 2 ? bestOffset : clampGanttOffset(value, 0, maxOffset);
}

type DragState = {
  deltaDays: number;
  end: Date;
  mode: GanttDragMode;
  taskId: string;
  start: Date;
  startX: number;
};

type GanttSliderRangeDragState = {
  endOffset: number;
  pointerOffset: number;
  startOffset: number;
};

export const GanttView = ({
  milestones = [],
  canEdit = false,
  canEditTask,
  completedItemsControlDisabled = false,
  members,
  onSelectIssue,
  onOpenStatusSettings,
  onShowCompletedItemsChange,
  showCompletedItems = false,
  tasks,
  onUpdateIssue,
  taskListStatuses,
  taskSourceLabels,
}: {
  /** miy: milestones with a due date are marked on that day. */
  milestones?: readonly PmsMilestone[];
  canEdit?: boolean;
  /** Narrows canEdit per task, e.g. when tasks from several lists share one chart. */
  canEditTask?: (task: PmsTask) => boolean;
  completedItemsControlDisabled?: boolean;
  members?: readonly PmsTaskListMember[];
  onSelectIssue?: (task: PmsTask) => void;
  onOpenStatusSettings?: () => void;
  onShowCompletedItemsChange?: (showCompletedItems: boolean) => void;
  showCompletedItems?: boolean;
  tasks: PmsTask[];
  onUpdateIssue?: (
    taskId: string,
    payload: Record<string, unknown>,
  ) => Promise<void> | void;
  taskListStatuses?: PmsTaskListStatus[];
  /** Small label after a task name, such as the list it comes from. */
  taskSourceLabels?: ReadonlyMap<string, string>;
}) => {
  const { i18n, t } = useTranslation('apps');
  const [visibleRange, setVisibleRange] = useState(getMonthDateRange);
  const [draftRange, setDraftRange] = useState(getMonthDateRange);
  const [customRangeOpen, setCustomRangeOpen] = useState(false);
  // The default range is the current month, which opens in day columns.
  const [scale, setScale] = useState<GanttScale>('day');
  const [sliderBaseDateOnly, setSliderBaseDateOnly] = useState(() =>
    toGanttDateOnly(getGanttSliderWindowStart(new Date())),
  );
  const [dragging, setDragging] = useState<DragState | null>(null);
  const [collapsedTaskIds, setCollapsedTaskIds] = useState<Set<string>>(
    new Set(),
  );
  const [pendingSchedules, setPendingSchedules] = useState<
    Map<string, GanttScheduleOverride>
  >(new Map());
  const [scrollViewportWidth, setScrollViewportWidth] = useState(0);
  const scrollViewportRef = useRef<HTMLDivElement | null>(null);
  const rangeSliderRootRef = useRef<ComponentRef<typeof RangeSlider> | null>(
    null,
  );
  const sliderRangeDragRef = useRef<GanttSliderRangeDragState | null>(null);
  const suppressNextBarClickRef = useRef(false);

  const normalizedVisibleRange = useMemo(
    () => normalizeInputDateRange(visibleRange, getMonthDateRange()),
    [visibleRange],
  );
  const { monthTitle, visibleEnd, visibleStart } = useMemo(() => {
    const start =
      parseGanttDateOnly(normalizedVisibleRange.fromDate) ?? new Date();
    const end = parseGanttDateOnly(normalizedVisibleRange.toDate) ?? start;
    const sameMonth =
      start.getFullYear() === end.getFullYear() &&
      start.getMonth() === end.getMonth() &&
      start.getDate() === 1 &&
      end.getDate() === getDaysInMonth(end.getFullYear(), end.getMonth());
    const fullMonthFormatter = new Intl.DateTimeFormat(i18n.language, {
      month: 'long',
      year: 'numeric',
    });
    const dateFormatter = new Intl.DateTimeFormat(i18n.language, {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
    return {
      monthTitle: sameMonth
        ? fullMonthFormatter.format(start)
        : `${dateFormatter.format(start)} - ${dateFormatter.format(end)}`,
      visibleEnd: end,
      visibleStart: start,
    };
  }, [i18n.language, normalizedVisibleRange]);

  const timeline = useMemo(
    () => buildGanttTimeline(visibleStart, visibleEnd, scale),
    [scale, visibleEnd, visibleStart],
  );
  const groupStartOffsets = useMemo(
    () => new Set(timeline.groups.map((group) => group.dayOffset)),
    [timeline],
  );
  const todayDateOnly = toGanttDateOnly(new Date());
  const availableTimelineWidth = Math.max(
    0,
    scrollViewportWidth - TASK_PANEL_WIDTH,
  );
  const dayWidth = getGanttDayWidth({
    availableWidth: availableTimelineWidth,
    dayCount: timeline.dayCount,
    scale,
  });
  const timelineWidth = timeline.dayCount * dayWidth;
  const chartWidth = Math.max(
    scrollViewportWidth,
    TASK_PANEL_WIDTH + timelineWidth,
  );
  const todayDayOffset = useMemo(() => {
    const today = parseGanttDateOnly(todayDateOnly);
    if (!today || today < visibleStart || today > visibleEnd) return null;
    return diffGanttDays(today, visibleStart);
  }, [todayDateOnly, visibleEnd, visibleStart]);
  // miy reinforcement: a marker per milestone due inside the shown range.
  const milestoneMarkers = useMemo(
    () =>
      milestones.flatMap((milestone) => {
        const due = parseGanttDateOnly(milestone.due_date ?? '');
        if (!due || due < visibleStart || due > visibleEnd) return [];
        return [
          {
            id: milestone.id,
            title: milestone.title,
            dayOffset: diffGanttDays(due, visibleStart),
          },
        ];
      }),
    [milestones, visibleEnd, visibleStart],
  );
  const todayOffset =
    todayDayOffset === null
      ? null
      : `${todayDayOffset * dayWidth + dayWidth / 2}px`;
  const activeMonthPreset = useMemo(() => {
    if (visibleStart.getDate() !== 1) return null;
    return (
      GANTT_MONTH_PRESETS.find((months) => {
        const presetEnd = new Date(
          visibleStart.getFullYear(),
          visibleStart.getMonth() + months,
          0,
        );
        return toGanttDateOnly(presetEnd) === toGanttDateOnly(visibleEnd);
      }) ?? null
    );
  }, [visibleEnd, visibleStart]);
  const monthLabelFormatter = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        month: 'short',
      }),
    [i18n.language],
  );
  const timelineLabelFormatters = useMemo(
    () => ({
      groupMonth: new Intl.DateTimeFormat(i18n.language, {
        month: 'long',
        year: 'numeric',
      }),
      groupYear: new Intl.DateTimeFormat(i18n.language, { year: 'numeric' }),
      week: new Intl.DateTimeFormat(i18n.language, {
        day: 'numeric',
        month: 'numeric',
      }),
    }),
    [i18n.language],
  );
  function formatTimelineUnit(start: Date): string {
    if (scale === 'day') return String(start.getDate());
    if (scale === 'week') return timelineLabelFormatters.week.format(start);
    return monthLabelFormatter.format(start);
  }
  function formatTimelineGroup(start: Date): string {
    return scale === 'month'
      ? timelineLabelFormatters.groupYear.format(start)
      : timelineLabelFormatters.groupMonth.format(start);
  }
  const normalizedDraftRange = useMemo(
    () => normalizeInputDateRange(draftRange, normalizedVisibleRange),
    [draftRange, normalizedVisibleRange],
  );
  const draftStart =
    parseGanttDateOnly(normalizedDraftRange.fromDate) ?? visibleStart;
  const draftEnd =
    parseGanttDateOnly(normalizedDraftRange.toDate) ?? draftStart;
  const sliderBaseDate = useMemo(
    () =>
      parseGanttDateOnly(sliderBaseDateOnly) ?? startOfLocalMonth(visibleStart),
    [sliderBaseDateOnly, visibleStart],
  );
  const sliderEndDate = useMemo(
    () => getGanttSliderWindowEnd(sliderBaseDate),
    [sliderBaseDate],
  );
  const sliderMaxOffset = diffGanttDays(sliderEndDate, sliderBaseDate);
  const sliderStartOffset = clampGanttOffset(
    diffGanttDays(draftStart, sliderBaseDate),
    0,
    sliderMaxOffset,
  );
  const sliderEndOffset = clampGanttOffset(
    diffGanttDays(draftEnd, sliderBaseDate),
    sliderStartOffset,
    sliderMaxOffset,
  );
  const sliderMarks = useMemo(
    () =>
      buildVisibleDates(sliderBaseDate, sliderEndDate)
        .filter((date) => date.getDate() === 1 || date.getDay() === 1)
        .map((date) => ({
          date,
          label: date.getDate() === 1 ? monthLabelFormatter.format(date) : null,
          major: date.getDate() === 1,
          offset: diffGanttDays(date, sliderBaseDate),
        })),
    [monthLabelFormatter, sliderBaseDate, sliderEndDate],
  );
  const draftRangeMaxToDate = useMemo(() => {
    const draftStart = parseGanttDateOnly(draftRange.fromDate);
    return draftStart ? toGanttDateOnly(getMaxRangeEnd(draftStart)) : undefined;
  }, [draftRange.fromDate]);

  function commitVisibleRange(
    range: GanttInputDateRange,
    options: {
      closeCustom?: boolean;
      keepScale?: boolean;
      syncSliderBase?: boolean;
    } = {},
  ) {
    const nextRange = normalizeInputDateRange(range, visibleRange);
    setDraftRange(nextRange);
    setVisibleRange(nextRange);
    // A newly picked range opens in the scale that fits it; moving the same
    // range back and forth keeps whatever scale the user chose.
    if (!options.keepScale) {
      const nextStart = parseGanttDateOnly(nextRange.fromDate);
      const nextEnd = parseGanttDateOnly(nextRange.toDate);
      if (nextStart && nextEnd) {
        setScale(getDefaultGanttScale(diffGanttDays(nextEnd, nextStart) + 1));
      }
    }
    if (options.syncSliderBase !== false) {
      const nextStart = parseGanttDateOnly(nextRange.fromDate);
      if (nextStart) {
        setSliderBaseDateOnly(
          toGanttDateOnly(getGanttSliderWindowStart(nextStart)),
        );
      }
    }
    if (options.closeCustom !== false) setCustomRangeOpen(false);
  }

  function shiftVisibleRange(months: number) {
    const normalizedRange = normalizeInputDateRange(
      visibleRange,
      getMonthDateRange(),
    );
    const currentStart =
      parseGanttDateOnly(normalizedRange.fromDate) ?? new Date();
    const currentEnd =
      parseGanttDateOnly(normalizedRange.toDate) ?? currentStart;
    const nextRange = {
      fromDate: toGanttDateOnly(addCalendarMonths(currentStart, months)),
      toDate: toGanttDateOnly(addCalendarMonths(currentEnd, months)),
    };
    commitVisibleRange(nextRange, { keepScale: true });
  }

  function applyMonthPreset(months: number) {
    const start = parseGanttDateOnly(normalizedVisibleRange.fromDate);
    commitVisibleRange(getMonthSpanDateRange(months, start ?? new Date()));
  }

  function resetVisibleRange() {
    commitVisibleRange(getMonthDateRange());
  }

  function toggleCustomRangeControls() {
    if (customRangeOpen) {
      setCustomRangeOpen(false);
      return;
    }
    setDraftRange(normalizedVisibleRange);
    setSliderBaseDateOnly(
      toGanttDateOnly(getGanttSliderWindowStart(visibleStart)),
    );
    setCustomRangeOpen(true);
  }

  function handleDateRangeSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    commitVisibleRange(draftRange);
  }

  function updateDraftRangeFromSliderOffsets({
    endOffset,
    overflowAnchor = 'start',
    preserveSpan = false,
    snap,
    startOffset,
  }: {
    endOffset: number;
    overflowAnchor?: GanttRangeOverflowAnchor;
    preserveSpan?: boolean;
    snap: boolean;
    startOffset: number;
  }) {
    const clampedStartOffset = clampGanttOffset(
      startOffset,
      0,
      sliderMaxOffset,
    );
    const clampedEndOffset = clampGanttOffset(endOffset, 0, sliderMaxOffset);
    const rangeSpan = Math.abs(clampedEndOffset - clampedStartOffset);
    let orderedStartOffset = Math.min(clampedStartOffset, clampedEndOffset);
    let orderedEndOffset = Math.max(clampedStartOffset, clampedEndOffset);

    if (preserveSpan) {
      const nextStartOffset = snap
        ? snapGanttSliderOffset(
            orderedStartOffset,
            sliderBaseDate,
            sliderEndDate,
          )
        : orderedStartOffset;
      orderedStartOffset = clampGanttOffset(
        nextStartOffset,
        0,
        Math.max(0, sliderMaxOffset - rangeSpan),
      );
      orderedEndOffset = orderedStartOffset + rangeSpan;
    } else if (snap) {
      const nextStartOffset = snapGanttSliderOffset(
        orderedStartOffset,
        sliderBaseDate,
        sliderEndDate,
      );
      const nextEndOffset = snapGanttSliderOffset(
        orderedEndOffset,
        sliderBaseDate,
        sliderEndDate,
      );
      orderedStartOffset = Math.min(nextStartOffset, nextEndOffset);
      orderedEndOffset = Math.max(nextStartOffset, nextEndOffset);
    }

    setDraftRange(
      normalizeInputDateRange(
        {
          fromDate: toGanttDateOnly(
            addGanttDays(sliderBaseDate, orderedStartOffset),
          ),
          toDate: toGanttDateOnly(
            addGanttDays(sliderBaseDate, orderedEndOffset),
          ),
        },
        normalizedDraftRange,
        overflowAnchor,
      ),
    );
  }

  function getSliderOverflowAnchor(value: number[]): GanttRangeOverflowAnchor {
    const nextStartOffset = value[0] ?? sliderStartOffset;
    const nextEndOffset = value[1] ?? sliderEndOffset;
    return Math.abs(nextEndOffset - sliderEndOffset) >
      Math.abs(nextStartOffset - sliderStartOffset)
      ? 'end'
      : 'start';
  }

  function handleSliderRangeChange(value: number[]) {
    updateDraftRangeFromSliderOffsets({
      endOffset: value[1] ?? sliderEndOffset,
      overflowAnchor: getSliderOverflowAnchor(value),
      snap: false,
      startOffset: value[0] ?? sliderStartOffset,
    });
  }

  function handleSliderRangeCommit(value: number[]) {
    updateDraftRangeFromSliderOffsets({
      endOffset: value[1] ?? sliderEndOffset,
      overflowAnchor: getSliderOverflowAnchor(value),
      snap: true,
      startOffset: value[0] ?? sliderStartOffset,
    });
  }

  function getSliderPointerOffset(clientX: number): number | null {
    if (!Number.isFinite(clientX)) return null;
    const rect = rangeSliderRootRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0) return null;
    const pointerRatio = clampGanttOffset(
      (clientX - rect.left) / rect.width,
      0,
      1,
    );
    return Math.round(pointerRatio * sliderMaxOffset);
  }

  function updateDraftRangeFromSliderRangeDrag(clientX: number, snap: boolean) {
    const dragState = sliderRangeDragRef.current;
    const pointerOffset = getSliderPointerOffset(clientX);
    if (!dragState || pointerOffset === null) return;

    const rangeSpan = dragState.endOffset - dragState.startOffset;
    const nextStartOffset = clampGanttOffset(
      dragState.startOffset + pointerOffset - dragState.pointerOffset,
      0,
      Math.max(0, sliderMaxOffset - rangeSpan),
    );
    updateDraftRangeFromSliderOffsets({
      endOffset: nextStartOffset + rangeSpan,
      preserveSpan: true,
      snap,
      startOffset: nextStartOffset,
    });
  }

  function handleSliderRangeDragMove(event: PointerEvent) {
    updateDraftRangeFromSliderRangeDrag(event.clientX, false);
  }

  function handleSliderRangeDragEnd(event: PointerEvent) {
    updateDraftRangeFromSliderRangeDrag(event.clientX, true);
    sliderRangeDragRef.current = null;
    window.removeEventListener('pointermove', handleSliderRangeDragMove);
  }

  function handleSliderRangePointerDown(
    event: ReactPointerEvent<HTMLSpanElement>,
  ) {
    if (event.button !== 0) return;
    const pointerOffset = getSliderPointerOffset(event.clientX);
    if (pointerOffset === null) return;

    event.preventDefault();
    event.stopPropagation();
    sliderRangeDragRef.current = {
      endOffset: sliderEndOffset,
      pointerOffset,
      startOffset: sliderStartOffset,
    };
    window.addEventListener('pointermove', handleSliderRangeDragMove);
    window.addEventListener('pointerup', handleSliderRangeDragEnd, {
      once: true,
    });
  }
  const hierarchy = useMemo(() => buildTaskHierarchy(tasks), [tasks]);
  const orderedTasks = useMemo(() => sortTasksByHierarchy(tasks), [tasks]);
  const editable = canEdit && Boolean(onUpdateIssue);
  const isTaskEditable = (task: PmsTask) =>
    editable && (canEditTask?.(task) ?? true);
  const taskIdSet = useMemo(
    () => new Set(tasks.map((task) => task.id)),
    [tasks],
  );
  const visibleOrderedTasks = useMemo(
    () =>
      orderedTasks.filter((task) => {
        let parent = hierarchy.get(task.id)?.parent ?? null;
        while (parent) {
          if (collapsedTaskIds.has(parent.id)) return false;
          parent = hierarchy.get(parent.id)?.parent ?? null;
        }
        return true;
      }),
    [collapsedTaskIds, hierarchy, orderedTasks],
  );
  const collapsibleTaskIds = useMemo(
    () =>
      orderedTasks
        .filter((task) => (hierarchy.get(task.id)?.childCount ?? 0) > 0)
        .map((task) => task.id),
    [hierarchy, orderedTasks],
  );
  const allSubtasksCollapsed =
    collapsibleTaskIds.length > 0 &&
    collapsibleTaskIds.every((taskId) => collapsedTaskIds.has(taskId));

  useEffect(() => {
    const element = scrollViewportRef.current;
    if (!element) return;

    const updateWidth = () => setScrollViewportWidth(element.clientWidth);
    updateWidth();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateWidth);
      return () => window.removeEventListener('resize', updateWidth);
    }

    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // A range that contains today opens scrolled to just before today.
  const initialScrollLeft =
    todayDayOffset === null
      ? 0
      : Math.max(0, (todayDayOffset - GANTT_TODAY_LEAD_DAYS[scale]) * dayWidth);
  useEffect(() => {
    scrollViewportRef.current?.scrollTo?.({ left: initialScrollLeft });
  }, [
    initialScrollLeft,
    normalizedVisibleRange.fromDate,
    normalizedVisibleRange.toDate,
  ]);

  useEffect(() => {
    if (!customRangeOpen) return;

    const nextDraftStart = parseGanttDateOnly(normalizedDraftRange.fromDate);
    const currentSliderStart = parseGanttDateOnly(sliderBaseDateOnly);
    if (!nextDraftStart || !currentSliderStart) return;

    const currentSliderEnd = getGanttSliderWindowEnd(currentSliderStart);
    if (
      nextDraftStart >= currentSliderStart &&
      nextDraftStart <= currentSliderEnd
    ) {
      return;
    }

    setSliderBaseDateOnly(
      toGanttDateOnly(getGanttSliderWindowStart(nextDraftStart)),
    );
  }, [customRangeOpen, normalizedDraftRange.fromDate, sliderBaseDateOnly]);

  useEffect(() => {
    setCollapsedTaskIds((current) => {
      const next = new Set<string>();
      for (const taskId of current) {
        if (taskIdSet.has(taskId)) next.add(taskId);
      }
      return next.size === current.size ? current : next;
    });
    setPendingSchedules((current) => {
      const next = new Map<string, GanttScheduleOverride>();
      for (const [taskId, schedule] of current) {
        if (taskIdSet.has(taskId)) next.set(taskId, schedule);
      }
      return next.size === current.size ? current : next;
    });
  }, [taskIdSet]);

  useEffect(() => {
    if (!dragging) return;

    const handleMouseMove = (event: MouseEvent) => {
      setDragging((current) =>
        current
          ? (() => {
              const deltaDays = Math.round(
                (event.clientX - current.startX) / dayWidth,
              );
              if (deltaDays !== 0) suppressNextBarClickRef.current = true;
              return {
                ...current,
                deltaDays,
              };
            })()
          : current,
      );
    };
    const handleMouseUp = () => {
      setDragging((current) => {
        if (current && current.deltaDays !== 0) {
          const { end: nextEnd, start: nextStart } =
            getGanttDragPreviewRange(current);
          const nextSchedule = {
            due_date: toGanttDateOnly(nextEnd),
            start_date: toGanttDateOnly(nextStart),
          };
          setPendingSchedules((schedules) => {
            const next = new Map(schedules);
            next.set(current.taskId, nextSchedule);
            return next;
          });
          void Promise.resolve(onUpdateIssue?.(current.taskId, nextSchedule))
            .catch(() => undefined)
            .finally(() => {
              setPendingSchedules((schedules) => {
                const next = new Map(schedules);
                const schedule = next.get(current.taskId);
                if (schedule === nextSchedule) next.delete(current.taskId);
                return next.size === schedules.size ? schedules : next;
              });
            });
        }
        return null;
      });
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [dayWidth, dragging, onUpdateIssue]);

  function handleBarMouseDown(
    event: React.MouseEvent,
    task: PmsTask,
    mode: GanttDragMode,
  ) {
    if (!isTaskEditable(task)) return;
    const issueDates = getGanttTaskDateRange(task);
    if (!issueDates) return;
    event.preventDefault();
    event.stopPropagation();
    suppressNextBarClickRef.current = false;
    setDragging({
      deltaDays: 0,
      end: issueDates.end,
      mode,
      taskId: task.id,
      start: issueDates.start,
      startX: event.clientX,
    });
  }

  function handleBarClick(event: React.MouseEvent, task: PmsTask) {
    if (suppressNextBarClickRef.current) {
      event.preventDefault();
      event.stopPropagation();
      suppressNextBarClickRef.current = false;
      return;
    }
    onSelectIssue?.(task);
  }

  function stopResizeHandleClick(event: React.MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
  }

  function toggleTaskCollapse(taskId: string) {
    setCollapsedTaskIds((current) => {
      const next = new Set(current);
      if (next.has(taskId)) next.delete(taskId);
      else next.add(taskId);
      return next;
    });
  }

  // Collapsing every parent leaves only the top-level tasks on the chart.
  function toggleAllSubtasks() {
    setCollapsedTaskIds(
      allSubtasksCollapsed ? new Set() : new Set(collapsibleTaskIds),
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-lg border border-app-border bg-app-surface">
      <div className="border-b border-app-border bg-app-surface-sidebar/30">
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 px-4 py-3">
          <div className="flex min-w-0 flex-wrap items-center gap-1">
            <CalendarDays size={16} className="mr-1 text-app-accent" />
            <Button
              aria-label={t('pms.gantt.previousYear')}
              onClick={() => shiftVisibleRange(-12)}
              size="icon"
              title={t('pms.gantt.previousYear')}
              variant="ghost"
            >
              <ChevronsLeft size={16} />
            </Button>
            <Button
              aria-label={t('pms.gantt.previousMonth')}
              onClick={() => shiftVisibleRange(-1)}
              size="icon"
              title={t('pms.gantt.previousMonth')}
              variant="ghost"
            >
              <ChevronLeft size={16} />
            </Button>
            <h3 className="app-text-title-md mx-2 min-w-0 truncate text-app-ink">
              {monthTitle}
            </h3>
            <Button
              aria-label={t('pms.gantt.nextMonth')}
              onClick={() => shiftVisibleRange(1)}
              size="icon"
              title={t('pms.gantt.nextMonth')}
              variant="ghost"
            >
              <ChevronRight size={16} />
            </Button>
            <Button
              aria-label={t('pms.gantt.nextYear')}
              onClick={() => shiftVisibleRange(12)}
              size="icon"
              title={t('pms.gantt.nextYear')}
              variant="ghost"
            >
              <ChevronsRight size={16} />
            </Button>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <div
              aria-label={t('pms.gantt.monthPresetsLabel')}
              className="flex flex-wrap gap-1"
              role="group"
            >
              {GANTT_MONTH_PRESETS.map((months) => (
                <Button
                  key={months}
                  onClick={() => applyMonthPreset(months)}
                  size="dense"
                  variant={
                    activeMonthPreset === months ? 'primary' : 'secondary'
                  }
                >
                  {t('pms.gantt.monthPreset', { count: months })}
                </Button>
              ))}
            </div>
            <div
              aria-label={t('pms.gantt.scaleLabel')}
              className="flex rounded-md border border-app-border p-0.5"
              role="group"
            >
              {GANTT_SCALES.map((option) => (
                <Button
                  key={option}
                  aria-pressed={scale === option}
                  className="h-6 px-2"
                  onClick={() => setScale(option)}
                  size="dense"
                  variant={scale === option ? 'primary' : 'ghost'}
                >
                  {t(GANTT_SCALE_LABEL_KEYS[option])}
                </Button>
              ))}
            </div>
            <div className="relative">
              <Button
                aria-controls="pms-gantt-custom-range-panel"
                aria-expanded={customRangeOpen}
                aria-haspopup="dialog"
                onClick={toggleCustomRangeControls}
                size="dense"
                variant={customRangeOpen ? 'primary' : 'secondary'}
              >
                {t('pms.gantt.customRange')}
              </Button>
              {customRangeOpen ? (
                <form
                  className="absolute right-0 top-full z-50 mt-2 flex w-[560px] max-w-[calc(100vw-2rem)] flex-col gap-4 rounded-lg border border-app-border bg-app-surface p-3 shadow-xl"
                  id="pms-gantt-custom-range-panel"
                  noValidate
                  onSubmit={handleDateRangeSubmit}
                >
                  <div className="grid gap-2 sm:grid-cols-2">
                    <label className="flex flex-col gap-1.5">
                      <span className="app-text-overline text-app-ink/55">
                        {t('pms.gantt.fromDateLabel')}
                      </span>
                      <input
                        className={GANTT_DATE_INPUT_CLASS}
                        max={draftRange.toDate || undefined}
                        onChange={(event) =>
                          setDraftRange((current) => ({
                            ...current,
                            fromDate: event.target.value,
                          }))
                        }
                        type="date"
                        value={draftRange.fromDate}
                      />
                    </label>
                    <label className="flex flex-col gap-1.5">
                      <span className="app-text-overline text-app-ink/55">
                        {t('pms.gantt.toDateLabel')}
                      </span>
                      <input
                        className={GANTT_DATE_INPUT_CLASS}
                        max={draftRangeMaxToDate}
                        min={draftRange.fromDate || undefined}
                        onChange={(event) =>
                          setDraftRange((current) => ({
                            ...current,
                            toDate: event.target.value,
                          }))
                        }
                        type="date"
                        value={draftRange.toDate}
                      />
                    </label>
                  </div>
                  <div
                    aria-label={t('pms.gantt.dateSliderLabel')}
                    className="rounded-md border border-app-border bg-app-surface-sidebar/40 px-3 pb-2 pt-3"
                    role="group"
                  >
                    <div className="flex items-center gap-3">
                      <span className="app-text-overline w-12 shrink-0 text-app-ink/55">
                        {t('pms.gantt.dateSliderLabel')}
                      </span>
                      <div className="relative h-14 min-w-0 flex-1">
                        {sliderMarks.map((mark) => {
                          const left =
                            sliderMaxOffset > 0
                              ? (mark.offset / sliderMaxOffset) * 100
                              : 0;
                          return (
                            <div
                              key={toGanttDateOnly(mark.date)}
                              className={cn(
                                'pointer-events-none absolute top-4 -translate-x-1/2 text-center',
                                mark.major ? 'z-10' : 'z-0',
                              )}
                              style={{ left: `${left}%` }}
                            >
                              <div
                                className={cn(
                                  'mx-auto w-px rounded-full bg-app-ink/20',
                                  mark.major ? 'h-5 bg-app-ink/45' : 'h-3',
                                )}
                              />
                              {mark.label ? (
                                <div className="mt-1 hidden whitespace-nowrap text-[10px] font-semibold leading-3 text-app-ink/55 sm:block">
                                  {mark.label}
                                </div>
                              ) : null}
                            </div>
                          );
                        })}
                        <RangeSlider
                          aria-label={t('pms.gantt.dateSliderLabel')}
                          className="relative z-20 flex h-10 w-full touch-none select-none items-center"
                          data-testid="pms-gantt-date-slider"
                          max={sliderMaxOffset}
                          min={0}
                          minStepsBetweenThumbs={0}
                          onValueChange={handleSliderRangeChange}
                          onValueCommit={handleSliderRangeCommit}
                          ref={rangeSliderRootRef}
                          step={1}
                          value={[sliderStartOffset, sliderEndOffset]}
                        >
                          <RangeSliderTrack className="relative h-2 grow rounded-full bg-app-border/70">
                            <RangeSliderRange
                              className="absolute h-full cursor-grab rounded-full bg-app-accent shadow-sm before:absolute before:-inset-y-4 before:inset-x-0 before:content-[''] active:cursor-grabbing"
                              data-testid="pms-gantt-range-drag-handle"
                              onPointerDown={handleSliderRangePointerDown}
                            />
                          </RangeSliderTrack>
                          <RangeSliderThumb
                            aria-label={t('pms.gantt.sliderStartLabel')}
                            className="block size-5 rounded-full border-[3px] border-app-accent bg-app-surface shadow-md transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-app-accent/30"
                          />
                          <RangeSliderThumb
                            aria-label={t('pms.gantt.sliderEndLabel')}
                            className="block size-5 rounded-full border-[3px] border-app-accent bg-app-surface shadow-md transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-app-accent/30"
                          />
                        </RangeSlider>
                      </div>
                    </div>
                  </div>
                  <div className="flex justify-end">
                    <Button size="dense" type="submit" variant="secondary">
                      {t('pms.gantt.applyDateRange')}
                    </Button>
                  </div>
                </form>
              ) : null}
            </div>
            <div className="flex items-center gap-1">
              {onShowCompletedItemsChange ? (
                <label
                  className={cn(
                    'app-text-control-sm flex h-7 shrink-0 items-center gap-2 rounded px-2 text-app-ink/70 transition-colors',
                    completedItemsControlDisabled
                      ? 'cursor-not-allowed opacity-50'
                      : 'cursor-pointer hover:bg-app-surface-hover hover:text-app-ink',
                  )}
                >
                  <input
                    checked={showCompletedItems}
                    className="size-3.5 rounded accent-app-accent"
                    disabled={completedItemsControlDisabled}
                    onChange={(event) =>
                      onShowCompletedItemsChange(event.target.checked)
                    }
                    type="checkbox"
                  />
                  {t('pms.list.showCompletedItems')}
                </label>
              ) : null}
              {onOpenStatusSettings ? (
                <Button
                  aria-label={t('pms.gantt.editStatusColors')}
                  onClick={onOpenStatusSettings}
                  size="icon"
                  title={t('pms.gantt.editStatusColors')}
                  variant="ghost"
                >
                  <Palette size={16} />
                </Button>
              ) : null}
              <Button onClick={resetVisibleRange} size="dense" variant="ghost">
                {t('pms.gantt.resetRange')}
              </Button>
            </div>
          </div>
        </div>
      </div>

      <div
        ref={scrollViewportRef}
        className={cn('flex-1 overflow-auto', GANTT_SCROLLBAR_CLASS)}
        data-testid="pms-gantt-scroll-viewport"
      >
        <div className="min-h-full" style={{ minWidth: `${chartWidth}px` }}>
          <div className="sticky top-0 z-40 flex border-b border-app-border bg-app-surface-sidebar/95 backdrop-blur">
            <div
              className="app-text-overline sticky left-0 z-50 flex shrink-0 border-r border-app-border bg-app-surface-sidebar/95 text-app-ink/55 backdrop-blur"
              style={{ width: `${TASK_PANEL_WIDTH}px` }}
            >
              <div className="flex min-w-0 flex-1 items-center justify-between gap-2 py-2 pl-4 pr-2">
                <span className="truncate">{t('pms.taskName')}</span>
                {collapsibleTaskIds.length > 0 ? (
                  <Button
                    className="shrink-0 gap-1"
                    onClick={toggleAllSubtasks}
                    size="dense"
                    variant="ghost"
                  >
                    {allSubtasksCollapsed ? (
                      <ChevronsUpDown size={14} />
                    ) : (
                      <ChevronsDownUp size={14} />
                    )}
                    {t(
                      allSubtasksCollapsed
                        ? 'pms.gantt.expandAll'
                        : 'pms.gantt.collapseAll',
                    )}
                  </Button>
                ) : null}
              </div>
              <div className="flex w-24 shrink-0 items-center border-l border-app-border px-2 py-2">
                <span className="truncate">{t('pms.list.assignee')}</span>
              </div>
            </div>
            <div
              className="relative flex shrink-0 flex-col"
              data-testid="pms-gantt-timeline-header"
              style={{ width: `${timelineWidth}px` }}
            >
              <div className="flex h-6 border-b border-app-border">
                {timeline.groups.map((group) => {
                  const groupWidth = group.dayCount * dayWidth;
                  return (
                    <div
                      key={toGanttDateOnly(group.start)}
                      className="app-text-micro flex shrink-0 items-center whitespace-nowrap border-r border-app-border font-semibold text-app-ink/60 last:border-r-0"
                      style={{ width: `${groupWidth}px` }}
                    >
                      {/* Sticks next to the task panel so the month or year
                          stays named while its columns scroll past. */}
                      {groupWidth >= GANTT_GROUP_LABEL_MIN_WIDTH ? (
                        <span
                          className="sticky px-2"
                          style={{ left: `${TASK_PANEL_WIDTH}px` }}
                        >
                          {formatTimelineGroup(group.start)}
                        </span>
                      ) : null}
                    </div>
                  );
                })}
              </div>
              <div className="flex h-7">
                {timeline.units.map((unit) => {
                  const isCurrent =
                    todayDayOffset !== null &&
                    todayDayOffset >= unit.dayOffset &&
                    todayDayOffset < unit.dayOffset + unit.dayCount;
                  return (
                    <div
                      key={toGanttDateOnly(unit.start)}
                      className={cn(
                        'app-text-micro flex shrink-0 items-center justify-center overflow-hidden whitespace-nowrap border-r border-app-border font-bold text-app-ink last:border-r-0',
                        isCurrent && 'bg-app-danger/10 text-app-danger',
                      )}
                      data-current={isCurrent || undefined}
                      style={{ width: `${unit.dayCount * dayWidth}px` }}
                    >
                      {unit.dayCount * dayWidth >= GANTT_UNIT_LABEL_MIN_WIDTH
                        ? formatTimelineUnit(unit.start)
                        : null}
                    </div>
                  );
                })}
              </div>
              {todayOffset ? (
                <div
                  aria-hidden="true"
                  className="pointer-events-none absolute bottom-0 top-0 z-20 w-0.5 bg-app-danger"
                  style={{ left: todayOffset }}
                />
              ) : null}
              {milestoneMarkers.map((marker) => (
                <span
                  key={marker.id}
                  aria-label={t('pms.milestoneSettings.calendarTitle', {
                    title: marker.title,
                  })}
                  className="absolute bottom-1 z-20 size-2.5 -translate-x-1/2 rotate-45 bg-violet-600"
                  data-testid="pms-gantt-milestone"
                  role="img"
                  style={{
                    left: `${marker.dayOffset * dayWidth + dayWidth / 2}px`,
                  }}
                  title={t('pms.milestoneSettings.calendarTitle', {
                    title: marker.title,
                  })}
                />
              ))}
            </div>
          </div>

          <div className="relative">
            {visibleOrderedTasks.map((task) => {
              const taskHierarchy = hierarchy.get(task.id);
              const depth = taskHierarchy?.depth ?? 0;
              const childCount = taskHierarchy?.childCount ?? 0;
              const collapsed = collapsedTaskIds.has(task.id);
              const effectiveTask = applyGanttScheduleOverride(
                task,
                pendingSchedules.get(task.id),
              );
              const barStyle = getGanttBarStyle({
                dayWidth,
                dragging,
                visibleEnd,
                visibleStart,
                task: effectiveTask,
              });
              const statusLabel = getStatusLabel(task.status, taskListStatuses);
              const taskEditable = isTaskEditable(task);
              const sourceLabel = taskSourceLabels?.get(task.id);
              const statusBarStyle = getGanttStatusBarStyle({
                status: task.status,
                taskListStatuses,
              });
              const taskDetailLabel = t('pms.list.viewDetails', {
                reference: task.title,
              });
              const taskNameContent = (
                <>
                  <StatusIconGlyph
                    label={statusLabel}
                    status={task.status}
                    taskListStatuses={taskListStatuses}
                  />
                  <span
                    className={cn(
                      'app-text-body-sm min-w-0 truncate text-app-ink transition-colors group-hover/task-name:text-app-accent',
                      childCount > 0 ? 'font-semibold' : 'font-medium',
                    )}
                  >
                    {task.title}
                  </span>
                  {childCount > 0 ? (
                    <span
                      className="app-text-micro inline-flex shrink-0 items-center gap-1 rounded bg-app-surface-sidebar px-1.5 py-0.5 font-semibold text-app-ink/45"
                      title={t('pms.gantt.childTasks', {
                        count: childCount,
                      })}
                    >
                      <GitBranch size={10} />
                      {childCount}
                    </span>
                  ) : null}
                  {sourceLabel ? (
                    <span
                      className="app-text-micro max-w-[7rem] shrink-0 truncate text-app-ink/40"
                      data-testid={`pms-gantt-source-${task.id}`}
                      title={sourceLabel}
                    >
                      {sourceLabel}
                    </span>
                  ) : null}
                </>
              );
              return (
                <div
                  key={task.id}
                  className={cn(
                    'group/gantt-row flex border-b border-app-border transition-colors hover:bg-app-surface-hover',
                    depth > 0 && 'bg-app-surface-sidebar/20',
                  )}
                >
                  <div
                    className={cn(
                      'sticky left-0 z-30 flex shrink-0 border-r border-app-border transition-colors group-hover/gantt-row:bg-app-surface-hover',
                      depth > 0 ? 'bg-app-surface-sidebar' : 'bg-app-surface',
                    )}
                    style={{ width: `${TASK_PANEL_WIDTH}px` }}
                  >
                    <div
                      className="flex min-w-0 flex-1 items-center gap-2 px-4 py-3"
                      style={{
                        paddingLeft: `${16 + Math.min(depth, 5) * 18}px`,
                      }}
                    >
                      {childCount > 0 ? (
                        <button
                          aria-label={t(
                            collapsed
                              ? 'pms.gantt.expandSubtasks'
                              : 'pms.gantt.collapseSubtasks',
                            { title: task.title },
                          )}
                          className="flex size-[18px] shrink-0 items-center justify-center rounded text-app-ink/45 transition-colors hover:bg-app-surface-hover hover:text-app-ink"
                          onClick={(event) => {
                            event.stopPropagation();
                            toggleTaskCollapse(task.id);
                          }}
                          title={t(
                            collapsed
                              ? 'pms.gantt.expandSubtasks'
                              : 'pms.gantt.collapseSubtasks',
                            { title: task.title },
                          )}
                          type="button"
                        >
                          {collapsed ? (
                            <ChevronRight size={14} />
                          ) : (
                            <ChevronDown size={14} />
                          )}
                        </button>
                      ) : depth > 0 ? (
                        <CornerDownRight
                          size={13}
                          className="shrink-0 text-app-ink/35"
                        />
                      ) : (
                        <span className="w-[13px] shrink-0" />
                      )}
                      {onSelectIssue ? (
                        <button
                          className="group/task-name flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded text-left transition-colors hover:text-app-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-app-accent/40"
                          onClick={() => onSelectIssue(effectiveTask)}
                          title={taskDetailLabel}
                          type="button"
                        >
                          {taskNameContent}
                        </button>
                      ) : (
                        <div className="flex min-w-0 flex-1 items-center gap-2">
                          {taskNameContent}
                        </div>
                      )}
                    </div>
                    <div
                      className="app-text-caption flex w-24 min-w-0 shrink-0 items-center border-l border-app-border px-2"
                      data-testid={`pms-gantt-assignee-${task.id}`}
                    >
                      <TaskAssigneeStack
                        members={members}
                        showName
                        sizeClassName="size-5"
                        task={task}
                        unassignedLabel={t('pms.taskDetail.unassigned')}
                        visibleCount={1}
                      />
                    </div>
                  </div>
                  <div
                    className="relative h-12 shrink-0"
                    style={{ width: `${timelineWidth}px` }}
                  >
                    {barStyle ? (
                      <button
                        type="button"
                        aria-label={taskDetailLabel}
                        className={cn(
                          // px-1 keeps a one-day bar inside MIN_DAY_WIDTH; wider
                          // padding pushed it past the last column.
                          'app-text-micro group/gantt-bar absolute top-1/2 z-10 flex h-6 -translate-y-1/2 items-center overflow-hidden rounded-full border bg-app-accent px-1 font-bold text-app-accent-fg shadow-sm',
                          taskEditable && 'cursor-grab active:cursor-grabbing',
                          !taskEditable && onSelectIssue && 'cursor-pointer',
                          dragging?.taskId === task.id &&
                            'ring-2 ring-app-accent/40',
                        )}
                        onClick={(event) =>
                          handleBarClick(event, effectiveTask)
                        }
                        onMouseDown={(event) =>
                          handleBarMouseDown(event, effectiveTask, 'move')
                        }
                        title={taskDetailLabel}
                        style={{ ...barStyle, ...statusBarStyle }}
                      >
                        {taskEditable ? (
                          <span
                            aria-label={t('pms.gantt.resizeStart', {
                              title: task.title,
                            })}
                            className="absolute inset-y-0 left-0 w-2 cursor-ew-resize rounded-l-full bg-white/0 transition-colors group-hover/gantt-bar:bg-white/35"
                            onMouseDown={(event) =>
                              handleBarMouseDown(
                                event,
                                effectiveTask,
                                'resize-start',
                              )
                            }
                            onClick={stopResizeHandleClick}
                            role="separator"
                            title={t('pms.gantt.resizeStart', {
                              title: task.title,
                            })}
                          />
                        ) : null}
                        <span className="min-w-0 truncate px-1">
                          {statusLabel}
                        </span>
                        {taskEditable ? (
                          <span
                            aria-label={t('pms.gantt.resizeEnd', {
                              title: task.title,
                            })}
                            className="absolute inset-y-0 right-0 w-2 cursor-ew-resize rounded-r-full bg-white/0 transition-colors group-hover/gantt-bar:bg-white/35"
                            onMouseDown={(event) =>
                              handleBarMouseDown(
                                event,
                                effectiveTask,
                                'resize-end',
                              )
                            }
                            onClick={stopResizeHandleClick}
                            role="separator"
                            title={t('pms.gantt.resizeEnd', {
                              title: task.title,
                            })}
                          />
                        ) : null}
                      </button>
                    ) : null}
                  </div>
                </div>
              );
            })}
            {/* Column lines and today are drawn once for all rows; a cell per
              day per row is thousands of nodes on a one-year chart. */}
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-y-0"
              data-testid="pms-gantt-grid"
              style={{
                left: `${TASK_PANEL_WIDTH}px`,
                width: `${timelineWidth}px`,
              }}
            >
              {scale === 'day' && todayDayOffset !== null ? (
                <div
                  className="absolute inset-y-0 bg-app-danger/5"
                  style={{
                    left: `${todayDayOffset * dayWidth}px`,
                    width: `${dayWidth}px`,
                  }}
                />
              ) : null}
              {timeline.units.slice(1).map((unit) => (
                <div
                  key={toGanttDateOnly(unit.start)}
                  className={cn(
                    'absolute inset-y-0 w-px',
                    groupStartOffsets.has(unit.dayOffset)
                      ? 'bg-app-border'
                      : 'bg-app-border/60',
                  )}
                  style={{ left: `${unit.dayOffset * dayWidth - 1}px` }}
                />
              ))}
              {todayOffset ? (
                <div
                  className="absolute inset-y-0 z-20 w-0.5 bg-app-danger"
                  style={{ left: todayOffset }}
                />
              ) : null}
              {milestoneMarkers.map((marker) => (
                <div
                  key={marker.id}
                  className="absolute inset-y-0 z-10 border-l border-dashed border-violet-600/60"
                  style={{
                    left: `${marker.dayOffset * dayWidth + dayWidth / 2}px`,
                  }}
                />
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
