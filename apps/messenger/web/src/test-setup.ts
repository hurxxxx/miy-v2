import { vi } from 'vitest';
import './platform/i18n';

// Same as AI-DO apps/web/src/test-setup.ts.
Object.defineProperty(window, 'scrollTo', {
  configurable: true,
  value: vi.fn(),
});

// jsdom has no scrollIntoView; the messenger scrolls to jumped-to messages.
if (!Element.prototype.scrollIntoView) {
  Object.defineProperty(Element.prototype, 'scrollIntoView', {
    configurable: true,
    value: vi.fn(),
  });
}
