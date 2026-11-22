import { vi } from 'vitest';
import './platform/i18n';

// Same as AI-DO apps/web/src/test-setup.ts.
Object.defineProperty(window, 'scrollTo', {
  configurable: true,
  value: vi.fn(),
});
