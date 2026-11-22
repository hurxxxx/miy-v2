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
    const error = new ApiError('http', 'pms_task_not_found', 404);
    expect(apiErrorMessage(error, 'fallback')).toBe(
      '태스크를 찾을 수 없습니다.',
    );
    syncLocale('en-US');
    expect(apiErrorMessage(error, 'fallback')).toBe('Task not found.');
    expect(
      apiErrorMessage(
        new ApiError('http', 'pms_file_size_limit_exceeded', 413),
        'x',
      ),
    ).toBe('File size exceeds 10 MB limit.');
    expect(
      apiErrorMessage(new ApiError('http', 'validation_request_invalid', 422), 'x'),
    ).toBe('Request validation failed.');
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

  it('maps a transport error from another bundle through its shared fields', () => {
    syncLocale('en-US');
    const error = Object.assign(new Error('pms_task_not_found'), {
      name: 'ApiError',
      kind: 'http',
      code: 'pms_task_not_found',
      status: 404,
      requestId: 'request-1',
      retryable: false,
    });

    expect(error).not.toBeInstanceOf(ApiError);
    expect(apiErrorMessage(error, 'fallback')).toBe('Task not found.');
    expect(toApiRequestError(error)).toMatchObject({
      status: 404,
      message: 'Task not found.',
      payload: { code: 'pms_task_not_found', requestId: 'request-1' },
    });
  });
});

describe('display language (miy: runtime.settings.locale)', () => {
  it('follows the portal locale and falls back to ko-KR', () => {
    expect(syncLocale('en-US')).toBe('en-US');
    expect(i18n.t('apps:pms.newTask')).toBe('New Task');
    expect(i18n.t('apps:pms.unsavedChanges')).toBe(
      'You have unsaved changes. Leave anyway?',
    );
    expect(syncLocale('fr-FR')).toBe('ko-KR');
    expect(i18n.t('apps:pms.unsavedChanges')).toBe(
      '저장되지 않은 작업이 있습니다. 이동하시겠습니까?',
    );
  });
});
