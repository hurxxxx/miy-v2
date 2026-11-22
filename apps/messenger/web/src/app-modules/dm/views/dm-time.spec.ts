import { describe, expect, it } from 'vitest';

import {
  dmDateGroupStartIds,
  formatDmDateDivider,
  formatDmMessageTime,
} from './dm-time';

describe('formatDmMessageTime', () => {
  it('treats timezone-less API timestamps as UTC and displays them in KST by default', () => {
    const formatted = formatDmMessageTime('2026-05-19T02:46:00', 'ko-KR', undefined);

    expect(formatted).toContain('11:46');
    expect(formatted).not.toContain('02:46');
  });

  it('uses the signed-in user timezone when provided', () => {
    expect(formatDmMessageTime('2026-05-19T02:46:00', 'en-US', 'UTC')).toBe('02:46 AM');
  });
});

describe('formatDmDateDivider', () => {
  it('shows the local date with its weekday', () => {
    const formatted = formatDmDateDivider('2026-10-19T16:00:00', 'ko-KR', 'Asia/Seoul');

    expect(formatted).toMatch(/2026\D+10\D+20/);
    expect(formatted).toContain('(화)');
  });
});

describe('dmDateGroupStartIds', () => {
  const message = (id: string, created_at: string) => ({ created_at, id });

  it('opens one group per local day', () => {
    const ids = dmDateGroupStartIds(
      [
        message('a', '2026-10-19T14:59:00'),
        message('b', '2026-10-19T15:01:00'),
        message('c', '2026-10-20T02:00:00'),
      ],
      'Asia/Seoul',
    );

    expect([...ids]).toEqual(['a', 'b']);
  });

  it('ignores unparsable timestamps instead of splitting groups', () => {
    const ids = dmDateGroupStartIds(
      [
        message('a', '2026-10-20T01:00:00'),
        message('bad', ''),
        message('b', '2026-10-20T02:00:00'),
      ],
      'Asia/Seoul',
    );

    expect([...ids]).toEqual(['a']);
  });
});
