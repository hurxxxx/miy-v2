import { ApiError } from '@miy/sdk';

import { i18n } from '../i18n';
import type { AppApi } from '../runtime/runtime-context';

import { apiErrorMessage } from './api-error';

/**
 * AI-DO apps/web/src/platform/api/client.ts, reduced to what the drive
 * uses. AI-DO called `fetch` with a bearer token; miy apps call their own
 * API through `runtime.api` (the frame's credential, no token in app code),
 * with app-relative paths (`/v1/community/channels` reaches `/api/v1/...`).
 *
 * The thrown `ApiRequestError` keeps AI-DO's shape (status + displayable
 * message). AI-DO's message was the server's translated `detail` or
 * "Request failed with N."; here it is the translated error code
 * (api-error.ts) or the matching localized `common:feedback` text.
 */
export class ApiRequestError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    public readonly payload: unknown = null,
  ) {
    super(message);
  }
}

export function toApiRequestError(error: unknown): unknown {
  if (!(error instanceof ApiError) || error.kind === 'abort') {
    return error;
  }
  const status = error.status ?? 0;
  const fallback =
    error.kind === 'http'
      ? i18n.t('common:feedback.requestFailed', { status })
      : i18n.t('common:feedback.requestFailedRetry');
  return new ApiRequestError(status, apiErrorMessage(error, fallback), {
    code: error.code,
    requestId: error.requestId,
  });
}

export async function apiFetchJson<T>(
  path: string,
  api: AppApi | null | undefined,
  init: RequestInit = {},
): Promise<T> {
  if (!api) {
    throw new ApiRequestError(0, i18n.t('common:feedback.requestFailedRetry'));
  }
  try {
    return await api<T>(path, init);
  } catch (error) {
    throw toApiRequestError(error);
  }
}

export async function apiFetchJsonWithMappedError<T, TError extends Error = Error>(
  path: string,
  api: AppApi | null | undefined,
  init: RequestInit = {},
  mapError: (error: ApiRequestError) => TError,
): Promise<T> {
  try {
    return await apiFetchJson<T>(path, api, init);
  } catch (error) {
    if (error instanceof ApiRequestError) {
      throw mapError(error);
    }
    throw error;
  }
}
