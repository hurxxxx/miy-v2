import { ApiError } from '@miy/sdk';

import { i18n } from '../i18n';

/**
 * AI-DO showed the server's translated error `detail`. miy's `runtime.api`
 * throws an `ApiError` that carries only a code (the server's
 * `{error:{code}}`), so the translation happens here with AI-DO's server
 * catalog messages (`errors` namespace). Unknown codes, network failures and
 * aborted requests use the screen's own fallback message, as AI-DO did when
 * the server gave no detail.
 */
export function apiErrorMessage(error: unknown, fallback: string): string {
  const code = apiErrorCode(error);
  if (code) {
    const key = `errors:${code}`;
    if (i18n.exists(key)) {
      return i18n.t(key);
    }
  }
  return fallback;
}

/** The server's error code of a failed request, when it gave one. */
export function apiErrorCode(error: unknown): string | null {
  if (error instanceof ApiError && error.kind === 'http') {
    return error.code ?? null;
  }
  if (error && typeof error === 'object' && 'payload' in error) {
    const payload = (error as { payload: unknown }).payload;
    if (payload && typeof payload === 'object' && 'code' in payload) {
      const code = (payload as { code: unknown }).code;
      return typeof code === 'string' ? code : null;
    }
  }
  return null;
}

/** The HTTP status of a failed request (0 for network failures). */
export function apiErrorStatus(error: unknown): number {
  if (error instanceof ApiError) return error.status ?? 0;
  if (error && typeof error === 'object' && 'status' in error) {
    const status = (error as { status: unknown }).status;
    return typeof status === 'number' ? status : 0;
  }
  return 0;
}
