import { describe, expect, it } from 'vitest';

import {
  downloadSpecForAttachment,
  imagePreviewUrlForAttachment,
  imageViewerForAttachment,
  resolveWebDmAttachmentUrl,
} from './dm-attachment-url';

describe('web DM attachment URL helpers', () => {
  it('resolves only same-origin DM attachment URLs', () => {
    expect(
      resolveWebDmAttachmentUrl(
        '/dm/attachments/a/content?disposition=inline',
        'https://workspace.example.test',
      ),
    ).toBe('/dm/attachments/a/content?disposition=inline');
    expect(
      resolveWebDmAttachmentUrl(
        'https://cdn.example.test/a.png',
        'https://workspace.example.test',
      ),
    ).toBeNull();
    expect(
      resolveWebDmAttachmentUrl('/v1/users/me', 'https://workspace.example.test'),
    ).toBeNull();
    expect(
      resolveWebDmAttachmentUrl(
        '/dm/attachments/a%2Fb/content',
        'https://workspace.example.test',
      ),
    ).toBeNull();
  });

  it('builds image preview and viewer state from safe attachment URLs', () => {
    expect(
      imagePreviewUrlForAttachment(
        { is_image: true, preview_url: '/dm/attachments/a/content?disposition=inline' },
        'https://workspace.example.test',
      ),
    ).toBe('/dm/attachments/a/content?disposition=inline');
    expect(
      imagePreviewUrlForAttachment(
        { is_image: false, preview_url: '/dm/attachments/a/content?disposition=inline' },
        'https://workspace.example.test',
      ),
    ).toBeNull();
    expect(
      imageViewerForAttachment(
        { filename: 'screen.png' },
        '/dm/attachments/a/content?disposition=inline',
        'https://workspace.example.test',
      ),
    ).toEqual({
      filename: 'screen.png',
      url: '/dm/attachments/a/content?disposition=inline',
    });
    expect(
      imageViewerForAttachment(
        { filename: 'screen.png' },
        'https://cdn.example.test/screen.png',
        'https://workspace.example.test',
      ),
    ).toBeNull();
  });

  it('builds download specs only for safe attachment URLs', () => {
    const scriptUrl = `java${'script'}:alert(1)`;

    expect(
      downloadSpecForAttachment(
        { filename: 'report.pdf' },
        '/dm/attachments/a/content',
        'https://workspace.example.test',
      ),
    ).toEqual({
      href: '/dm/attachments/a/content',
      filename: 'report.pdf',
      rel: 'noreferrer',
    });
    expect(
      downloadSpecForAttachment(
        { filename: 'report.pdf' },
        scriptUrl,
        'https://workspace.example.test',
      ),
    ).toBeNull();
  });
});
