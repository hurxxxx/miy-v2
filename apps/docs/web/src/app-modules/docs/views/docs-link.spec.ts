import { describe, expect, it, vi } from 'vitest';

import { copyText, portalOrigin, portalUrlFor } from './docs-link';

describe('frame links and clipboard (miy)', () => {
  it('uses the portal origin the browser reports, else a portal path', () => {
    const list = (origins: string[]) =>
      ({ ancestorOrigins: Object.assign([...origins], { contains: () => false }) }) as unknown as Location;
    expect(portalOrigin(list(['https://portal.example']))).toBe('https://portal.example');
    expect(portalOrigin(list([]))).toBe('');
    expect(portalOrigin(list(['javascript:alert(1)']))).toBe('');
    expect(portalOrigin({} as Location)).toBe('');
    expect(portalUrlFor('/apps/docs/', '/doc-1?page=p', 'https://portal.example')).toBe(
      'https://portal.example/apps/docs/doc-1?page=p',
    );
    expect(portalUrlFor('/apps/docs', 'doc-1', '')).toBe('/apps/docs/doc-1');
  });

  it('falls back to the copy command when the clipboard API is blocked', async () => {
    const blocked = { writeText: vi.fn().mockRejectedValue(new Error('NotAllowedError')) };
    const execCommand = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, 'execCommand', { value: execCommand, configurable: true });
    await expect(copyText('link', document, blocked)).resolves.toBe(true);
    expect(execCommand).toHaveBeenCalledWith('copy');
    execCommand.mockReturnValue(false);
    await expect(copyText('link', document, blocked)).resolves.toBe(false);
    const allowed = { writeText: vi.fn().mockResolvedValue(undefined) };
    await expect(copyText('link', document, allowed)).resolves.toBe(true);
    expect(allowed.writeText).toHaveBeenCalledWith('link');
  });
});
