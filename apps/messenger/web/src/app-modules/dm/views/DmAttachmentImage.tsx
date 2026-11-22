import { useEffect, useState } from 'react';

import { useAuth } from '../../../platform/auth/auth-provider';
import { loadDmAttachmentObjectUrl } from '../api/dm-api';

/**
 * miy: AI-DO showed image attachments with `<img src={signed URL}>`. The
 * frame has no cookie or signed URL, so the image is read through
 * `runtime.fileApi` and shown from a local object URL. URLs are kept for the
 * frame's lifetime (bounded) so polling re-renders do not download again.
 */
const MAX_CACHED = 200;
const cache = new Map<string, Promise<string>>();

function cachedObjectUrl(token: string, path: string): Promise<string> {
  const hit = cache.get(path);
  if (hit) {
    cache.delete(path);
    cache.set(path, hit);
    return hit;
  }
  const loading = loadDmAttachmentObjectUrl(token, path);
  cache.set(path, loading);
  loading.catch(() => cache.delete(path));
  while (cache.size > MAX_CACHED) {
    const [oldest, url] = cache.entries().next().value as [string, Promise<string>];
    cache.delete(oldest);
    void url.then((value) => URL.revokeObjectURL(value)).catch(() => undefined);
  }
  return loading;
}

export function DmAttachmentImage({
  alt,
  className,
  path,
}: {
  alt: string;
  className?: string;
  path: string;
}) {
  const { token } = useAuth();
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!token) return undefined;
    let active = true;
    setUrl(null);
    cachedObjectUrl(token, path)
      .then((value) => {
        if (active) setUrl(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [path, token]);
  if (!url) {
    return (
      <span
        aria-label={alt}
        className={`block h-32 w-full animate-pulse bg-app-surface ${className ?? ''}`}
        role="img"
      />
    );
  }
  return <img alt={alt} className={className} src={url} />;
}
