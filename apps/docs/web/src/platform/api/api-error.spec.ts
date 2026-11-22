import { afterEach, describe, expect, it } from 'vitest';
import { ApiError } from '@miy/sdk';

import { i18n, syncLocale } from '../i18n';
import { apiErrorMessage } from './api-error';
import { ApiRequestError, toApiRequestError } from './client';

afterEach(() => {
  syncLocale('ko-KR');
});

describe('API error messages (miy: translated from the error code)', () => {
  it('uses AI-DO catalog messages for known server codes', () => {
    const error = new ApiError('http', 'docs_page_parent_cycle', 409);
    expect(apiErrorMessage(error, 'fallback')).toBe(
      '페이지 부모 관계에 순환이 포함될 수 없습니다.',
    );
    syncLocale('en-US');
    expect(apiErrorMessage(error, 'fallback')).toBe(
      'Page parent relationship cannot contain a cycle.',
    );
    expect(
      apiErrorMessage(new ApiError('http', 'docs_attachment_size_limit_exceeded', 413), 'x'),
    ).toBe('Attachments may be at most 10MB.');
  });

  it('falls back for unknown codes, network failures and aborts', () => {
    expect(apiErrorMessage(new ApiError('http', 'http_502', 502), 'fb')).toBe('fb');
    expect(
      apiErrorMessage(new ApiError('network', 'network_unavailable'), 'fb'),
    ).toBe('fb');
    expect(apiErrorMessage(new Error('boom'), 'fb')).toBe('fb');
  });

  it('maps transport errors to AI-DO style request errors', () => {
    const http = toApiRequestError(new ApiError('http', 'http_500', 500));
    expect(http).toBeInstanceOf(ApiRequestError);
    expect((http as ApiRequestError).status).toBe(500);
    expect((http as ApiRequestError).message).toBe(
      i18n.t('common:feedback.requestFailed', { status: 500 }),
    );
    const network = toApiRequestError(
      new ApiError('network', 'network_unavailable'),
    );
    expect((network as ApiRequestError).message).toBe(
      i18n.t('common:feedback.requestFailedRetry'),
    );
    const abort = new ApiError('abort', 'request_aborted');
    expect(toApiRequestError(abort)).toBe(abort);
  });
});

describe('display language (miy: runtime.settings.locale)', () => {
  it('follows the portal locale and falls back to ko-KR', () => {
    expect(syncLocale('en-US')).toBe('en-US');
    expect(i18n.t('apps:docs.newDoc')).toBe('New Document');
    expect(i18n.t('apps:docsApp.conflict.reload')).toBe('Reload');
    expect(syncLocale('fr-FR')).toBe('ko-KR');
    expect(i18n.t('apps:docs.newDoc')).toBe('새 문서');
    expect(i18n.t('apps:docsApp.conflict.reload')).toBe('다시 불러오기');
  });
});
