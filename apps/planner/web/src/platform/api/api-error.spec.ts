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
    const error = new ApiError('http', 'planner_event_end_after_start', 400);
    expect(apiErrorMessage(error, 'fallback')).toBe(
      'Planner 이벤트 종료 시각은 시작 시각보다 이후여야 합니다.',
    );
    syncLocale('en-US');
    expect(apiErrorMessage(error, 'fallback')).toBe(
      'Planner event end must be after start.',
    );
    expect(
      apiErrorMessage(new ApiError('http', 'calendar_range_too_large', 400), 'x'),
    ).toBe('Range exceeds maximum 366 days.');
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
    expect(i18n.t('apps:planner.planner')).toBe('Planner');
    expect(i18n.t('apps:planner.holidays.unavailable')).toBe('No holiday data');
    expect(syncLocale('fr-FR')).toBe('ko-KR');
    expect(i18n.t('apps:planner.planner')).toBe('플래너');
    expect(i18n.t('apps:planner.holidays.unavailable')).toBe('휴일 정보 없음');
  });
});
