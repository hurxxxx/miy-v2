import type { components } from '../../../api.generated';
import { apiFetchJson } from '../../../platform/api/client';
import { emitCommunityChannelsChanged } from '../../../platform/community/community-channel-events';
import type { AppApi } from '../../../platform/runtime/runtime-context';

/**
 * The community parts of AI-DO platform/admin/admin-api.ts (channel manager)
 * and platform/admin/admin-dm-safety-api.ts (filter terms), on `runtime.api`.
 * AI-DO reached them from the platform admin console; the board serves them
 * to board managers (`/v1/community/admin/...`).
 */
export type AdminCommunityChannelItem = components['schemas']['ChannelOut'];
export type AdminCommunityChannelInput = Required<components['schemas']['ChannelRequest']>;
export type AdminCommunityChannelsResponse = components['schemas']['ChannelsResponse'];

export function listCommunityAdminChannels(
  api: AppApi | null | undefined,
): Promise<AdminCommunityChannelsResponse> {
  return apiFetchJson<AdminCommunityChannelsResponse>(
    '/v1/community/admin/channels',
    api,
  );
}

export function createCommunityAdminChannel(
  api: AppApi | null | undefined,
  payload: AdminCommunityChannelInput,
): Promise<AdminCommunityChannelItem> {
  return apiFetchJson<AdminCommunityChannelItem>(
    '/v1/community/admin/channels',
    api,
    { method: 'POST', body: JSON.stringify(payload) },
  ).then((channel) => {
    emitCommunityChannelsChanged();
    return channel;
  });
}

export function updateCommunityAdminChannel(
  api: AppApi | null | undefined,
  channelKey: string,
  payload: AdminCommunityChannelInput,
): Promise<AdminCommunityChannelItem> {
  return apiFetchJson<AdminCommunityChannelItem>(
    `/v1/community/admin/channels/${encodeURIComponent(channelKey)}`,
    api,
    { method: 'PATCH', body: JSON.stringify(payload) },
  ).then((channel) => {
    emitCommunityChannelsChanged();
    return channel;
  });
}

export function deleteCommunityAdminChannel(
  api: AppApi | null | undefined,
  channelKey: string,
): Promise<void> {
  return apiFetchJson<void>(
    `/v1/community/admin/channels/${encodeURIComponent(channelKey)}`,
    api,
    { method: 'DELETE' },
  ).then(() => {
    emitCommunityChannelsChanged();
  });
}

export type FilterTerm = components['schemas']['FilterTermItem'];
export type FilterTermList = components['schemas']['FilterTermList'];

const FILTER_TERMS = '/v1/community/admin/filter-terms';

export function listFilterTerms(api: AppApi | null | undefined): Promise<FilterTermList> {
  return apiFetchJson<FilterTermList>(FILTER_TERMS, api);
}

export function addFilterTerm(
  api: AppApi | null | undefined,
  term: string,
): Promise<FilterTerm> {
  return apiFetchJson<FilterTerm>(FILTER_TERMS, api, {
    method: 'POST',
    body: JSON.stringify({ term } satisfies components['schemas']['FilterTermCreateRequest']),
  });
}

export function removeFilterTerm(
  api: AppApi | null | undefined,
  termId: string,
): Promise<void> {
  return apiFetchJson<void>(`${FILTER_TERMS}/${encodeURIComponent(termId)}`, api, {
    method: 'DELETE',
  });
}
