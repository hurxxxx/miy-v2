import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useTabEditing } from '../../../../../sdk/collab/tab-editing';
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
  const inFlight = useRef<Promise<boolean> | null>(null);
  const departure = useTabEditing(runtime.activity, () => Boolean(room.current), () => finish(false), onFinished);
  useNavigationDirty(Boolean(status?.dirty || departure.unconfirmed));
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
    if (room.current || runtime.activity?.getSnapshot() === false || departure.pending) return;
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

  function finish(manual: boolean): Promise<boolean> {
    const session = room.current;
    if (!session) return Promise.resolve(true);
    if (inFlight.current) return inFlight.current;
    const current = session.snapshot;
    if (terminal && current.dirty && !manual) return Promise.resolve(false);
    setEnding(true);
    const work = async () => {
      try {
        if (!terminal && (current.ready || current.dirty)) await session.end();
        if (room.current !== session) return false;
        session.destroy();
        room.current = null;
        setStatus(undefined);
        if (manual) onFinished();
        return true;
      } catch {
        if (room.current === session) setStatus(s => s && { ...s, phase: 'failed' });
        return false;
      }
    };
    inFlight.current = work().finally(() => {
      inFlight.current = null;
      setEnding(false);
    });
    return inFlight.current;
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
          disabled={ending || departure.pending}
          onClick={() => (session ? void finish(true) : start())}
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
          editable={!terminal && !ending && !departure.pending}
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
