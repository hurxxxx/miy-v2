import type { PlannerEvent, PlannerEventCreateInput, PlannerEventUpdateInput } from '../api/planner-api';
import {
  addLocalCalendarDays,
  addNativeDateInputDays,
  formatNativeDateInputValue,
  formatNativeDateTimeInputValue,
  parseNativeDateInputValue,
} from '../../../platform/time/native-date-input';
import { formatZonedWallTime, resolveZonedWallTime } from '../../../platform/time/zoned-wall-time';
import { defaultTimedRange } from './planner-event-default-time';

export interface PlannerEventModalRange {
  start: Date;
  end: Date;
  allDay: boolean;
}

export interface PlannerEventModalDraft {
  allDay: boolean;
  startDateValue: string;
  startTimeValue: string;
  endDateValue: string;
  endTimeValue: string;
}

export interface PlannerEventModalState {
  timeZone?: string;
  loading: boolean;
  saving: boolean;
  deleting: boolean;
  error: string | null;
  title: string;
  description: string;
  location: string;
  allDay: boolean;
  startDateValue: string;
  startTimeValue: string;
  endDateValue: string;
  endTimeValue: string;
}

export type PlannerEventModalAction =
  | { type: 'loadStart' }
  | { type: 'loadSuccess'; event: PlannerEvent }
  | { type: 'loadFailure'; error: string }
  | { type: 'setError'; error: string | null }
  | { type: 'saveStart' }
  | { type: 'saveDone' }
  | { type: 'deleteStart' }
  | { type: 'deleteDone' }
  | { type: 'setTitle'; title: string }
  | { type: 'setDescription'; description: string }
  | { type: 'setLocation'; location: string }
  | { type: 'setStartDateValue'; startDateValue: string }
  | { type: 'setStartTimeValue'; startTimeValue: string }
  | { type: 'setEndDateValue'; endDateValue: string }
  | { type: 'setEndTimeValue'; endTimeValue: string }
  | { type: 'setAllDay'; allDay: boolean };

export const formatDateInputValue = formatNativeDateInputValue;
export const formatDateTimeInputValue = formatNativeDateTimeInputValue;
export const parseLocalDate = parseNativeDateInputValue;
export const addLocalDays = addLocalCalendarDays;

export function incrementYmd(value: string): string {
  return addNativeDateInputDays(value, 1);
}

export function decrementYmd(value: string): string {
  return addNativeDateInputDays(value, -1);
}

function wallTime(value: Date, timeZone?: string): string {
  return timeZone ? formatZonedWallTime(value, timeZone) : formatDateTimeInputValue(value);
}

function dateInputValueFromApiValue(value: string, timeZone?: string): string {
  return value.includes('T') ? wallTime(new Date(value), timeZone).slice(0, 10) : value.slice(0, 10);
}

function timeInputValueFromApiValue(value: string, hasTime: boolean, timeZone?: string): string {
  return hasTime ? wallTime(new Date(value), timeZone).slice(11) : '';
}

function instant(date: string, time: string, timeZone?: string): Date {
  const value = `${date}T${time}`;
  return timeZone ? resolveZonedWallTime(value, timeZone) : new Date(value);
}

function combineDateAndOptionalTime(dateValue: string, timeValue: string, timeZone?: string): string {
  const normalizedTime = timeValue.trim();
  return normalizedTime ? instant(dateValue, normalizedTime, timeZone).toISOString() : dateValue;
}

function effectiveStartDateTime(state: PlannerEventModalState): Date | null {
  if (!state.startDateValue) return null;
  if (state.startTimeValue.trim()) {
    return instant(state.startDateValue, state.startTimeValue.trim(), state.timeZone);
  }
  return instant(state.startDateValue, '00:00', state.timeZone);
}

function effectiveEndDateTime(state: PlannerEventModalState): Date | null {
  if (!state.endDateValue) return null;
  if (state.endTimeValue.trim()) {
    return instant(state.endDateValue, state.endTimeValue.trim(), state.timeZone);
  }
  return new Date(instant(incrementYmd(state.endDateValue), '00:00', state.timeZone).getTime() - 1);
}

export function buildDraftFromRange(
  range?: PlannerEventModalRange | null,
  now?: Date,
  timeZone?: string,
): PlannerEventModalDraft {
  if (range?.allDay) {
    return {
      allDay: true,
      startDateValue: wallTime(range.start, timeZone).slice(0, 10),
      startTimeValue: '',
      endDateValue: decrementYmd(wallTime(range.end, timeZone).slice(0, 10)),
      endTimeValue: '',
    };
  }
  const { start, end } = range ?? defaultTimedRange(now, timeZone);
  const startValue = wallTime(start, timeZone);
  const endValue = wallTime(end, timeZone);
  return {
    allDay: false,
    startDateValue: startValue.slice(0, 10),
    startTimeValue: startValue.slice(11),
    endDateValue: endValue.slice(0, 10),
    endTimeValue: endValue.slice(11),
  };
}

export function plannerEventToDraft(
  event: PlannerEvent,
  timeZone?: string,
): PlannerEventModalDraft {
  if (event.allDay) {
    return {
      allDay: true,
      startDateValue: event.start,
      startTimeValue: '',
      endDateValue: decrementYmd(event.end),
      endTimeValue: '',
    };
  }
  const startHasTime = event.startHasTime ?? event.start.includes('T');
  const endHasTime = event.endHasTime ?? event.end.includes('T');
  return {
    allDay: false,
    startDateValue: dateInputValueFromApiValue(event.start, timeZone),
    startTimeValue: timeInputValueFromApiValue(event.start, startHasTime, timeZone),
    endDateValue: dateInputValueFromApiValue(event.end, timeZone),
    endTimeValue: timeInputValueFromApiValue(event.end, endHasTime, timeZone),
  };
}

export function plannerEventModalSessionKey(
  eventId: string | null | undefined,
  initialRange: PlannerEventModalRange | null | undefined,
): string {
  if (eventId) {
    return `edit:${eventId}`;
  }
  if (!initialRange) {
    return 'create:default';
  }
  return [
    'create',
    initialRange.allDay ? 'all-day' : 'timed',
    initialRange.start.toISOString(),
    initialRange.end.toISOString(),
  ].join(':');
}

export function initialPlannerEventModalState(
  initialRange: PlannerEventModalRange | null | undefined,
  now?: Date,
  timeZone?: string,
): PlannerEventModalState {
  const draft = buildDraftFromRange(initialRange, now, timeZone);
  return {
    timeZone,
    loading: false,
    saving: false,
    deleting: false,
    error: null,
    title: '',
    description: '',
    location: '',
    allDay: draft.allDay,
    startDateValue: draft.startDateValue,
    startTimeValue: draft.startTimeValue,
    endDateValue: draft.endDateValue,
    endTimeValue: draft.endTimeValue,
  };
}

function buildAllDayToggleDraft(
  state: PlannerEventModalState,
  nextAllDay: boolean,
): PlannerEventModalDraft {
  if (nextAllDay === state.allDay) {
    return {
      allDay: state.allDay,
      startDateValue: state.startDateValue,
      startTimeValue: state.startTimeValue,
      endDateValue: state.endDateValue,
      endTimeValue: state.endTimeValue,
    };
  }
  if (nextAllDay) {
    return {
      allDay: true,
      startDateValue: state.startDateValue,
      startTimeValue: '',
      endDateValue: state.endDateValue,
      endTimeValue: '',
    };
  }

  const defaultRange = defaultTimedRange(undefined, state.timeZone);
  const startDate =
    state.startDateValue || wallTime(defaultRange.start, state.timeZone).slice(0, 10);
  const endDate = state.endDateValue || startDate;
  return {
    allDay: false,
    startDateValue: startDate,
    startTimeValue:
      state.startTimeValue || wallTime(defaultRange.start, state.timeZone).slice(11),
    endDateValue: endDate,
    endTimeValue:
      state.endTimeValue || wallTime(defaultRange.end, state.timeZone).slice(11),
  };
}

export function plannerEventModalReducer(
  state: PlannerEventModalState,
  action: PlannerEventModalAction,
): PlannerEventModalState {
  switch (action.type) {
    case 'loadStart':
      return { ...state, loading: true, error: null };
    case 'loadSuccess': {
      const eventDraft = plannerEventToDraft(action.event, state.timeZone);
      return {
        ...state,
        loading: false,
        title: action.event.title,
        description: action.event.description,
        location: action.event.location,
        allDay: eventDraft.allDay,
        startDateValue: eventDraft.startDateValue,
        startTimeValue: eventDraft.startTimeValue,
        endDateValue: eventDraft.endDateValue,
        endTimeValue: eventDraft.endTimeValue,
      };
    }
    case 'loadFailure':
      return { ...state, loading: false, error: action.error };
    case 'setError':
      return { ...state, error: action.error };
    case 'saveStart':
      return { ...state, saving: true, error: null };
    case 'saveDone':
      return { ...state, saving: false };
    case 'deleteStart':
      return { ...state, deleting: true, error: null };
    case 'deleteDone':
      return { ...state, deleting: false };
    case 'setTitle':
      return { ...state, title: action.title };
    case 'setDescription':
      return { ...state, description: action.description };
    case 'setLocation':
      return { ...state, location: action.location };
    case 'setStartDateValue':
      return { ...state, startDateValue: action.startDateValue };
    case 'setStartTimeValue':
      return { ...state, startTimeValue: action.startTimeValue };
    case 'setEndDateValue':
      return { ...state, endDateValue: action.endDateValue };
    case 'setEndTimeValue':
      return { ...state, endTimeValue: action.endTimeValue };
    case 'setAllDay':
      return { ...state, ...buildAllDayToggleDraft(state, action.allDay) };
    default:
      return state;
  }
}

export function canSave(state: PlannerEventModalState): boolean {
  if (!state.title.trim()) return false;
  if (!state.startDateValue || !state.endDateValue) return false;
  const startDate = parseLocalDate(state.startDateValue);
  const endDate = parseLocalDate(state.endDateValue);
  if (Number.isNaN(startDate.getTime()) || Number.isNaN(endDate.getTime())) {
    return false;
  }
  if (endDate < startDate) return false;
  if (state.allDay) return true;
  const start = effectiveStartDateTime(state);
  const end = effectiveEndDateTime(state);
  if (!start || !end || !(end > start)) return false;
  return (
    (!state.startTimeValue || wallTime(start, state.timeZone) === `${state.startDateValue}T${state.startTimeValue}`) &&
    (!state.endTimeValue || wallTime(end, state.timeZone) === `${state.endDateValue}T${state.endTimeValue}`)
  );
}

export function buildPlannerEventSavePayload(
  state: PlannerEventModalState,
): PlannerEventCreateInput {
  return {
    title: state.title.trim(),
    description: state.description.trim(),
    location: state.location.trim(),
    allDay: state.allDay,
    start: state.allDay
      ? state.startDateValue
      : combineDateAndOptionalTime(state.startDateValue, state.startTimeValue, state.timeZone),
    end: state.allDay
      ? incrementYmd(state.endDateValue)
      : combineDateAndOptionalTime(state.endDateValue, state.endTimeValue, state.timeZone),
  };
}

// miy: unsaved-input reporting (runtime.setNavigationState). AI-DO had no
// navigation guard; the dialog compares its fields with how it was opened.
export type PlannerEventModalEditableFields = Pick<
  PlannerEventModalState,
  | 'title'
  | 'description'
  | 'location'
  | 'allDay'
  | 'startDateValue'
  | 'startTimeValue'
  | 'endDateValue'
  | 'endTimeValue'
>;

export function plannerEventModalEditableFields(
  state: PlannerEventModalState,
): PlannerEventModalEditableFields {
  return {
    title: state.title,
    description: state.description,
    location: state.location,
    allDay: state.allDay,
    startDateValue: state.startDateValue,
    startTimeValue: state.startTimeValue,
    endDateValue: state.endDateValue,
    endTimeValue: state.endTimeValue,
  };
}

export function isPlannerEventModalEdited(
  baseline: PlannerEventModalEditableFields,
  state: PlannerEventModalState,
): boolean {
  const current = plannerEventModalEditableFields(state);
  return (Object.keys(baseline) as Array<keyof PlannerEventModalEditableFields>)
    .some((key) => baseline[key] !== current[key]);
}

export function buildPlannerEventUpdatePayload(
  state: PlannerEventModalState,
  baseline: PlannerEventModalEditableFields,
): PlannerEventUpdateInput {
  const payload = buildPlannerEventSavePayload(state);
  const update: PlannerEventUpdateInput = {};
  for (const field of ['title', 'description', 'location'] as const) {
    if (state[field] !== baseline[field]) update[field] = payload[field];
  }
  const modeChanged = state.allDay !== baseline.allDay;
  if (modeChanged || state.startDateValue !== baseline.startDateValue || state.startTimeValue !== baseline.startTimeValue) {
    update.start = payload.start;
  }
  if (modeChanged || state.endDateValue !== baseline.endDateValue || state.endTimeValue !== baseline.endTimeValue) {
    update.end = payload.end;
  }
  if (modeChanged) update.allDay = payload.allDay;
  return update;
}
