import { describe, expect, it } from 'vitest';

import { excalidrawAssetPath, withoutRemoteFallback } from './self-hosted-assets';

describe('self-hosted Excalidraw assets', () => {
  it('points beside the UI chunks', () => {
    expect(
      excalidrawAssetPath('https://frame.example/ui/r1/assets/AppRoot-abc.js'),
    ).toBe('https://frame.example/ui/r1/excalidraw-assets/');
  });

  it('drops only the esm.sh fallback source', () => {
    const own =
      "url(https://frame.example/excalidraw-assets/fonts/Virgil/Virgil-Regular.woff2) format('woff2')";
    const source = `${own}, url(https://esm.sh/@excalidraw/excalidraw@0.18.1/dist/prod/fonts/Virgil/Virgil-Regular.woff2) format('woff2')`;
    expect(withoutRemoteFallback(source)).toBe(own);
    expect(withoutRemoteFallback(own)).toBe(own);
    const data = 'url(data:font/woff2;base64,AAAA)';
    expect(withoutRemoteFallback(data)).toBe(data);
  });
});
