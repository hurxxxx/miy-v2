import { useEffect, useState } from 'react';
import { FileIcon } from 'lucide-react';

import { useAppFileApi } from '../../../platform/runtime/runtime-context';
import { fetchAttachmentBlob } from '../api/pms-api';

/**
 * miy: AI-DO rendered `<img src={attachment.download_url}>` with a signed URL.
 * Attachment bytes come through the platform file gateway here, so the image
 * is read as a blob and shown from a local object URL that is revoked when the
 * thumbnail goes away. A failed read shows the file icon AI-DO used for other
 * files.
 */
export function PmsAttachmentThumbnail({
  attachmentId,
  filename,
  className,
}: {
  attachmentId: string;
  filename: string;
  className?: string;
}) {
  const fileApi = useAppFileApi();
  const [state, setState] = useState<{
    id: string;
    url: string | null;
    failed: boolean;
  } | null>(null);
  useEffect(() => {
    let cancelled = false;
    let objectUrl: string | null = null;
    fetchAttachmentBlob(fileApi, attachmentId)
      .then((blob) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(blob);
        setState({ id: attachmentId, url: objectUrl, failed: false });
      })
      .catch(() => {
        if (!cancelled) setState({ id: attachmentId, url: null, failed: true });
      });
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [attachmentId, fileApi]);
  const current = state?.id === attachmentId ? state : null;
  if (current?.url) {
    return <img src={current.url} alt={filename} className={className} />;
  }
  return (
    <div className="flex size-8 items-center justify-center rounded border border-app-border bg-app-surface-sidebar">
      <FileIcon
        aria-hidden="true"
        size={14}
        className={current?.failed ? 'text-app-ink/40' : 'text-app-ink/20'}
      />
    </div>
  );
}
