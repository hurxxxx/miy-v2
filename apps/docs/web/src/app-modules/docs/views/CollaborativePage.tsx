import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { EditingRoom, type RoomStatus } from '../../../../../sdk/collab/client';
import {
  BlockEditor,
  BlockViewer,
  type BlockContent,
} from '../../../components/editor';
import { useRuntime } from '../../../platform/runtime/runtime-context';
import { useNavigationDirty } from '../../../platform/runtime/navigation-state';
import type { DocsPageItem } from '../api/docs-api';

interface Props {
  page: DocsPageItem;
  uploadFile?: (file: File) => Promise<string>;
  resolveFileUrl?: (url: string) => Promise<string>;
  onLiveChange: (
    pageId: string,
    update: Partial<Pick<DocsPageItem, 'content_blocks' | 'revision'>>,
  ) => void;
  onFinished: () => void;
}

export function CollaborativePage({
  page,
  uploadFile,
  resolveFileUrl,
  onFinished,
  onLiveChange,
}: Props) {
  const runtime = useRuntime();
  const { t } = useTranslation('apps');
  const room = useRef<EditingRoom | null>(null);
  const [status, setStatus] = useState<RoomStatus>();
  const [ending, setEnding] = useState(false);
  const terminal =
    status?.phase === 'revoked' ||
    status?.phase === 'replaced' ||
    status?.phase === 'rejected';
  useNavigationDirty(Boolean(status?.dirty && !terminal));
  useEffect(
    () => () => {
      room.current?.destroy();
      room.current = null;
    },
    [],
  );

  const onContent = useCallback(
    (blocks: BlockContent) => {
      onLiveChange(page.id, {
        content_blocks: blocks as DocsPageItem['content_blocks'],
      });
    },
    [page.id, onLiveChange],
  );

  function start() {
    if (room.current) return;
    let revision = page.revision;
    const session = new EditingRoom(
      runtime,
      '/v1/collaboration/' + encodeURIComponent(page.id) + '/session',
      (next) => {
        setStatus(next);
        if (next.revision !== revision) {
          revision = next.revision;
          onLiveChange(page.id, { revision });
        }
      },
    );
    room.current = session;
    setStatus({
      ready: false,
      dirty: false,
      phase: 'connecting',
      revision: page.revision,
    });
    session.start();
  }

  async function finish() {
    const session = room.current;
    if (!session || ending) return;
    setEnding(true);
    try {
      if (!terminal && (status?.ready || status?.dirty)) await session.end();
      session.destroy();
      room.current = null;
      setStatus(undefined);
      onFinished();
    } catch {
      setStatus((s) => s && { ...s, phase: 'failed' });
    } finally {
      setEnding(false);
    }
  }

  const session = room.current;
  return (
    <div className="space-y-3">
      <div className="not-prose flex items-center justify-end gap-3">
        {status && (
          <span role="status">{t('collaboration.' + status.phase)}</span>
        )}
        <button
          type="button"
          className="rounded-md border border-app-border px-3 py-1.5"
          disabled={ending}
          onClick={() => (session ? void finish() : start())}
        >
          {t(session ? 'collaboration.end' : 'collaboration.start')}
        </button>
      </div>
      {session && status?.ready ? (
        <BlockEditor
          onChange={onContent}
          collaboration={{
            fragment: session.doc.getXmlFragment('prosemirror'),
            provider: session,
            user: session.awareness.getLocalState()?.user,
          }}
          editable={!terminal && !ending}
          uploadFile={uploadFile}
          resolveFileUrl={resolveFileUrl}
        />
      ) : (
        <BlockViewer
          content={(page.content_blocks ?? []) as BlockContent}
          resolveFileUrl={resolveFileUrl}
        />
      )}
    </div>
  );
}
