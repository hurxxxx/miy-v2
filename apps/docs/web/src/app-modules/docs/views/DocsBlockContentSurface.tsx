import { CollaborativePage } from './CollaborativePage';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Loader2 } from 'lucide-react';

import {
  BlockEditor,
  BlockViewer,
  type BlockContent,
} from '../../../components/editor';
import type { DocsPageItem } from '../api/docs-api';
import type { DocsSaveState } from './useDocsPageContentSaveController';

interface DocsBlockContentSurfaceProps {
  page: DocsPageItem;
  canEdit: boolean;
  contentEditorVersion?: number;
  uploadFile?: (file: File) => Promise<string>;
  resolveFileUrl?: (url: string) => Promise<string>;
  saveState: DocsSaveState;
  onBlocksChange: (pageId: string, blocks: Record<string, unknown>[]) => void;
  onLiveChange: (pageId: string, update: Partial<Pick<DocsPageItem, 'content_blocks' | 'revision'>>) => void;
  onReloadConflict: (page: DocsPageItem) => void;
  onKeepMine: (page: DocsPageItem) => void;
  onRetrySave: () => void;
}

export function DocsBlockContentSurface({
  page,
  canEdit,
  contentEditorVersion = 0,
  uploadFile,
  resolveFileUrl,
  saveState,
  onBlocksChange,
  onLiveChange,
  onReloadConflict,
  onKeepMine,
  onRetrySave,
}: DocsBlockContentSurfaceProps) {
  const { t } = useTranslation(['apps']);
  const blockContent = (page.content_blocks ?? []) as BlockContent;
  const surfaceKey = `${page.id}:${contentEditorVersion}`;
  const forThisPage = saveState.pageId === page.id;

  if (!canEdit) {
    return (
      <BlockViewer
        key={surfaceKey}
        content={blockContent}
        resolveFileUrl={resolveFileUrl}
      />
    );
  }

  if (page.realtime_collab) {
    return <CollaborativePage key={surfaceKey} page={page} onLiveChange={onLiveChange} uploadFile={uploadFile} resolveFileUrl={resolveFileUrl} onFinished={() => onReloadConflict(page)} />;
  }

  return (
    <div className="space-y-3">
      {forThisPage && saveState.status === 'conflict' ? (
        <div
          role="alert"
          className="not-prose flex flex-wrap items-center justify-between gap-3 rounded-md border border-ui-warning/30 bg-ui-warning/10 px-3 py-2"
        >
          <span className="app-text-body-sm flex items-center gap-2 text-app-ink">
            <AlertTriangle size={15} className="shrink-0 text-ui-warning" />
            {t('apps:docsApp.conflict.message')}
          </span>
          <span className="flex gap-2">
            <button
              type="button"
              onClick={() => onReloadConflict(page)}
              className="app-text-control-sm rounded-md border border-app-border bg-app-bg px-3 py-1.5 text-app-ink hover:bg-app-surface-hover"
            >
              {t('apps:docsApp.conflict.reload')}
            </button>
            <button
              type="button"
              onClick={() => onKeepMine(page)}
              className="app-text-control-sm rounded-md bg-app-accent px-3 py-1.5 text-app-accent-fg hover:opacity-90"
            >
              {t('apps:docsApp.conflict.keepMine')}
            </button>
          </span>
        </div>
      ) : null}
      {forThisPage && saveState.status === 'failed' ? (
        <div
          role="alert"
          className="not-prose flex flex-wrap items-center justify-between gap-3 rounded-md border border-app-danger/40 bg-app-danger/10 px-3 py-2"
        >
          <span className="app-text-body-sm text-app-danger">
            {t('apps:docsApp.save.failed')}
          </span>
          <button
            type="button"
            onClick={onRetrySave}
            className="app-text-control-sm rounded-md border border-app-border bg-app-bg px-3 py-1.5 text-app-ink hover:bg-app-surface-hover"
          >
            {t('apps:docsApp.save.retry')}
          </button>
        </div>
      ) : null}
      <div className="not-prose flex h-4 justify-end">
        {forThisPage && (saveState.status === 'saving' || saveState.status === 'pending') ? (
          <span className="app-text-micro inline-flex items-center gap-1 text-app-ink/55">
            <Loader2 size={11} className="animate-spin" />
            {t('apps:docsApp.save.saving')}
          </span>
        ) : null}
      </div>
      <BlockEditor
        key={surfaceKey}
        initialContent={blockContent}
        placeholder={t('apps:docs.startWriting')}
        uploadFile={uploadFile}
        resolveFileUrl={resolveFileUrl}
        onChange={(content) =>
          onBlocksChange(page.id, content as Record<string, unknown>[])
        }
      />
    </div>
  );
}
