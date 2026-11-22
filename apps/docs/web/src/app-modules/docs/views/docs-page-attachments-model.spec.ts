import { describe, expect, it } from 'vitest';

import type { DocsPageAttachment } from '../api/docs-api';
import {
  docPageAttachmentErrorMessage,
  sortDocPageAttachments,
  withUploadedAttachments,
  withoutAttachment,
} from './docs-page-attachments-model';

function attachment(
  id: string,
  createdAt: string,
  filename = `${id}.pdf`,
): DocsPageAttachment {
  return {
    id,
    page_id: 'page-1',
    filename,
    content_type: 'application/pdf',
    size_bytes: 10,
    uploaded_by_id: 'user-1',
    uploaded_by_name: 'Docs Admin',
    created_at: createdAt,
  };
}

describe('sortDocPageAttachments', () => {
  it('orders oldest first', () => {
    const sorted = sortDocPageAttachments([
      attachment('b', '2026-09-18T10:00:00'),
      attachment('a', '2026-09-17T10:00:00'),
    ]);

    expect(sorted.map((item) => item.id)).toEqual(['a', 'b']);
  });

  it('breaks a timestamp tie on id so the order never flickers', () => {
    const sorted = sortDocPageAttachments([
      attachment('b', '2026-09-18T10:00:00'),
      attachment('a', '2026-09-18T10:00:00'),
    ]);

    expect(sorted.map((item) => item.id)).toEqual(['a', 'b']);
  });

  it('leaves the input array untouched', () => {
    const input = [
      attachment('b', '2026-09-18T10:00:00'),
      attachment('a', '2026-09-17T10:00:00'),
    ];

    sortDocPageAttachments(input);

    expect(input.map((item) => item.id)).toEqual(['b', 'a']);
  });
});

describe('withUploadedAttachments', () => {
  it('appends new uploads in list order', () => {
    const next = withUploadedAttachments(
      [attachment('a', '2026-09-17T10:00:00')],
      [attachment('b', '2026-09-18T10:00:00')],
    );

    expect(next.map((item) => item.id)).toEqual(['a', 'b']);
  });

  it('does not double a row when the same upload arrives twice', () => {
    const uploaded = attachment('a', '2026-09-17T10:00:00');

    const next = withUploadedAttachments([uploaded], [uploaded]);

    expect(next).toHaveLength(1);
  });

  it('replaces an existing row with the newer server copy', () => {
    const next = withUploadedAttachments(
      [attachment('a', '2026-09-17T10:00:00', 'old.pdf')],
      [attachment('a', '2026-09-17T10:00:00', 'renamed.pdf')],
    );

    expect(next[0].filename).toBe('renamed.pdf');
  });
});

describe('withoutAttachment', () => {
  it('drops only the removed row', () => {
    const next = withoutAttachment(
      [
        attachment('a', '2026-09-17T10:00:00'),
        attachment('b', '2026-09-18T10:00:00'),
      ],
      'a',
    );

    expect(next.map((item) => item.id)).toEqual(['b']);
  });

  it('is a no-op for an id that is not in the list', () => {
    const current = [attachment('a', '2026-09-17T10:00:00')];

    expect(withoutAttachment(current, 'missing')).toHaveLength(1);
  });
});

describe('docPageAttachmentErrorMessage', () => {
  it('uses the server message when there is one', () => {
    expect(
      docPageAttachmentErrorMessage(new Error('Too large'), 'fallback'),
    ).toBe('Too large');
  });

  it('falls back for a blank or non-error failure', () => {
    expect(docPageAttachmentErrorMessage(new Error('   '), 'fallback')).toBe(
      'fallback',
    );
    expect(docPageAttachmentErrorMessage('boom', 'fallback')).toBe('fallback');
  });
});
