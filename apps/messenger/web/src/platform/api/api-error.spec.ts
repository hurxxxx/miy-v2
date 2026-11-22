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
    const error = new ApiError('http', 'dm_conversation_blocked', 403);
    expect(apiErrorMessage(error, 'fallback')).toBe(
      '1:1 차단 상태라 이 사람과 1:1 대화를 할 수 없습니다.',
    );
    syncLocale('en-US');
    expect(apiErrorMessage(error, 'fallback')).toBe(
      '1:1 messaging with this person is blocked.',
    );
    expect(
      apiErrorMessage(new ApiError('http', 'dm_attachment_size_limit_exceeded', 413), 'x'),
    ).toBe('DM attachments must be 10 MB or smaller.');
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
    expect(i18n.t('apps:dm.title')).toBe('DM');
    expect(i18n.t('apps:dm.edited')).toBe('Edited');
    expect(syncLocale('fr-FR')).toBe('ko-KR');
    expect(i18n.t('apps:dm.edited')).toBe('수정됨');
    expect(i18n.t('apps:dm.safety.report')).toBe('신고');
  });
});
