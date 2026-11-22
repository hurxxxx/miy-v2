// AI-DO apps/web/src/platform/browser/browser-download.ts, the Blob download
// only (the frame allows downloads; the other helpers navigated the window).
export type BrowserDownloadAnchor = {
  href: string;
  download: string;
  click: () => void;
  remove: () => void;
};

export type BrowserDownloadAdapter = {
  createObjectUrl: (blob: Blob) => string;
  revokeObjectUrl: (url: string) => void;
  createAnchor: () => BrowserDownloadAnchor;
  appendAnchor: (anchor: BrowserDownloadAnchor) => void;
};

export const defaultBrowserDownloadAdapter: BrowserDownloadAdapter = {
  createObjectUrl: (blob) => URL.createObjectURL(blob),
  revokeObjectUrl: (url) => URL.revokeObjectURL(url),
  createAnchor: () => document.createElement('a'),
  appendAnchor: (anchor) => {
    document.body.appendChild(anchor as HTMLAnchorElement);
  },
};

export function downloadBlobAsFile(
  blob: Blob,
  filename: string,
  adapter: BrowserDownloadAdapter = defaultBrowserDownloadAdapter,
): void {
  const url = adapter.createObjectUrl(blob);
  const anchor = adapter.createAnchor();
  anchor.href = url;
  anchor.download = filename;
  adapter.appendAnchor(anchor);
  try {
    anchor.click();
  } finally {
    anchor.remove();
    adapter.revokeObjectUrl(url);
  }
}
