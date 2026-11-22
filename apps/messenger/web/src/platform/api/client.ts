import type { AppRuntime } from '@miy/sdk';
import { ApiError } from '@miy/sdk';

import { i18n } from '../i18n';

import { apiErrorMessage } from './api-error';

/**
 * AI-DO apps/web/src/platform/api/client.ts for the miy frame. AI-DO called
 * `fetch` with a bearer token; a miy app calls its own API through
 * `runtime.api` / `runtime.fileApi` with the frame's in-memory credential.
 *
 * The ported screens and models keep AI-DO's `token` argument: it is an
 * opaque marker (see platform/auth/auth-provider.tsx) and the requests go
 * through the runtime bound here, never a credential in app code. Paths are
 * app relative (`/v1/dm/...` reaches `/api/v1/dm/...`).
 *
 * The thrown `ApiRequestError` keeps AI-DO's shape (status + displayable
 * message); the message is the translated error code (api-error.ts) or the
 * localized `common:feedback` text, and `payload.code` the server code.
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

type BoundRuntime = Pick<AppRuntime, 'api' | 'fileApi'>;

let boundRuntime: BoundRuntime | null = null;

/** AppRoot binds the frame runtime; tests may bind a stand-in. */
export function bindApiRuntime(runtime: BoundRuntime | null): void {
  boundRuntime = runtime;
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

function requireRuntime(token: string | null | undefined): BoundRuntime {
  if (!token || !boundRuntime) {
    throw new ApiRequestError(0, i18n.t('common:feedback.requestFailedRetry'));
  }
  return boundRuntime;
}

export async function apiFetchJson<T>(
  path: string,
  token: string | null | undefined,
  init: RequestInit = {},
): Promise<T> {
  const runtime = requireRuntime(token);
  try {
    return await runtime.api<T>(path, init);
  } catch (error) {
    throw toApiRequestError(error);
  }
}

/** miy: binary transfers through the file gateway (`runtime.fileApi`). */
export async function apiFetchFile(
  path: string,
  token: string | null | undefined,
  init: RequestInit = {},
): Promise<Response> {
  const runtime = requireRuntime(token);
  try {
    return await runtime.fileApi(path, init);
  } catch (error) {
    throw toApiRequestError(error);
  }
}

export async function apiFetchJsonWithMappedError<T, TError extends Error = Error>(
  path: string,
  token: string | null | undefined,
  init: RequestInit = {},
  mapError: (error: ApiRequestError) => TError,
): Promise<T> {
  try {
    return await apiFetchJson<T>(path, token, init);
  } catch (error) {
    if (error instanceof ApiRequestError) {
      throw mapError(error);
    }
    throw error;
  }
}

/** The server error code of an `ApiRequestError` (AI-DO `dmErrorCode`). */
export function apiErrorCode(error: unknown): string | null {
  if (!(error instanceof ApiRequestError)) return null;
  const payload = error.payload;
  if (payload && typeof payload === 'object' && 'code' in payload) {
    const code = (payload as { code?: unknown }).code;
    return typeof code === 'string' ? code : null;
  }
  return null;
}
