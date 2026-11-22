import { isApiError } from '@miy/sdk';

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
  ugc_filter_term_invalid: { max: 50 },
  ugc_filter_term_limit: { limit: 1000 },
};

export function apiErrorMessage(error: unknown, fallback: string): string {
  if (isApiError(error) && error.kind === 'http') {
    const key = `errors:${error.code}`;
    if (i18n.exists(key)) {
      return i18n.t(key, ERROR_PARAMS[error.code]);
    }
  }
  return fallback;
}

/**
 * miy (banned words, 보강): AI-DO's board screens show their own fixed failure
 * text. A post or comment refused by the banned-word filter shows the
 * catalog sentence instead, so the writer knows why; every other failure
 * keeps the screen's text.
 */
export function contentErrorMessage(error: unknown, fallback: string): string {
  const code =
    error && typeof error === 'object' && 'code' in error
      ? (error as { code: unknown }).code
      : null;
  if (code === 'ugc_content_rejected') {
    return i18n.t('errors:ugc_content_rejected');
  }
  return fallback;
}
