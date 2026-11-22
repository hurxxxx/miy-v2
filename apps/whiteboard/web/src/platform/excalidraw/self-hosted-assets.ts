/**
 * Excalidraw under the app frame's CSP (docs/PORTING.md §3.10).
 *
 * The frame allows fonts from its own origin only (`font-src 'self'`).
 * Excalidraw 0.18 loads its hand-drawn and CJK fonts from
 * `window.EXCALIDRAW_ASSET_PATH`, falling back to https://esm.sh. The build
 * copies the package's fonts next to the UI chunks (vite.config.ts,
 * `excalidraw-assets/fonts/…`), and this module points Excalidraw there.
 *
 * Excalidraw always appends the esm.sh URL as a second font source. The font
 * loads from the first source, but the browser still reports a CSP violation
 * for every blocked second source (about 230 font faces), so FontFace drops
 * that fallback here.
 *
 * Import this module before anything that loads `@excalidraw/excalidraw`
 * (AppRoot does; the editor and previews load Excalidraw lazily after it).
 */
const FALLBACK_SOURCE = /,\s*url\(\s*["']?https:\/\/esm\.sh\/[^)]*\)\s*(format\([^)]*\))?/g;

/** `…/excalidraw-assets/` beside this module's chunk (`…/assets/x.js`). */
export function excalidrawAssetPath(moduleUrl: string): string {
  const directory = ['..', 'excalidraw-assets', ''].join('/');
  return new URL(directory, moduleUrl).href;
}

/** A FontFace source list without Excalidraw's esm.sh fallback. */
export function withoutRemoteFallback(source: string): string {
  return source.replace(FALLBACK_SOURCE, '');
}

declare global {
  interface Window {
    EXCALIDRAW_ASSET_PATH?: string | string[];
  }
}

let installed = false;

export function installExcalidrawAssets(moduleUrl: string = import.meta.url): void {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  window.EXCALIDRAW_ASSET_PATH = excalidrawAssetPath(moduleUrl);
  const NativeFontFace = window.FontFace;
  if (typeof NativeFontFace !== 'function') return;
  class SelfHostedFontFace extends NativeFontFace {
    constructor(
      family: string,
      source: string | BufferSource,
      descriptors?: FontFaceDescriptors,
    ) {
      super(
        family,
        typeof source === 'string' ? withoutRemoteFallback(source) : source,
        descriptors,
      );
    }
  }
  window.FontFace = SelfHostedFontFace;
}

installExcalidrawAssets();
