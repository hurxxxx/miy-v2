import type { components } from '../../../api.generated';
import { apiFetchJsonWithMappedError } from '../../../platform/api/client';
import { COMMUNITY_CHANNELS_CHANGED_EVENT } from '../../../platform/community/community-channel-events';
import type { AppApi } from '../../../platform/runtime/runtime-context';

// miy: AI-DO passed the session credential to every call. The app calls its
// own API through `runtime.api` (`api` below) with app-relative paths:
// `/v1/community/...` reaches the server as `/v1/community/...`, AI-DO's
// path. Images of a post come through the file gateway (`runtime.fileApi`).
export type FileApi = (path: string, init?: RequestInit) => Promise<Response>;

export type CommunityChannel = components['schemas']['ChannelOut'];
export type CommunityChannelsResponse = components['schemas']['ChannelsResponse'];
export type CommunityPost = components['schemas']['PostOut'];
export type CommunityComment = components['schemas']['CommentOut'];
export type CommunityPostDetail = components['schemas']['PostDetail'];
export type CommunityPostListResponse = components['schemas']['PostListResponse'];
export type CommunityPostCreateInput = components['schemas']['PostCreateRequest'];
export type CommunityPostUpdateInput = components['schemas']['PostUpdateRequest'];
export type CommunityCommentCreateInput = components['schemas']['CommentCreateRequest'];
export type CommunityCommentUpdateInput = components['schemas']['CommentUpdateRequest'];

export interface CommunityMediaResolveResponse {
  resolved: Record<string, string>;
}

export class CommunityApiError extends Error {
  status: number;
  /** miy: the server error code, so the banned-word rejection can be named. */
  code: string | null;

  constructor(status: number, message: string, code: string | null = null) {
    super(message);
    this.status = status;
    this.code = code;
    this.name = 'CommunityApiError';
  }
}

function errorCode(payload: unknown): string | null {
  return payload && typeof payload === 'object' && 'code' in payload
    ? String((payload as { code: unknown }).code ?? '') || null
    : null;
}

async function request<T>(
  path: string,
  api: AppApi | null | undefined,
  init: RequestInit = {},
): Promise<T> {
  return apiFetchJsonWithMappedError<T>(
    path,
    api,
    init,
    (error) =>
      new CommunityApiError(
        error.status,
        error.message || `Community request failed with ${error.status}.`,
        errorCode(error.payload),
      ),
  );
}

let cachedCommunityChannels: CommunityChannelsResponse | null = null;
let communityChannelsRequest: Promise<CommunityChannelsResponse> | null = null;

function cloneCommunityChannelsResponse(
  response: CommunityChannelsResponse,
): CommunityChannelsResponse {
  return {
    channels: response.channels.map((channel) => ({ ...channel })),
  };
}

export function invalidateCommunityChannelsCache(): void {
  cachedCommunityChannels = null;
  communityChannelsRequest = null;
}

if (typeof window !== 'undefined') {
  window.addEventListener(
    COMMUNITY_CHANNELS_CHANGED_EVENT,
    invalidateCommunityChannelsCache,
  );
}

export function listCommunityChannels(
  api: AppApi | null | undefined,
  options: { forceRefresh?: boolean } = {},
): Promise<CommunityChannelsResponse> {
  if (!options.forceRefresh && cachedCommunityChannels) {
    return Promise.resolve(
      cloneCommunityChannelsResponse(cachedCommunityChannels),
    );
  }

  if (!options.forceRefresh && communityChannelsRequest) {
    return communityChannelsRequest.then(cloneCommunityChannelsResponse);
  }

  const nextRequest = request<CommunityChannelsResponse>(
    '/v1/community/channels',
    api,
  )
    .then((response) => {
      if (communityChannelsRequest === nextRequest) {
        cachedCommunityChannels = cloneCommunityChannelsResponse(response);
      }
      return cloneCommunityChannelsResponse(response);
    })
    .finally(() => {
      if (communityChannelsRequest === nextRequest) {
        communityChannelsRequest = null;
      }
    });
  communityChannelsRequest = nextRequest;
  return nextRequest;
}

export function listCommunityPosts(
  api: AppApi | null | undefined,
  channelKey: string,
  options: { page?: number; pageSize?: number; search?: string } = {},
): Promise<CommunityPostListResponse> {
  const params = new URLSearchParams();
  if (options.page) params.set('page', String(options.page));
  if (options.pageSize) params.set('page_size', String(options.pageSize));
  if (options.search?.trim()) params.set('q', options.search.trim());
  const query = params.toString();
  return request<CommunityPostListResponse>(
    `/v1/community/channels/${encodeURIComponent(channelKey)}/posts${query ? `?${query}` : ''}`,
    api,
  );
}

export function createCommunityPost(
  api: AppApi | null | undefined,
  channelKey: string,
  payload: CommunityPostCreateInput,
): Promise<CommunityPost> {
  return request<CommunityPost>(
    `/v1/community/channels/${encodeURIComponent(channelKey)}/posts`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
}

export function getCommunityPost(
  api: AppApi | null | undefined,
  postId: string,
): Promise<CommunityPostDetail> {
  return request<CommunityPostDetail>(
    `/v1/community/posts/${encodeURIComponent(postId)}`,
    api,
  );
}

export function markCommunityPostRead(
  api: AppApi | null | undefined,
  postId: string,
): Promise<void> {
  return request<void>(
    `/v1/community/posts/${encodeURIComponent(postId)}/read`,
    api,
    { method: 'POST' },
  );
}

export function unlockCommunityPost(
  api: AppApi | null | undefined,
  postId: string,
  password: string,
): Promise<CommunityPostDetail> {
  return request<CommunityPostDetail>(
    `/v1/community/posts/${encodeURIComponent(postId)}/unlock`,
    api,
    {
      method: 'POST',
      body: JSON.stringify({ password } satisfies components['schemas']['UnlockRequest']),
    },
  );
}

/**
 * AI-DO `POST /community/posts/{id}/media/resolve` answered signed proxy URLs.
 * miy has no public file URL: each image of the post (or of its comments) is
 * downloaded through the app's file route with the same checks (and the
 * post password for a secret post) and handed back as a local Blob URL.
 */
export async function resolveCommunityPostMediaUrls(
  fileApi: FileApi,
  postId: string,
  urls: string[],
  password?: string | null,
): Promise<CommunityMediaResolveResponse> {
  const resolved: Record<string, string> = {};
  for (const url of urls) {
    if (!url.startsWith('media:')) continue;
    const mediaId = url.slice('media:'.length);
    const response = await fileApi(
      `/community/posts/${encodeURIComponent(postId)}/media/${encodeURIComponent(mediaId)}`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          password: password || null,
        } satisfies components['schemas']['PostMediaRequest']),
      },
    );
    if (!response.ok) {
      throw new CommunityApiError(
        response.status,
        `Community request failed with ${response.status}.`,
      );
    }
    resolved[url] = URL.createObjectURL(await response.blob());
  }
  return { resolved };
}

export function updateCommunityPost(
  api: AppApi | null | undefined,
  postId: string,
  payload: CommunityPostUpdateInput,
): Promise<CommunityPost> {
  return request<CommunityPost>(
    `/v1/community/posts/${encodeURIComponent(postId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
  );
}

export function deleteCommunityPost(
  api: AppApi | null | undefined,
  postId: string,
): Promise<void> {
  return request<void>(
    `/v1/community/posts/${encodeURIComponent(postId)}`,
    api,
    { method: 'DELETE' },
  );
}

export function createCommunityComment(
  api: AppApi | null | undefined,
  postId: string,
  payload: CommunityCommentCreateInput,
): Promise<CommunityComment> {
  return request<CommunityComment>(
    `/v1/community/posts/${encodeURIComponent(postId)}/comments`,
    api,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
}

export function updateCommunityComment(
  api: AppApi | null | undefined,
  postId: string,
  commentId: string,
  payload: CommunityCommentUpdateInput,
): Promise<CommunityComment> {
  return request<CommunityComment>(
    `/v1/community/posts/${encodeURIComponent(postId)}/comments/${encodeURIComponent(commentId)}`,
    api,
    {
      method: 'PATCH',
      body: JSON.stringify(payload),
    },
  );
}

export function deleteCommunityComment(
  api: AppApi | null | undefined,
  postId: string,
  commentId: string,
): Promise<void> {
  return request<void>(
    `/v1/community/posts/${encodeURIComponent(postId)}/comments/${encodeURIComponent(commentId)}`,
    api,
    { method: 'DELETE' },
  );
}
