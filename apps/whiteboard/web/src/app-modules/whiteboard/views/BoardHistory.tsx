import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { exportToSvg } from '@excalidraw/excalidraw';
import { Button, Dialog } from '@miy/ui';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import {
  normalizeWhiteboardScene,
  type WhiteboardDetail,
} from '../api/whiteboard-api';
import { hydrateSceneFiles } from '../api/whiteboard-files';

interface Version {
  id: string;
  revision: number;
  author: string;
  created_at: string;
}
interface Props {
  board: WhiteboardDetail;
  canRestore: boolean;
  onClose: () => void;
  onRestored: () => Promise<void>;
}

export function BoardHistory({
  board,
  canRestore,
  onClose,
  onRestored,
}: Props) {
  const { api, fileApi } = useRuntime();
  const { t } = useTranslation('apps');
  const path = '/v1/boards/' + encodeURIComponent(board.id) + '/versions';
  const [versions, setVersions] = useState<Version[]>();
  const [selected, setSelected] = useState<string>();
  const [preview, setPreview] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  useEffect(() => {
    let active = true;
    void api<Version[]>(path)
      .then((items) => {
        if (active) {
          setVersions(items);
          setSelected(items[0]?.id);
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [api, path]);
  useEffect(() => {
    if (!selected) return;
    const controller = new AbortController();
    let url: string | undefined;
    setPreview(undefined);
    setError(undefined);
    void (async () => {
      const scene = normalizeWhiteboardScene(
        await api(path + '/' + selected, { signal: controller.signal }),
      );
      const { files } = await hydrateSceneFiles(scene, {
        fileApi,
        boardId: board.id,
        signal: controller.signal,
      });
      const svg = await exportToSvg({
        elements: scene.elements as never,
        appState: scene.appState,
        files: files as never,
      });
      if (controller.signal.aborted) return;
      url = URL.createObjectURL(
        new Blob([svg.outerHTML], { type: 'image/svg+xml' }),
      );
      setPreview(url);
    })().catch((e) => {
      if (!controller.signal.aborted) setError(String(e));
    });
    return () => {
      controller.abort();
      if (url) URL.revokeObjectURL(url);
    };
  }, [api, fileApi, path, board.id, selected]);
  async function restore() {
    if (!selected || busy || !canRestore) return;
    setBusy(true);
    try {
      await api(path + '/' + selected + '/restore', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ expected_revision: board.revision }),
      });
      await onRestored();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
      title={t('collaboration.history')}
      closeLabel={t('common:actions.close')}
      actions={
        canRestore && (
          <Button disabled={busy || !preview} onClick={() => void restore()}>
            {t('collaboration.restore')}
          </Button>
        )
      }
    >
      {error && <p role="alert">{error}</p>}
      {versions?.length === 0 && <p>{t('collaboration.empty')}</p>}
      {Boolean(versions?.length) && (
        <select
          aria-label={t('collaboration.history')}
          value={selected ?? ''}
          onChange={(e) => setSelected(e.target.value)}
        >
          {versions?.map((item) => (
            <option key={item.id} value={item.id}>
              v{item.revision} · {item.created_at} · {item.author}
            </option>
          ))}
        </select>
      )}
      {preview && (
        <img
          className="max-h-96 max-w-full"
          src={preview}
          alt={t('collaboration.history')}
        />
      )}
    </Dialog>
  );
}
