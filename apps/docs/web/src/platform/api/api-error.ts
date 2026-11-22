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
const ERROR_PARAMS: Record<string, Record<string, unknown>> = {
  docs_attachment_size_limit_exceeded: { limit_mb: 10 },
};

export function apiErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof ApiError && error.kind === 'http') {
    const key = `errors:${error.code}`;
    if (i18n.exists(key)) {
      return i18n.t(key, ERROR_PARAMS[error.code]);
    }
  }
  return fallback;
}
