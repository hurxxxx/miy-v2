import { useEffect, useMemo, useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { cn } from '../../lib/utils';
import { useRuntime } from '../../platform/runtime/runtime-context';
import { browseFiles, type FileFolderItem } from './api/files-api';
import { FolderTreeRow } from './folder-tree-row';
import { buildFolderTreeModel } from './folder-tree-model';
import { FILES_CHANGED_EVENT } from './file-upload-session';

/**
 * AI-DO sidebar-folders.tsx: the space's folder tree, reloaded on
 * `files:changed`. AI-DO drew it with `@pierre/trees` (a shadow-DOM web
 * component) when the folder paths were unique and fell back to its own
 * `FolderTreeRow` list otherwise; drive always uses that list
 * (docs/PORTING.md §3.10). A late answer for another space is ignored.
 */
export function FilesSidebarFolders({
  activeFolderId,
  onOpenFolder,
  rootActive,
  spaceId,
}: {
  activeFolderId: string | null;
  onOpenFolder: (folderId: string | null) => void;
  rootActive: boolean;
  spaceId: string | null;
}) {
  const { t } = useTranslation('apps');
  const { api } = useRuntime();
  const [state, setState] = useState<{
    spaceId: string;
    folders: FileFolderItem[];
  } | null>(null);

  useEffect(() => {
    if (!spaceId) return;
    let pending: AbortController | undefined;
    async function load(id: string) {
      pending?.abort();
      pending = new AbortController();
      const { signal } = pending;
      try {
        const response = await browseFiles({ api }, id, null, { signal });
        if (!signal.aborted) {
          setState({ spaceId: id, folders: response.all_folders });
        }
      } catch {
        if (!signal.aborted) {
          setState({ spaceId: id, folders: [] });
        }
      }
    }
    const handler = (event: Event) => {
      if ((event as CustomEvent<{ spaceId?: string }>).detail?.spaceId === spaceId) {
        void load(spaceId);
      }
    };
    void load(spaceId);
    window.addEventListener(FILES_CHANGED_EVENT, handler);
    return () => {
      pending?.abort();
      window.removeEventListener(FILES_CHANGED_EVENT, handler);
    };
  }, [api, spaceId]);

  const folders = state?.spaceId === spaceId ? state.folders : [];
  const folderTreeModel = useMemo(
    () => buildFolderTreeModel(folders),
    [folders],
  );
  if (!spaceId) {
    return null;
  }

  return (
    <div className="mt-2 flex min-h-0 flex-1 flex-col gap-1 border-t border-app-border pt-2">
      <span className="sidebar-section-label block px-3 py-1 text-app-ink/55">
        {t('files.sidebar.folders')}
      </span>
      <button
        type="button"
        onClick={() => onOpenFolder(null)}
        aria-current={rootActive ? 'page' : undefined}
        className={cn(
          'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
          rootActive && 'sidebar-submenu-item-active',
        )}
      >
        <FolderOpen size={14} className="text-app-ink/55" />
        <span className="sidebar-submenu-label truncate">
          {t('files.sidebar.root')}
        </span>
      </button>
      {folderTreeModel.tree.length > 0 ? (
        <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto">
          {folderTreeModel.tree.map((folder) => (
            <FolderTreeRow
              key={folder.id}
              activeFolderId={activeFolderId}
              folder={folder}
              level={0}
              onOpen={onOpenFolder}
            />
          ))}
        </div>
      ) : (
        <div className="px-3 py-2 text-center">
          <span className="app-text-micro text-app-ink/70">
            {t('files.sidebar.empty')}
          </span>
        </div>
      )}
    </div>
  );
}
