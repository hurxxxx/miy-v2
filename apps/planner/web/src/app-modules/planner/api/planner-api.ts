import { apiFetchJsonWithMappedError } from '../../../platform/api/client';
import { readStoredTimeZonePreference } from '../../../platform/time/time-utils';
import type { AppApi } from '../../../platform/runtime/runtime-context';

export interface PlannerEvent {
  id: string;
  ownerId: string;
  ownerName: string;
  title: string;
  description: string;
  location: string;
  timeZone: string;
  allDay: boolean;
  startHasTime: boolean;
  endHasTime: boolean;
  start: string;
  end: string;
  createdAt: string;
  updatedAt: string;
}

export interface PlannerEventsResponse {
  items: PlannerEvent[];
}

export interface PlannerEventCreateInput {
  title: string;
  description?: string;
  location?: string;
  allDay?: boolean;
  start: string;
  end: string;
}

export type PlannerEventUpdateInput = Partial<PlannerEventCreateInput>;

export class PlannerApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = 'PlannerApiError';
  }
}

// miy: `api` (runtime.api, the app's own API) replaces AI-DO's bearer token.
// Paths are app relative; the gateway serves them under /api as before.
async function request<T>(
  path: string,
  api: AppApi,
  init: RequestInit = {},
): Promise<T> {
  return apiFetchJsonWithMappedError<T>(
    path,
    api,
    init,
    (error) =>
      new PlannerApiError(
        error.status,
        error.message || `Planner request failed with ${error.status}.`,
      ),
  );
}

export function listPlannerEvents(
  api: AppApi,
  optionsOrLegacyWorkspace: { from?: string; to?: string } | string = {},
  legacyOptions: { from?: string; to?: string } = {},
): Promise<PlannerEventsResponse> {
  const options =
    typeof optionsOrLegacyWorkspace === 'string'
      ? legacyOptions
      : optionsOrLegacyWorkspace;
  const params = new URLSearchParams();
  if (options.from) params.set('from', options.from);
  if (options.to) params.set('to', options.to);
  const query = params.toString();
  return request<PlannerEventsResponse>(
    `/v1/planner/events${query ? `?${query}` : ''}`,
    api,
  );
}

export function getPlannerEvent(
  api: AppApi,
  eventId: string,
): Promise<PlannerEvent> {
  return request<PlannerEvent>(
    `/v1/planner/events/${encodeURIComponent(eventId)}`,
    api,
  );
}

/**
 * AI-DO stored the event in the user's profile time zone. In miy the zone is
 * the portal display setting, so it travels with the create request.
 */
export function createPlannerEvent(
  api: AppApi,
  payload: PlannerEventCreateInput,
  timeZone = readStoredTimeZonePreference(),
): Promise<PlannerEvent> {
  const zone = new URLSearchParams({ timeZone });
  return request<PlannerEvent>(`/v1/planner/events?${zone.toString()}`, api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function updatePlannerEvent(
  api: AppApi,
  eventId: string,
  payload: PlannerEventUpdateInput,
): Promise<PlannerEvent> {
  return request<PlannerEvent>(`/v1/planner/events/${encodeURIComponent(eventId)}`, api, {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
}

export function deletePlannerEvent(
  api: AppApi,
  eventId: string,
): Promise<void> {
  return request<void>(`/v1/planner/events/${encodeURIComponent(eventId)}`, api, {
    method: 'DELETE',
  });
}
