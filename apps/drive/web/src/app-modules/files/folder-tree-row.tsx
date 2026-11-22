import { Folder } from 'lucide-react';

import { cn } from '../../lib/utils';
import type { FolderNode } from './folder-tree-model';

/**
 * AI-DO folder-tree-row.tsx: a react-router `Link` per folder. The frame has
 * no router; the row navigates with `runtime.navigate` (`onOpen`).
 */
export function FolderTreeRow({
  activeFolderId,
  folder,
  level,
  onOpen,
}: {
  activeFolderId: string | null;
  folder: FolderNode;
  level: number;
  onOpen: (folderId: string) => void;
}) {
  return (
    <>
      <button
        type="button"
        onClick={() => onOpen(folder.id)}
        aria-current={activeFolderId === folder.id ? 'page' : undefined}
        className={cn(
          'sidebar-submenu-item ml-1 w-[calc(100%-0.25rem)] text-left',
          activeFolderId === folder.id && 'sidebar-submenu-item-active',
        )}
        style={{ paddingLeft: `${12 + level * 12}px` }}
      >
        <Folder size={14} className="text-app-ink/55" />
        <span className="sidebar-submenu-label truncate">{folder.name}</span>
      </button>
      {folder.children.map((child) => (
        <FolderTreeRow
          key={child.id}
          activeFolderId={activeFolderId}
          folder={child}
          level={level + 1}
          onOpen={onOpen}
        />
      ))}
    </>
  );
}
