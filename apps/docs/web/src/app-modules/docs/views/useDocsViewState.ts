import { useState } from 'react';

// AI-DO useDocsViewState without the PMS task links, PMS space choices and
// the link-share state (features not ported).

import type {
  DocsContentFormat,
  DocsHubItem,
  DocsPageItem,
  NativeDocSharingResponse,
  ShareableUserItem,
} from '../api/docs-api';
import type { DropZone } from '../api/docs-page-reorder';

export const useDocsViewState = () => {
  const [editorLoading, setEditorLoading] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [docMenuOpen, setDocMenuOpen] = useState(false);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newDocTitle, setNewDocTitle] = useState('');
  const [newDocLocation, setNewDocLocation] = useState<string>('workspace');
  const [newDocContentFormat, setNewDocContentFormat] =
    useState<DocsContentFormat>('block');
  const [showCreatePageModal, setShowCreatePageModal] = useState(false);
  const [newPageTitle, setNewPageTitle] = useState('');
  const [newPageContentFormat, setNewPageContentFormat] =
    useState<DocsContentFormat>('block');
  const [creating, setCreating] = useState(false);
  const [creatingPage, setCreatingPage] = useState(false);
  const [selectedDoc, setSelectedDoc] = useState<DocsHubItem | null>(null);
  const [pages, setPages] = useState<DocsPageItem[]>([]);
  const [manualExpandedNodes, setManualExpandedNodes] = useState<Set<string>>(
    new Set(),
  );
  const [contentEditorVersions, setContentEditorVersions] = useState<
    Record<string, number>
  >({});
  const [htmlEditPageIds, setHtmlEditPageIds] = useState<Set<string>>(
    new Set(),
  );
  const [readModeOpen, setReadModeOpen] = useState(false);
  const [showShareModal, setShowShareModal] = useState(false);
  const [sharingState, setSharingState] =
    useState<NativeDocSharingResponse | null>(null);
  const [shareableUsers, setShareableUsers] = useState<ShareableUserItem[]>([]);
  const [shareAccessLevel, setShareAccessLevel] = useState<'read' | 'edit'>(
    'read',
  );
  const [shareLoading, setShareLoading] = useState(false);
  const [inviteQuery, setInviteQuery] = useState('');
  const [inviteFocused, setInviteFocused] = useState(false);
  const [linkCopied, setLinkCopied] = useState(false);
  const [copiedDocId, setCopiedDocId] = useState<string | null>(null);
  const [changingLocation, setChangingLocation] = useState(false);
  const [activeDragId, setActiveDragId] = useState<string | null>(null);
  const [dropIndicator, setDropIndicator] = useState<{
    overId: string;
    zone: DropZone;
  } | null>(null);

  return {
    activeDragId,
    changingLocation,
    contentEditorVersions,
    copiedDocId,
    creating,
    creatingPage,
    docMenuOpen,
    dropIndicator,
    editorLoading,
    htmlEditPageIds,
    inviteFocused,
    inviteQuery,
    linkCopied,
    manualExpandedNodes,
    menuOpenId,
    newDocContentFormat,
    newDocLocation,
    newDocTitle,
    newPageContentFormat,
    newPageTitle,
    pages,
    readModeOpen,
    searchOpen,
    selectedDoc,
    setActiveDragId,
    setChangingLocation,
    setContentEditorVersions,
    setCopiedDocId,
    setCreating,
    setCreatingPage,
    setDocMenuOpen,
    setDropIndicator,
    setEditorLoading,
    setHtmlEditPageIds,
    setInviteFocused,
    setInviteQuery,
    setLinkCopied,
    setManualExpandedNodes,
    setMenuOpenId,
    setNewDocContentFormat,
    setNewDocLocation,
    setNewDocTitle,
    setNewPageContentFormat,
    setNewPageTitle,
    setPages,
    setReadModeOpen,
    setSearchOpen,
    setSelectedDoc,
    setShareAccessLevel,
    setShareLoading,
    setShareableUsers,
    setSharingState,
    setShowCreateModal,
    setShowCreatePageModal,
    setShowShareModal,
    shareAccessLevel,
    shareLoading,
    shareableUsers,
    sharingState,
    showCreateModal,
    showCreatePageModal,
    showShareModal,
  };
};
