import type { paths } from '../../../api.generated';
import { apiFetchJsonWithMappedError } from '../../../platform/api/client';
import { readStoredTimeZonePreference } from '../../../platform/time/time-utils';
import type { AppApi } from '../../../platform/runtime/runtime-context';

const eventsPath = '/v1/planner/events';
const eventPath = '/v1/planner/events/{event_id}';
type Events = paths[typeof eventsPath];
type Event = paths[typeof eventPath];
type EventsQuery = NonNullable<Events['get']['parameters']['query']>;
type CreateQuery = NonNullable<Events['post']['parameters']['query']>;
export type PlannerEvent = Event['get']['responses'][200]['content']['application/json'];
export type PlannerEventsResponse = Events['get']['responses'][200]['content']['application/json'];
export type PlannerEventCreateInput = Events['post']['requestBody']['content']['application/json'];
export type PlannerEventUpdateInput = Event['patch']['requestBody']['content']['application/json'];

function plannerEventPath({ event_id }: Event['get']['parameters']['path']): string {
  return eventPath.replace('{event_id}', encodeURIComponent(event_id));
}

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
  optionsOrLegacyWorkspace: EventsQuery | string = {},
  legacyOptions: EventsQuery = {},
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
    `${eventsPath}${query ? `?${query}` : ''}`,
    api,
  );
}

export function getPlannerEvent(
  api: AppApi,
  eventId: string,
): Promise<PlannerEvent> {
  return request<PlannerEvent>(plannerEventPath({ event_id: eventId }), api);
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
  const query = { timeZone } satisfies CreateQuery;
  const zone = new URLSearchParams(query);
  return request<PlannerEvent>(`${eventsPath}?${zone.toString()}`, api, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function updatePlannerEvent(
  api: AppApi,
  eventId: string,
  payload: PlannerEventUpdateInput,
): Promise<PlannerEvent> {
  return request<PlannerEvent>(plannerEventPath({ event_id: eventId }), api, {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
}

export function deletePlannerEvent(
  api: AppApi,
  eventId: string,
): Promise<void> {
  return request<void>(plannerEventPath({ event_id: eventId }), api, {
    method: 'DELETE',
  });
}
