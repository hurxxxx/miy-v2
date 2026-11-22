import { afterEach, describe, expect, it } from 'vitest';
import { ApiError } from '@miy/sdk';

import { i18n, syncLocale } from '../i18n';
import { apiErrorMessage, contentErrorMessage } from './api-error';
import { ApiRequestError, toApiRequestError } from './client';

afterEach(() => {
  syncLocale('ko-KR');
});

describe('API error messages (miy: translated from the error code)', () => {
  it('uses AI-DO catalog messages for known server codes', () => {
    const error = new ApiError('http', 'community_channel_read_only', 403);
    expect(apiErrorMessage(error, 'fallback')).toBe(
      '읽기 전용 채널에는 글이나 댓글을 작성할 수 없습니다.',
    );
    syncLocale('en-US');
    expect(apiErrorMessage(error, 'fallback')).toBe(
      'Read-only community channels do not allow posts or comments.',
    );
    expect(
      apiErrorMessage(new ApiError('http', 'ugc_filter_term_limit', 422), 'x'),
    ).toBe('You can register up to 1000 filter terms.');
    expect(
      apiErrorMessage(new ApiError('http', 'ugc_filter_term_invalid', 422), 'x'),
    ).toContain('1 to 50 characters');
  });

  it('names only the banned-word rejection in screens with fixed texts', () => {
    expect(contentErrorMessage({ code: 'ugc_content_rejected' }, 'fb')).toBe(
      '운영 정책상 보낼 수 없는 표현이 포함되어 있습니다.',
    );
    expect(contentErrorMessage({ code: 'community_post_not_found' }, 'fb')).toBe('fb');
    expect(contentErrorMessage(new Error('x'), 'fb')).toBe('fb');
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
    expect(i18n.t('apps:community.newPost')).toBe('New post');
    expect(i18n.t('apps:board.notices')).toBe('Announcements');
    expect(syncLocale('fr-FR')).toBe('ko-KR');
    expect(i18n.t('apps:community.newPost')).toBe('새 글');
    expect(i18n.t('apps:board.notices')).toBe('공지사항');
  });
});
